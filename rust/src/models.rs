#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AncMode {
    #[default]
    Off,
    Transparent,
    Normal,
    Deep,
    Experiment,
    WindSuppression,
    Unknown(u8),
}

impl AncMode {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Transparent,
        Self::Normal,
        Self::Deep,
        Self::Experiment,
        Self::WindSuppression,
    ];

    pub const fn value(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::Transparent => 0x01,
            Self::Normal => 0x02,
            Self::Deep => 0x03,
            Self::Experiment => 0x10,
            Self::WindSuppression => 0x11,
            Self::Unknown(value) => value,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "关闭",
            Self::Transparent => "通透",
            Self::Normal => "普通降噪",
            Self::Deep => "深度降噪",
            Self::Experiment => "试验性降噪",
            Self::WindSuppression => "风噪抑制",
            Self::Unknown(_) => "未知模式",
        }
    }
}

impl From<u8> for AncMode {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::Off,
            0x01 => Self::Transparent,
            0x02 => Self::Normal,
            0x03 => Self::Deep,
            0x10 => Self::Experiment,
            0x11 => Self::WindSuppression,
            value => Self::Unknown(value),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EqMode {
    Blue,
    #[default]
    Balanced,
    Bass,
    Pure,
    Game,
    Fine,
    Vocal,
    Unknown(u8),
}

impl EqMode {
    pub const ALL: [Self; 7] = [
        Self::Blue,
        Self::Balanced,
        Self::Bass,
        Self::Pure,
        Self::Game,
        Self::Fine,
        Self::Vocal,
    ];

    pub const fn value(self) -> u8 {
        match self {
            Self::Blue => 0x00,
            Self::Balanced => 0x01,
            Self::Bass => 0x02,
            Self::Pure => 0x03,
            Self::Game => 0x04,
            Self::Fine => 0x05,
            Self::Vocal => 0x06,
            Self::Unknown(value) => value,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Blue => "悔恨之泪",
            Self::Balanced => "均衡中正",
            Self::Bass => "欧美澎湃",
            Self::Pure => "真律还原",
            Self::Game => "游戏优化",
            Self::Fine => "细腻佳音",
            Self::Vocal => "温婉人声",
            Self::Unknown(_) => "未知模式",
        }
    }
}

impl From<u8> for EqMode {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::Blue,
            0x01 => Self::Balanced,
            0x02 => Self::Bass,
            0x03 => Self::Pure,
            0x04 => Self::Game,
            0x05 => Self::Fine,
            0x06 => Self::Vocal,
            value => Self::Unknown(value),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecMode {
    Aac,
    Lhdc,
    Sbc,
}

impl CodecMode {
    pub const ALL: [Self; 3] = [Self::Aac, Self::Lhdc, Self::Sbc];

    pub const fn value(self) -> u8 {
        match self {
            Self::Aac => 0x00,
            Self::Lhdc => 0x01,
            Self::Sbc => 0x02,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Aac => "AAC",
            Self::Lhdc => "LHDC",
            Self::Sbc => "SBC",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FirmwareVersion {
    pub main: Option<u8>,
    pub sub: Option<u8>,
}

impl FirmwareVersion {
    pub const fn new(main: u8, sub: u8) -> Self {
        Self {
            main: Some(main),
            sub: Some(sub),
        }
    }

    pub const fn known(self) -> bool {
        self.main.is_some() && self.sub.is_some()
    }

    pub const fn supports_extended_eq(self) -> bool {
        matches!(self.sub, Some(sub) if sub >= 8)
    }

    pub const fn supports_modern_codec_switch(self) -> bool {
        matches!(self.sub, Some(sub) if sub >= 8)
    }

    pub fn display(self) -> String {
        match (self.main, self.sub) {
            (Some(main), Some(sub)) => format!("{main}.{sub}"),
            _ => "未知".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceState {
    pub connection_state: ConnectionState,
    pub device_name: String,
    pub port_name: Option<String>,
    pub firmware: FirmwareVersion,
    pub anc_mode: AncMode,
    pub eq_mode: EqMode,
    pub left_battery: Option<u8>,
    pub right_battery: Option<u8>,
    pub case_battery: Option<u8>,
    pub selected_codec: Option<CodecMode>,
    pub game_mode_enabled: Option<bool>,
    pub low_latency_enabled: Option<bool>,
    pub last_error: Option<String>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            connection_state: ConnectionState::Disconnected,
            device_name: "未连接".to_owned(),
            port_name: None,
            firmware: FirmwareVersion::default(),
            anc_mode: AncMode::default(),
            eq_mode: EqMode::default(),
            left_battery: None,
            right_battery: None,
            case_battery: None,
            selected_codec: None,
            game_mode_enabled: None,
            low_latency_enabled: None,
            last_error: None,
        }
    }
}
