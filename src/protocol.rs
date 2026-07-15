use crate::models::{AncMode, CodecMode, EqMode, FirmwareVersion};

pub const MAGIC: u8 = 0x4e;
const HEADER_LENGTH: usize = 6;
const MIN_PAYLOAD_LENGTH: usize = 3;
const MAX_PAYLOAD_LENGTH: usize = 4096;

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Op {
    Version = 0x0003,
    LegacyCodecLhdc = 0x0004,
    Battery = 0x0005,
    AncSet = 0x0201,
    AncQuery = 0x0101,
    EqSet = 0x0207,
    EqQuery = 0x0107,
    GameModeSet = 0x0208,
    GameModeQuery = 0x0108,
    LowLatencySet = 0x0206,
    LowLatencyQuery = 0x0106,
    DualConnSet = 0x0205,
    DualConnQuery = 0x0105,
    InEarSet = 0x0209,
    InEarQuery = 0x0109,
    Codec = 0x0204,
    WindSuppressionSet = 0x02e1,
    WindSuppressionQuery = 0x01e1,
    FullState = 0x0103,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedMessage {
    pub op_code: u16,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedStateUpdate {
    pub anc_mode: Option<AncMode>,
    pub eq_mode: Option<EqMode>,
    pub left_battery: Option<u8>,
    pub right_battery: Option<u8>,
    pub case_battery: Option<Option<u8>>,
    pub game_mode_enabled: Option<bool>,
    pub low_latency_enabled: Option<bool>,
    pub firmware: Option<FirmwareVersion>,
}

#[derive(Debug, Default)]
pub struct PacketStreamParser {
    buffer: Vec<u8>,
}

impl PacketStreamParser {
    pub fn feed(&mut self, data: &[u8]) -> Vec<ParsedMessage> {
        self.buffer.extend_from_slice(data);
        let mut messages = Vec::new();

        loop {
            let Some(start) = self.buffer.iter().position(|byte| *byte == MAGIC) else {
                self.buffer.clear();
                break;
            };
            if start > 0 {
                self.buffer.drain(..start);
            }
            if self.buffer.len() < HEADER_LENGTH {
                break;
            }

            let payload_length = u16::from_le_bytes([self.buffer[1], self.buffer[2]]) as usize;
            if !(MIN_PAYLOAD_LENGTH..=MAX_PAYLOAD_LENGTH).contains(&payload_length) {
                self.buffer.remove(0);
                continue;
            }

            let packet_length = payload_length + 3;
            if self.buffer.len() < packet_length {
                break;
            }

            let op_code = u16::from_le_bytes([self.buffer[4], self.buffer[5]]);
            let payload = self.buffer[HEADER_LENGTH..packet_length].to_vec();
            self.buffer.drain(..packet_length);
            messages.push(ParsedMessage { op_code, payload });
        }

        messages
    }
}

pub fn build_command(op_code: u16, params: &[u8]) -> Vec<u8> {
    let payload_length = MIN_PAYLOAD_LENGTH + params.len();
    let mut packet = Vec::with_capacity(payload_length + 3);
    packet.push(MAGIC);
    packet.extend_from_slice(&(payload_length as u16).to_le_bytes());
    packet.push(0);
    packet.extend_from_slice(&op_code.to_le_bytes());
    packet.extend_from_slice(params);
    packet
}

pub fn query_firmware() -> Vec<u8> {
    build_command(Op::Version as u16, &[])
}

pub fn query_battery() -> Vec<u8> {
    build_command(Op::Battery as u16, &[])
}

pub fn query_anc() -> Vec<u8> {
    build_command(Op::AncQuery as u16, &[])
}

pub fn set_anc(mode: AncMode) -> Vec<u8> {
    build_command(Op::AncSet as u16, &[mode.value(), 0])
}

pub fn query_eq() -> Vec<u8> {
    build_command(Op::EqQuery as u16, &[])
}

pub fn set_eq(mode: EqMode) -> Vec<u8> {
    build_command(Op::EqSet as u16, &[mode.value()])
}

pub fn set_codec(mode: CodecMode) -> Vec<u8> {
    build_command(Op::Codec as u16, &[mode.value()])
}

pub fn set_legacy_codec_lhdc(enabled: bool) -> Vec<u8> {
    build_command(Op::LegacyCodecLhdc as u16, &[u8::from(enabled)])
}

pub fn query_game_mode() -> Vec<u8> {
    build_command(Op::GameModeQuery as u16, &[])
}

pub fn set_game_mode(enabled: bool) -> Vec<u8> {
    build_command(Op::GameModeSet as u16, &[u8::from(enabled)])
}

pub fn query_low_latency() -> Vec<u8> {
    build_command(Op::LowLatencyQuery as u16, &[])
}

pub fn set_low_latency(enabled: bool) -> Vec<u8> {
    build_command(Op::LowLatencySet as u16, &[u8::from(enabled)])
}

pub fn parse_state_update(message: &ParsedMessage) -> ParsedStateUpdate {
    let payload = &message.payload;
    match message.op_code {
        op if op == Op::Battery as u16 && payload.len() >= 3 => ParsedStateUpdate {
            left_battery: Some(payload[0]),
            right_battery: Some(payload[1]),
            case_battery: Some((payload[2] != 0).then_some(payload[2])),
            ..Default::default()
        },
        op if op == Op::AncQuery as u16 && !payload.is_empty() => ParsedStateUpdate {
            anc_mode: Some(payload[0].into()),
            ..Default::default()
        },
        op if op == Op::EqQuery as u16 && !payload.is_empty() => ParsedStateUpdate {
            eq_mode: Some(payload[0].into()),
            ..Default::default()
        },
        op if op == Op::GameModeQuery as u16 && !payload.is_empty() => ParsedStateUpdate {
            game_mode_enabled: Some(payload[0] == 1),
            ..Default::default()
        },
        op if op == Op::LowLatencyQuery as u16 && !payload.is_empty() => ParsedStateUpdate {
            low_latency_enabled: Some(payload[0] == 1),
            ..Default::default()
        },
        op if op == Op::Version as u16 && payload.len() >= 2 => ParsedStateUpdate {
            firmware: Some(FirmwareVersion::new(payload[1], payload[0])),
            ..Default::default()
        },
        _ => ParsedStateUpdate::default(),
    }
}

pub fn paced_startup_queries() -> Vec<Vec<u8>> {
    vec![
        query_firmware(),
        query_battery(),
        query_anc(),
        query_eq(),
        query_game_mode(),
        query_low_latency(),
    ]
}
