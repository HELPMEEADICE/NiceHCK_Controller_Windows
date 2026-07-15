use std::cmp::Ordering;

use super::TransportError;

pub const RFCOMM_SERVICE_UUID: &str = "0000a100-1000-8000-4e48-434b4354524c";
pub const DEFAULT_PATTERNS: [&str; 4] = ["YUANDAO", "OriG", "NiceHCK", "Controller"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportKind {
    Rfcomm,
    Serial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfcommCandidate {
    pub device_name: String,
    pub device_id: String,
    pub address: String,
    pub service_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerialCandidate {
    pub device_name: String,
    pub port_name: String,
    pub description: String,
    pub is_bluetooth: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Candidate {
    Rfcomm(RfcommCandidate),
    Serial(SerialCandidate),
}

impl Candidate {
    pub fn kind(&self) -> TransportKind {
        match self {
            Self::Rfcomm(_) => TransportKind::Rfcomm,
            Self::Serial(_) => TransportKind::Serial,
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::Rfcomm(candidate) => format!(
                "RFCOMM | {} | {} | service={} | id={}",
                candidate.device_name,
                candidate.address,
                if candidate.service_name.is_empty() {
                    RFCOMM_SERVICE_UUID
                } else {
                    &candidate.service_name
                },
                candidate.device_id
            ),
            Self::Serial(candidate) => format!(
                "SERIAL | {} | {} | bluetooth={} | {}",
                candidate.port_name,
                candidate.device_name,
                candidate.is_bluetooth,
                candidate.description
            ),
        }
    }

    fn searchable_text(&self) -> String {
        match self {
            Self::Rfcomm(candidate) => format!(
                "{} {} {} {}",
                candidate.device_name,
                candidate.device_id,
                candidate.address,
                candidate.service_name
            ),
            Self::Serial(candidate) => format!(
                "{} {} {}",
                candidate.device_name, candidate.port_name, candidate.description
            ),
        }
    }
}

pub fn candidate_matches(candidate: &Candidate, patterns: &[&str]) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let text = candidate.searchable_text().to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| text.contains(&pattern.to_ascii_lowercase()))
}

pub fn rank_candidates(candidates: &mut [Candidate], patterns: &[&str]) {
    candidates.sort_by(|left, right| compare_candidates(left, right, patterns));
}

fn compare_candidates(left: &Candidate, right: &Candidate, patterns: &[&str]) -> Ordering {
    candidate_rank(left, patterns)
        .cmp(&candidate_rank(right, patterns))
        .then_with(|| left.description().cmp(&right.description()))
}

fn candidate_rank(candidate: &Candidate, patterns: &[&str]) -> (u8, usize, u8) {
    let text = candidate.searchable_text().to_ascii_lowercase();
    let pattern_rank = patterns
        .iter()
        .position(|pattern| text.contains(&pattern.to_ascii_lowercase()))
        .unwrap_or(patterns.len());
    match candidate {
        Candidate::Rfcomm(_) => (0, pattern_rank, 0),
        Candidate::Serial(serial) => (1, pattern_rank, u8::from(!serial.is_bluetooth)),
    }
}

pub fn extract_bluetooth_address(device_id: &str) -> Option<String> {
    let chars: Vec<char> = device_id.chars().collect();
    let mut delimited_match = None;

    for window in chars.windows(17) {
        let separator = window[2];
        if (separator == ':' || separator == '-')
            && [0, 1, 3, 4, 6, 7, 9, 10, 12, 13, 15, 16]
                .iter()
                .all(|index| window[*index].is_ascii_hexdigit())
            && [2, 5, 8, 11, 14]
                .iter()
                .all(|index| window[*index] == separator)
        {
            let raw: String = window.iter().filter(|value| **value != separator).collect();
            // Keep the last six groups when a preceding word happens to end in
            // two hexadecimal characters, for example "device-aa-bb-...".
            delimited_match = Some(format_address(&raw));
        }
    }
    if delimited_match.is_some() {
        return delimited_match;
    }

    for (start, window) in chars.windows(12).enumerate() {
        let bounded_left = start == 0 || !chars[start - 1].is_ascii_hexdigit();
        let end = start + 12;
        let bounded_right = end == chars.len() || !chars[end].is_ascii_hexdigit();
        if bounded_left && bounded_right && window.iter().all(|value| value.is_ascii_hexdigit()) {
            return Some(format_address(&window.iter().collect::<String>()));
        }
    }
    None
}

fn format_address(raw: &str) -> String {
    raw.as_bytes()
        .chunks(2)
        .map(|pair| String::from_utf8_lossy(pair).to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(":")
}

pub async fn discover() -> Result<Vec<Candidate>, TransportError> {
    discover_with_patterns(&DEFAULT_PATTERNS).await
}

#[cfg(not(windows))]
async fn discover_with_patterns(_patterns: &[&str]) -> Result<Vec<Candidate>, TransportError> {
    Err(TransportError::UnsupportedPlatform)
}

#[cfg(windows)]
async fn discover_with_patterns(patterns: &[&str]) -> Result<Vec<Candidate>, TransportError> {
    let (rfcomm, serial) = tokio::join!(discover_rfcomm(patterns), discover_serial());
    let mut candidates = Vec::new();
    let mut failures = Vec::new();
    match rfcomm {
        Ok(found) => candidates.extend(found),
        Err(error) => failures.push(format!("RFCOMM discovery failed: {error}")),
    }
    match serial {
        Ok(found) => candidates.extend(found),
        Err(error) => failures.push(format!("serial discovery failed: {error}")),
    }
    if candidates.is_empty() && !failures.is_empty() {
        return Err(TransportError::Worker(failures.join("; ")));
    }
    rank_candidates(&mut candidates, patterns);
    Ok(candidates)
}

#[cfg(windows)]
async fn discover_rfcomm(patterns: &[&str]) -> Result<Vec<Candidate>, TransportError> {
    use windows::Devices::Bluetooth::Rfcomm::RfcommServiceId;
    use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothDevice};
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::core::{GUID, HSTRING};

    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
    let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.await?;
    let devices = infos
        .into_iter()
        .map(|info| Ok((info.Name()?.to_string(), info.Id()?.to_string())))
        .collect::<Result<Vec<_>, windows::core::Error>>()?;
    let service_id =
        RfcommServiceId::FromUuid(GUID::from_u128(0x0000a100_1000_8000_4e48_434b4354524c))?;
    let mut candidates = Vec::new();

    for (device_name, device_id) in devices {
        let probe = Candidate::Rfcomm(RfcommCandidate {
            device_name: device_name.clone(),
            device_id: device_id.clone(),
            address: extract_bluetooth_address(&device_id).unwrap_or_else(|| device_id.clone()),
            service_name: String::new(),
        });
        if !candidate_matches(&probe, patterns) {
            continue;
        }

        let device = BluetoothDevice::FromIdAsync(&HSTRING::from(&device_id))?.await?;
        let services = device
            .GetRfcommServicesForIdWithCacheModeAsync(&service_id, BluetoothCacheMode::Uncached)?
            .await?
            .Services()?;
        for service in services {
            candidates.push(Candidate::Rfcomm(RfcommCandidate {
                device_name: device_name.clone(),
                device_id: device_id.clone(),
                address: extract_bluetooth_address(&device_id).unwrap_or_else(|| device_id.clone()),
                service_name: service.ConnectionServiceName()?.to_string(),
            }));
        }
    }
    Ok(candidates)
}

#[cfg(windows)]
async fn discover_serial() -> Result<Vec<Candidate>, TransportError> {
    use serialport::SerialPortType;
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Devices::SerialCommunication::SerialDevice;

    let ports = serialport::available_ports()?;
    let metadata: Vec<(String, String)> = async {
        let selector = SerialDevice::GetDeviceSelector()?;
        let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.await?;
        infos
            .into_iter()
            .map(|info| Ok((info.Name()?.to_string(), info.Id()?.to_string())))
            .collect::<Result<_, windows::core::Error>>()
    }
    .await
    .unwrap_or_default();

    Ok(ports
        .into_iter()
        .map(|port| {
            let windows_metadata = metadata.iter().find(|(name, id)| {
                contains_port_name(name, &port.port_name) || contains_port_name(id, &port.port_name)
            });
            let (device_name, description, is_bluetooth) = match port.port_type {
                SerialPortType::BluetoothPort => (
                    port.port_name.clone(),
                    "Bluetooth serial port".to_owned(),
                    true,
                ),
                SerialPortType::UsbPort(info) => {
                    let name = info
                        .product
                        .clone()
                        .or(info.manufacturer.clone())
                        .unwrap_or_else(|| port.port_name.clone());
                    let description = format!(
                        "USB VID={:04X} PID={:04X} manufacturer={} product={} serial={}",
                        info.vid,
                        info.pid,
                        info.manufacturer.as_deref().unwrap_or("-"),
                        info.product.as_deref().unwrap_or("-"),
                        info.serial_number.as_deref().unwrap_or("-")
                    );
                    (name, description, false)
                }
                SerialPortType::PciPort => {
                    (port.port_name.clone(), "PCI serial port".to_owned(), false)
                }
                SerialPortType::Unknown => (
                    port.port_name.clone(),
                    "Unknown serial port".to_owned(),
                    false,
                ),
            };
            let (device_name, description, is_bluetooth) = match windows_metadata {
                Some((name, id)) => {
                    let text = format!("{name} {id}").to_ascii_lowercase();
                    (
                        name.clone(),
                        format!("{description} | windows_id={id}"),
                        is_bluetooth
                            || ["bluetooth", "bthenum", "bthmodem", "rfcomm"]
                                .iter()
                                .any(|token| text.contains(token)),
                    )
                }
                None => (device_name, description, is_bluetooth),
            };
            Candidate::Serial(SerialCandidate {
                device_name,
                port_name: port.port_name,
                description,
                is_bluetooth,
            })
        })
        .filter(|candidate| {
            candidate_matches(candidate, &DEFAULT_PATTERNS)
                || matches!(candidate, Candidate::Serial(serial) if serial.is_bluetooth)
        })
        .collect())
}

#[cfg(windows)]
fn contains_port_name(text: &str, port_name: &str) -> bool {
    let text = text.to_ascii_uppercase();
    let port_name = port_name.to_ascii_uppercase();
    text.match_indices(&port_name).any(|(start, _)| {
        let end = start + port_name.len();
        let left_bound = start == 0 || !text.as_bytes()[start - 1].is_ascii_alphanumeric();
        let right_bound = end == text.len() || !text.as_bytes()[end].is_ascii_alphanumeric();
        left_bound && right_bound
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial(name: &str, port: &str, bluetooth: bool) -> Candidate {
        Candidate::Serial(SerialCandidate {
            device_name: name.to_owned(),
            port_name: port.to_owned(),
            description: "test port".to_owned(),
            is_bluetooth: bluetooth,
        })
    }

    #[test]
    fn matching_is_case_insensitive_and_checks_all_candidate_fields() {
        assert!(candidate_matches(
            &serial("yuanDAO Space", "COM8", true),
            &DEFAULT_PATTERNS
        ));
        assert!(!candidate_matches(
            &serial("Generic headset", "COM9", true),
            &DEFAULT_PATTERNS
        ));
    }

    #[test]
    fn ranking_is_rfcomm_first_then_pattern_and_stable_description() {
        let mut candidates = vec![
            serial("NiceHCK", "COM9", true),
            serial("YUANDAO", "COM8", false),
            Candidate::Rfcomm(RfcommCandidate {
                device_name: "NiceHCK".to_owned(),
                device_id: "id-b".to_owned(),
                address: "BB".to_owned(),
                service_name: "service".to_owned(),
            }),
            Candidate::Rfcomm(RfcommCandidate {
                device_name: "YUANDAO".to_owned(),
                device_id: "id-a".to_owned(),
                address: "AA".to_owned(),
                service_name: "service".to_owned(),
            }),
        ];
        rank_candidates(&mut candidates, &DEFAULT_PATTERNS);
        assert_eq!(candidates[0].kind(), TransportKind::Rfcomm);
        assert!(candidates[0].description().contains("YUANDAO"));
        assert_eq!(candidates[1].kind(), TransportKind::Rfcomm);
        assert!(candidates[2].description().contains("YUANDAO"));
    }

    #[test]
    fn extracts_compact_and_delimited_addresses() {
        assert_eq!(
            extract_bluetooth_address("BTHENUM\\DEV_001A7DDA7113"),
            Some("00:1A:7D:DA:71:13".to_owned())
        );
        assert_eq!(
            extract_bluetooth_address("device-aa-bb-cc-dd-ee-ff-service"),
            Some("AA:BB:CC:DD:EE:FF".to_owned())
        );
        assert_eq!(extract_bluetooth_address("no-address"), None);
    }
}
