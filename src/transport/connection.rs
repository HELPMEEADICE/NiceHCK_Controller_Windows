use std::thread::JoinHandle;

use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use super::Candidate;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport support is available only on Windows")]
    UnsupportedPlatform,
    #[error("connection is closed")]
    Disconnected,
    #[error("serial transport error: {0}")]
    Serial(#[from] serialport::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(windows)]
    #[error("Windows transport error: {0}")]
    Windows(#[from] windows::core::Error),
    #[error("transport worker failed: {0}")]
    Worker(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConnectionEvent {
    Data(Vec<u8>),
    Error(String),
    Disconnected,
}

enum Command {
    Send(Vec<u8>),
    Disconnect(oneshot::Sender<()>),
}

pub struct Connection {
    commands: mpsc::UnboundedSender<Command>,
    events: mpsc::UnboundedReceiver<ConnectionEvent>,
    worker: Option<JoinHandle<()>>,
}

impl Connection {
    pub async fn send(&self, data: impl Into<Vec<u8>>) -> Result<(), TransportError> {
        self.commands
            .send(Command::Send(data.into()))
            .map_err(|_| TransportError::Disconnected)
    }

    pub async fn recv(&mut self) -> Option<ConnectionEvent> {
        self.events.recv().await
    }

    pub async fn disconnect(mut self) -> Result<(), TransportError> {
        let (done_tx, done_rx) = oneshot::channel();
        let sent = self.commands.send(Command::Disconnect(done_tx)).is_ok();
        if sent {
            tokio::time::timeout(std::time::Duration::from_secs(5), done_rx)
                .await
                .map_err(|_| TransportError::Worker("transport shutdown timed out".to_owned()))?
                .map_err(|_| {
                    TransportError::Worker("transport worker stopped during shutdown".to_owned())
                })?;
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| TransportError::Worker("worker thread panicked".to_owned()))?;
        }
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let (done, _) = oneshot::channel();
        let _ = self.commands.send(Command::Disconnect(done));
    }
}

pub async fn connect(candidate: Candidate) -> Result<Connection, TransportError> {
    connect_platform(candidate).await
}

pub async fn connect_first(
    candidates: impl IntoIterator<Item = Candidate>,
) -> Result<(Candidate, Connection), TransportError> {
    let mut candidates: Vec<_> = candidates.into_iter().collect();
    super::rank_candidates(&mut candidates, &super::DEFAULT_PATTERNS);
    let mut failures = Vec::new();
    for candidate in candidates {
        match connect(candidate.clone()).await {
            Ok(connection) => return Ok((candidate, connection)),
            Err(error) => failures.push(format!("{}: {error}", candidate.description())),
        }
    }
    Err(TransportError::Worker(if failures.is_empty() {
        "no transport candidates were supplied".to_owned()
    } else {
        format!("all connection attempts failed: {}", failures.join("; "))
    }))
}

#[cfg(not(windows))]
async fn connect_platform(_candidate: Candidate) -> Result<Connection, TransportError> {
    Err(TransportError::UnsupportedPlatform)
}

#[cfg(windows)]
async fn connect_platform(candidate: Candidate) -> Result<Connection, TransportError> {
    match candidate {
        Candidate::Rfcomm(candidate) => connect_rfcomm(candidate).await,
        Candidate::Serial(candidate) => connect_serial(candidate).await,
    }
}

#[cfg(windows)]
async fn connect_serial(candidate: super::SerialCandidate) -> Result<Connection, TransportError> {
    use std::io::{Read, Write};
    use std::time::Duration;
    use tokio::sync::mpsc::error::TryRecvError;

    let (commands_tx, mut commands_rx) = mpsc::unbounded_channel();
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let worker = std::thread::Builder::new()
        .name("nicehck-serial".to_owned())
        .spawn(move || {
            let opened = serialport::new(&candidate.port_name, 115_200)
                .timeout(Duration::from_millis(200))
                .open();
            let mut port = match opened {
                Ok(port) => {
                    let _ = ready_tx.send(Ok(()));
                    port
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(TransportError::Serial(error)));
                    return;
                }
            };
            let mut buffer = [0_u8; 512];
            loop {
                loop {
                    match commands_rx.try_recv() {
                        Ok(Command::Send(data)) => {
                            if let Err(error) = port.write_all(&data).and_then(|_| port.flush()) {
                                let _ = events_tx.send(ConnectionEvent::Error(error.to_string()));
                                let _ = events_tx.send(ConnectionEvent::Disconnected);
                                return;
                            }
                        }
                        Ok(Command::Disconnect(done)) => {
                            drop(port);
                            let _ = events_tx.send(ConnectionEvent::Disconnected);
                            let _ = done.send(());
                            return;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                match port.read(&mut buffer) {
                    Ok(0) => {}
                    Ok(count) => {
                        if events_tx
                            .send(ConnectionEvent::Data(buffer[..count].to_vec()))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(error) => {
                        let _ = events_tx.send(ConnectionEvent::Error(error.to_string()));
                        let _ = events_tx.send(ConnectionEvent::Disconnected);
                        return;
                    }
                }
            }
        })?;

    ready_rx
        .await
        .map_err(|_| TransportError::Worker("serial worker stopped during startup".to_owned()))??;
    Ok(Connection {
        commands: commands_tx,
        events: events_rx,
        worker: Some(worker),
    })
}

#[cfg(windows)]
async fn connect_rfcomm(candidate: super::RfcommCandidate) -> Result<Connection, TransportError> {
    let (commands_tx, commands_rx) = mpsc::unbounded_channel();
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let worker = std::thread::Builder::new()
        .name("nicehck-rfcomm".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_tx.send(Err(TransportError::Io(error)));
                    return;
                }
            };
            runtime.block_on(rfcomm_worker(candidate, commands_rx, events_tx, ready_tx));
        })?;

    ready_rx
        .await
        .map_err(|_| TransportError::Worker("RFCOMM worker stopped during startup".to_owned()))??;
    Ok(Connection {
        commands: commands_tx,
        events: events_rx,
        worker: Some(worker),
    })
}

#[cfg(windows)]
async fn rfcomm_worker(
    candidate: super::RfcommCandidate,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<ConnectionEvent>,
    ready: oneshot::Sender<Result<(), TransportError>>,
) {
    use std::time::Duration;
    use windows::Devices::Bluetooth::Rfcomm::RfcommServiceId;
    use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothDevice};
    use windows::Devices::Enumeration::DeviceAccessStatus;
    use windows::Networking::Sockets::StreamSocket;
    use windows::Storage::Streams::{DataReader, DataWriter, InputStreamOptions};
    use windows::core::{GUID, HSTRING};

    let setup = tokio::time::timeout(Duration::from_secs(5), async {
        let device = BluetoothDevice::FromIdAsync(&HSTRING::from(&candidate.device_id))?.await?;
        let service_id =
            RfcommServiceId::FromUuid(GUID::from_u128(0x0000a100_1000_8000_4e48_434b4354524c))?;
        let services = device
            .GetRfcommServicesForIdWithCacheModeAsync(&service_id, BluetoothCacheMode::Uncached)?
            .await?
            .Services()?;
        if services.Size()? == 0 {
            return Err(TransportError::Worker(format!(
                "target RFCOMM service is unavailable: {}",
                candidate.device_name
            )));
        }
        let service = services.GetAt(0)?;
        let access = service.RequestAccessAsync()?.await?;
        if access != DeviceAccessStatus::Allowed {
            return Err(TransportError::Worker(format!(
                "RFCOMM service access was denied: {access:?}"
            )));
        }
        let socket = StreamSocket::new()?;
        socket
            .ConnectWithProtectionLevelAsync(
                &service.ConnectionHostName()?,
                &service.ConnectionServiceName()?,
                service.ProtectionLevel()?,
            )?
            .await?;
        let reader = DataReader::CreateDataReader(&socket.InputStream()?)?;
        reader.SetInputStreamOptions(InputStreamOptions::Partial)?;
        let writer = DataWriter::CreateDataWriter(&socket.OutputStream()?)?;
        Ok::<_, TransportError>((socket, reader, writer))
    })
    .await;

    let (socket, reader, writer) = match setup {
        Ok(Ok(parts)) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Ok(Err(error)) => {
            let _ = ready.send(Err(error));
            return;
        }
        Err(_) => {
            let _ = ready.send(Err(TransportError::Worker(
                "RFCOMM connection timed out".to_owned(),
            )));
            return;
        }
    };

    let (reader_done_tx, mut reader_done_rx) = oneshot::channel();
    let reader_events = events.clone();
    let reader_task = tokio::spawn(async move {
        loop {
            let loaded = async {
                let operation = reader.LoadAsync(512)?;
                operation.await
            }
            .await;
            match loaded {
                Ok(0) => break,
                Ok(count) => {
                    let mut chunk = vec![0_u8; count as usize];
                    match reader.ReadBytes(&mut chunk) {
                        Ok(()) => {
                            if reader_events.send(ConnectionEvent::Data(chunk)).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = reader_events.send(ConnectionEvent::Error(error.to_string()));
                            break;
                        }
                    }
                }
                Err(error) => {
                    let _ = reader_events.send(ConnectionEvent::Error(error.to_string()));
                    break;
                }
            }
        }
        let _ = reader_done_tx.send(());
    });

    let mut disconnect_done = None;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Send(data)) => {
                    let sent = tokio::time::timeout(Duration::from_secs(3), async {
                        writer.WriteBytes(&data)?;
                        writer.StoreAsync()?.await?;
                        writer.FlushAsync()?.await?;
                        Ok::<(), windows::core::Error>(())
                    }).await;
                    match sent {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            let _ = events.send(ConnectionEvent::Error(error.to_string()));
                            break;
                        }
                        Err(_) => {
                            let _ = events.send(ConnectionEvent::Error(
                                "RFCOMM write timed out".to_owned(),
                            ));
                            break;
                        }
                    }
                }
                Some(Command::Disconnect(done)) => {
                    disconnect_done = Some(done);
                    break;
                }
                None => break,
            },
            _ = &mut reader_done_rx => break,
        }
    }
    if let Ok(cancel) = socket.CancelIOAsync() {
        let _ = cancel.await;
    }
    reader_task.abort();
    let _ = reader_task.await;
    let _ = writer.Close();
    let _ = socket.Close();
    drop(socket);
    let _ = events.send(ConnectionEvent::Disconnected);
    if let Some(done) = disconnect_done {
        let _ = done.send(());
    }
}
