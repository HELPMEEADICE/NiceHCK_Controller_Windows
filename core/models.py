from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class ConnectionState(str, Enum):
    DISCONNECTED = "disconnected"
    CONNECTING = "connecting"
    CONNECTED = "connected"
    ERROR = "error"


class AncMode(Enum):
    OFF = (0x00, "关闭")
    TRANSPARENT = (0x01, "通透")
    NORMAL = (0x02, "普通降噪")
    DEEP = (0x03, "深度降噪")
    EXPERIMENT = (0x10, "试验性降噪")
    WIND_SUPPRESSION = (0x11, "风噪抑制")

    def __init__(self, value: int, label: str) -> None:
        self._value_ = value
        self.label = label

    @classmethod
    def from_value(cls, value: int) -> "AncMode":
        for mode in cls:
            if mode.value == value:
                return mode
        return cls.OFF


class EqMode(Enum):
    BLUE = (0x00, "悔恨之泪")
    BALANCED = (0x01, "均衡中正")
    BASS = (0x02, "欧美澎湃")
    PURE = (0x03, "真律还原")
    GAME = (0x04, "游戏优化")
    FINE = (0x05, "细腻佳音")
    VOCAL = (0x06, "温婉人声")

    def __init__(self, value: int, label: str) -> None:
        self._value_ = value
        self.label = label

    @classmethod
    def from_value(cls, value: int) -> "EqMode":
        for mode in cls:
            if mode.value == value:
                return mode
        return cls.BALANCED


class CodecMode(Enum):
    AAC = (0x00, "AAC")
    LHDC = (0x01, "LHDC")
    SBC = (0x02, "SBC")

    def __init__(self, value: int, label: str) -> None:
        self._value_ = value
        self.label = label


@dataclass(slots=True)
class FirmwareVersion:
    main: int = -1
    sub: int = -1

    @property
    def known(self) -> bool:
        return self.main >= 0 and self.sub >= 0

    @property
    def supports_extended_eq(self) -> bool:
        return self.sub >= 8

    @property
    def supports_modern_codec_switch(self) -> bool:
        return self.sub >= 8

    def display(self) -> str:
        if not self.known:
            return "未知"
        return f"{self.main}.{self.sub}"


@dataclass(slots=True)
class DeviceState:
    connection_state: ConnectionState = ConnectionState.DISCONNECTED
    device_name: str = "未连接"
    port_name: Optional[str] = None
    firmware: FirmwareVersion = field(default_factory=FirmwareVersion)
    anc_mode: AncMode = AncMode.OFF
    eq_mode: EqMode = EqMode.BALANCED
    left_battery: Optional[int] = None
    right_battery: Optional[int] = None
    case_battery: Optional[int] = None
    selected_codec: Optional[CodecMode] = None
    game_mode_enabled: Optional[bool] = None
    low_latency_enabled: Optional[bool] = None
    last_error: Optional[str] = None
