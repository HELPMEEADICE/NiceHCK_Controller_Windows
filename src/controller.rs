use std::time::Duration;

use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, timeout};
use tracing::{error, info};

use crate::models::{AncMode, CodecMode, ConnectionState, DeviceState, EqMode};
use crate::protocol::{
    PacketStreamParser, ParsedStateUpdate, paced_startup_queries, parse_state_update, query_anc,
    query_eq, query_game_mode, query_low_latency, set_anc, set_codec, set_eq, set_game_mode,
    set_legacy_codec_lhdc, set_low_latency,
};
use crate::transport::{self, Candidate, Connection, ConnectionEvent};

const COMMAND_PACING: Duration = Duration::from_millis(100);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Clone, Debug)]
pub enum ControllerCommand {
    Connect,
    Disconnect,
    Refresh,
    SetAnc(AncMode),
    SetEq(EqMode),
    SetCodec(CodecMode),
    SetGameMode(bool),
    SetLowLatency(bool),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControllerEvent {
    State(DeviceState),
    Log(String),
    Error(String),
}

#[derive(Debug, Error)]
pub enum ControllerError {
    #[error("当前未连接耳机")]
    NotConnected,
    #[error("当前固件版本过低，不支持该 EQ 模式")]
    UnsupportedEq,
    #[error("当前固件版本过低，不支持 SBC 编码切换")]
    UnsupportedCodec,
    #[error("设备发现超时")]
    DiscoveryTimeout,
    #[error("未找到匹配的 NiceHCK 蓝牙设备")]
    NoCandidates,
    #[error("所有连接方式均失败：{0}")]
    AllConnectionsFailed(String),
    #[error(transparent)]
    Transport(#[from] transport::TransportError),
}

#[derive(Clone)]
pub struct ControllerHandle {
    commands: mpsc::UnboundedSender<ActorMessage>,
}

impl ControllerHandle {
    pub fn send(&self, command: ControllerCommand) -> Result<(), ControllerError> {
        self.commands
            .send(ActorMessage::Command(command))
            .map_err(|_| transport::TransportError::Disconnected.into())
    }

    pub async fn shutdown(&self) {
        let (done_tx, done_rx) = oneshot::channel();
        if self.commands.send(ActorMessage::Shutdown(done_tx)).is_ok() {
            let _ = done_rx.await;
        }
    }
}

enum ActorMessage {
    Command(ControllerCommand),
    Shutdown(oneshot::Sender<()>),
}

pub fn spawn_controller() -> (ControllerHandle, mpsc::UnboundedReceiver<ControllerEvent>) {
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tokio::spawn(run_actor(command_rx, event_tx));
    (
        ControllerHandle {
            commands: command_tx,
        },
        event_rx,
    )
}

struct ControllerActor {
    state: DeviceState,
    parser: PacketStreamParser,
    connection: Option<Connection>,
    events: mpsc::UnboundedSender<ControllerEvent>,
}

async fn run_actor(
    mut commands: mpsc::UnboundedReceiver<ActorMessage>,
    events: mpsc::UnboundedSender<ControllerEvent>,
) {
    let mut actor = ControllerActor {
        state: DeviceState::default(),
        parser: PacketStreamParser::default(),
        connection: None,
        events,
    };
    actor.emit_state();

    loop {
        tokio::select! {
            message = commands.recv() => match message {
                Some(ActorMessage::Command(command)) => actor.handle_command(command).await,
                Some(ActorMessage::Shutdown(done)) => {
                    actor.disconnect(false).await;
                    let _ = done.send(());
                    break;
                }
                None => {
                    actor.disconnect(false).await;
                    break;
                }
            },
            event = receive_transport_event(&mut actor.connection), if actor.connection.is_some() => {
                actor.handle_transport_event(event).await;
            }
        }
    }
}

async fn receive_transport_event(connection: &mut Option<Connection>) -> Option<ConnectionEvent> {
    match connection {
        Some(connection) => connection.recv().await,
        None => std::future::pending().await,
    }
}

impl ControllerActor {
    async fn handle_command(&mut self, command: ControllerCommand) {
        let result = match command {
            ControllerCommand::Connect => self.connect().await,
            ControllerCommand::Disconnect => {
                self.disconnect(true).await;
                Ok(())
            }
            ControllerCommand::Refresh => self.send_startup_queries().await,
            ControllerCommand::SetAnc(mode) => self.set_anc(mode).await,
            ControllerCommand::SetEq(mode) => self.set_eq(mode).await,
            ControllerCommand::SetCodec(mode) => self.set_codec(mode).await,
            ControllerCommand::SetGameMode(enabled) => self.set_game_mode(enabled).await,
            ControllerCommand::SetLowLatency(enabled) => self.set_low_latency(enabled).await,
        };
        if let Err(error) = result {
            self.emit_error(error.to_string());
        }
    }

    async fn connect(&mut self) -> Result<(), ControllerError> {
        if self.connection.is_some() {
            self.disconnect(false).await;
        }
        self.state.connection_state = ConnectionState::Connecting;
        self.state.last_error = None;
        self.emit_state();
        self.emit_log("正在搜索已配对的 NiceHCK 设备".to_owned());

        let candidates = timeout(DISCOVERY_TIMEOUT, transport::discover())
            .await
            .map_err(|_| ControllerError::DiscoveryTimeout)??;
        if candidates.is_empty() {
            self.connection_failed(ControllerError::NoCandidates.to_string());
            return Err(ControllerError::NoCandidates);
        }

        let mut failures = Vec::new();
        for candidate in candidates {
            self.emit_log(format!("尝试连接：{}", candidate.description()));
            match timeout(CONNECTION_TIMEOUT, transport::connect(candidate.clone())).await {
                Ok(Ok(connection)) => {
                    self.finish_connection(candidate, connection).await?;
                    return Ok(());
                }
                Ok(Err(error)) => {
                    let detail = format!("{}：{error}", candidate.description());
                    self.emit_log(format!("连接失败：{detail}"));
                    failures.push(detail);
                }
                Err(_) => {
                    let detail = format!("{}：连接超时", candidate.description());
                    self.emit_log(format!("连接失败：{detail}"));
                    failures.push(detail);
                }
            }
        }

        let detail = failures.join("；");
        self.connection_failed(detail.clone());
        Err(ControllerError::AllConnectionsFailed(detail))
    }

    async fn finish_connection(
        &mut self,
        candidate: Candidate,
        connection: Connection,
    ) -> Result<(), ControllerError> {
        self.state.connection_state = ConnectionState::Connected;
        self.state.device_name = candidate_device_name(&candidate).to_owned();
        self.state.port_name = Some(candidate_endpoint(&candidate));
        self.state.last_error = None;
        self.connection = Some(connection);
        self.emit_state();
        self.emit_log(format!("已连接：{}", candidate.description()));
        self.send_startup_queries().await
    }

    fn connection_failed(&mut self, message: String) {
        self.state.connection_state = ConnectionState::Error;
        self.state.last_error = Some(message);
        self.emit_state();
    }

    async fn disconnect(&mut self, announce: bool) {
        if let Some(connection) = self.connection.take()
            && let Err(error) = connection.disconnect().await
        {
            self.emit_log(format!("断开传输时出错：{error}"));
        }
        self.parser = PacketStreamParser::default();
        self.state = DeviceState::default();
        self.emit_state();
        if announce {
            self.emit_log("已断开连接".to_owned());
        }
    }

    async fn send_startup_queries(&mut self) -> Result<(), ControllerError> {
        self.ensure_connected()?;
        for packet in paced_startup_queries() {
            self.send_packet(packet).await?;
            sleep(COMMAND_PACING).await;
        }
        self.emit_log("已发送状态查询".to_owned());
        Ok(())
    }

    async fn set_anc(&mut self, mode: AncMode) -> Result<(), ControllerError> {
        self.ensure_connected()?;
        self.send_packet(set_anc(mode)).await?;
        sleep(COMMAND_PACING).await;
        self.send_packet(query_anc()).await?;
        self.emit_log(format!("已发送 ANC 切换：{}", mode.label()));
        Ok(())
    }

    async fn set_eq(&mut self, mode: EqMode) -> Result<(), ControllerError> {
        self.ensure_connected()?;
        if !self.state.firmware.supports_extended_eq()
            && matches!(mode, EqMode::Fine | EqMode::Vocal)
        {
            return Err(ControllerError::UnsupportedEq);
        }
        self.send_packet(set_eq(mode)).await?;
        sleep(COMMAND_PACING).await;
        self.send_packet(query_eq()).await?;
        self.emit_log(format!("已发送 EQ 切换：{}", mode.label()));
        Ok(())
    }

    async fn set_codec(&mut self, mode: CodecMode) -> Result<(), ControllerError> {
        self.ensure_connected()?;
        let packet = if self.state.firmware.supports_modern_codec_switch() {
            set_codec(mode)
        } else {
            if mode == CodecMode::Sbc {
                return Err(ControllerError::UnsupportedCodec);
            }
            set_legacy_codec_lhdc(mode == CodecMode::Lhdc)
        };
        self.send_packet(packet).await?;
        self.state.selected_codec = Some(mode);
        self.emit_state();
        self.emit_log(format!("已发送编码切换：{}", mode.label()));
        Ok(())
    }

    async fn set_game_mode(&mut self, enabled: bool) -> Result<(), ControllerError> {
        self.ensure_connected()?;
        self.send_packet(set_game_mode(enabled)).await?;
        sleep(COMMAND_PACING).await;
        self.send_packet(query_game_mode()).await?;
        self.emit_log(format!(
            "已发送游戏模式切换：{}",
            if enabled { "开" } else { "关" }
        ));
        Ok(())
    }

    async fn set_low_latency(&mut self, enabled: bool) -> Result<(), ControllerError> {
        self.ensure_connected()?;
        self.send_packet(set_low_latency(enabled)).await?;
        sleep(COMMAND_PACING).await;
        self.send_packet(query_low_latency()).await?;
        self.emit_log(format!(
            "已发送低延迟切换：{}",
            if enabled { "开" } else { "关" }
        ));
        Ok(())
    }

    fn ensure_connected(&self) -> Result<(), ControllerError> {
        if self.connection.is_some() {
            Ok(())
        } else {
            Err(ControllerError::NotConnected)
        }
    }

    async fn send_packet(&self, packet: Vec<u8>) -> Result<(), ControllerError> {
        let connection = self
            .connection
            .as_ref()
            .ok_or(ControllerError::NotConnected)?;
        connection.send(packet.clone()).await?;
        self.emit_log(format!("发送：{}", format_bytes(&packet)));
        Ok(())
    }

    async fn handle_transport_event(&mut self, event: Option<ConnectionEvent>) {
        match event {
            Some(ConnectionEvent::Data(data)) => {
                self.emit_log(format!("接收：{}", format_bytes(&data)));
                for message in self.parser.feed(&data) {
                    self.apply_update(parse_state_update(&message));
                }
            }
            Some(ConnectionEvent::Error(message)) => {
                self.emit_error(format!("蓝牙通信错误：{message}"));
            }
            Some(ConnectionEvent::Disconnected) | None => {
                self.connection = None;
                self.state.connection_state = ConnectionState::Error;
                self.state.last_error = Some("设备连接已断开".to_owned());
                self.emit_state();
                self.emit_error("设备连接已断开".to_owned());
            }
        }
    }

    fn apply_update(&mut self, update: ParsedStateUpdate) {
        let mut changed = false;
        if let Some(firmware) = update.firmware {
            self.state.firmware = firmware;
            changed = true;
        }
        if let Some(value) = update.left_battery {
            self.state.left_battery = Some(value);
            changed = true;
        }
        if let Some(value) = update.right_battery {
            self.state.right_battery = Some(value);
            changed = true;
        }
        if let Some(value) = update.case_battery {
            self.state.case_battery = value;
            changed = true;
        }
        if let Some(value) = update.anc_mode {
            self.state.anc_mode = value;
            changed = true;
        }
        if let Some(value) = update.eq_mode {
            self.state.eq_mode = value;
            changed = true;
        }
        if let Some(value) = update.game_mode_enabled {
            self.state.game_mode_enabled = Some(value);
            changed = true;
        }
        if let Some(value) = update.low_latency_enabled {
            self.state.low_latency_enabled = Some(value);
            changed = true;
        }
        if changed {
            self.emit_state();
        }
    }

    fn emit_state(&self) {
        let _ = self.events.send(ControllerEvent::State(self.state.clone()));
    }

    fn emit_log(&self, message: String) {
        info!("{message}");
        let _ = self.events.send(ControllerEvent::Log(message));
    }

    fn emit_error(&self, message: String) {
        error!("{message}");
        let _ = self.events.send(ControllerEvent::Error(message));
    }
}

fn candidate_device_name(candidate: &Candidate) -> &str {
    match candidate {
        Candidate::Rfcomm(candidate) => &candidate.device_name,
        Candidate::Serial(candidate) => &candidate.device_name,
    }
}

fn candidate_endpoint(candidate: &Candidate) -> String {
    match candidate {
        Candidate::Rfcomm(candidate) => format!("RFCOMM:{}", candidate.address),
        Candidate::Serial(candidate) => candidate.port_name.clone(),
    }
}

fn format_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FirmwareVersion;

    #[test]
    fn formats_packets_for_diagnostics() {
        assert_eq!(format_bytes(&[0x4e, 0, 0xff]), "4E 00 FF");
    }

    #[test]
    fn applies_explicitly_unknown_case_battery() {
        let (events, _) = mpsc::unbounded_channel();
        let mut actor = ControllerActor {
            state: DeviceState {
                case_battery: Some(50),
                ..DeviceState::default()
            },
            parser: PacketStreamParser::default(),
            connection: None,
            events,
        };
        actor.apply_update(ParsedStateUpdate {
            case_battery: Some(None),
            ..ParsedStateUpdate::default()
        });
        assert_eq!(actor.state.case_battery, None);
    }

    #[test]
    fn firmware_capability_boundary_matches_existing_app() {
        assert!(!FirmwareVersion::new(4, 7).supports_extended_eq());
        assert!(FirmwareVersion::new(4, 8).supports_extended_eq());
    }
}
