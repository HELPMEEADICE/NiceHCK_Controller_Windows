from __future__ import annotations

from dataclasses import dataclass
from enum import IntEnum
from typing import List, Sequence

from .models import AncMode, CodecMode, EqMode, FirmwareVersion


MAGIC = 0x4E


class Op(IntEnum):
    VERSION = 0x0003
    LEGACY_CODEC_LHDC = 0x0004
    BATTERY = 0x0005
    ANC_SET = 0x0201
    ANC_QUERY = 0x0101
    EQ_SET = 0x0207
    EQ_QUERY = 0x0107
    GAME_MODE_SET = 0x0208
    GAME_MODE_QUERY = 0x0108
    LOW_LATENCY_SET = 0x0206
    LOW_LATENCY_QUERY = 0x0106
    DUAL_CONN_SET = 0x0205
    DUAL_CONN_QUERY = 0x0105
    IN_EAR_SET = 0x0209
    IN_EAR_QUERY = 0x0109
    CODEC = 0x0204
    WIND_SUPPRESSION_SET = 0x02E1
    WIND_SUPPRESSION_QUERY = 0x01E1
    FULL_STATE = 0x0103


@dataclass(slots=True)
class ParsedMessage:
    op_code: int
    payload: bytes


@dataclass(slots=True)
class ParsedStateUpdate:
    anc_mode: AncMode | None = None
    eq_mode: EqMode | None = None
    left_battery: int | None = None
    right_battery: int | None = None
    case_battery: int | None = None
    game_mode_enabled: bool | None = None
    low_latency_enabled: bool | None = None
    firmware: FirmwareVersion | None = None


class PacketStreamParser:
    def __init__(self) -> None:
        self._buffer = bytearray()

    def feed(self, data: bytes) -> List[ParsedMessage]:
        self._buffer.extend(data)
        messages: List[ParsedMessage] = []

        while True:
            start = self._find_magic()
            if start < 0:
                self._buffer.clear()
                break
            if start > 0:
                del self._buffer[:start]
            if len(self._buffer) < 6:
                break

            payload_length = self._buffer[1] | (self._buffer[2] << 8)
            packet_length = payload_length + 3
            if len(self._buffer) < packet_length:
                break

            packet = bytes(self._buffer[:packet_length])
            del self._buffer[:packet_length]

            op_code = packet[4] | (packet[5] << 8)
            messages.append(ParsedMessage(op_code=op_code, payload=packet[6:]))

        return messages

    def _find_magic(self) -> int:
        for index, value in enumerate(self._buffer):
            if value == MAGIC:
                return index
        return -1


def build_command(op_code: int, *params: int) -> bytes:
    payload_length = 3 + len(params)
    packet = bytearray(3 + payload_length)
    packet[0] = MAGIC
    packet[1] = payload_length & 0xFF
    packet[2] = (payload_length >> 8) & 0xFF
    packet[3] = 0x00
    packet[4] = op_code & 0xFF
    packet[5] = (op_code >> 8) & 0xFF
    for index, param in enumerate(params, start=6):
        packet[index] = param & 0xFF
    return bytes(packet)


def query_firmware() -> bytes:
    return build_command(Op.VERSION)


def query_battery() -> bytes:
    return build_command(Op.BATTERY)


def query_anc() -> bytes:
    return build_command(Op.ANC_QUERY)


def set_anc(mode: AncMode) -> bytes:
    return build_command(Op.ANC_SET, mode.value, 0x00)


def query_eq() -> bytes:
    return build_command(Op.EQ_QUERY)


def set_eq(mode: EqMode) -> bytes:
    return build_command(Op.EQ_SET, mode.value)


def set_codec(mode: CodecMode) -> bytes:
    return build_command(Op.CODEC, mode.value)


def set_legacy_codec_lhdc(enabled: bool) -> bytes:
    return build_command(Op.LEGACY_CODEC_LHDC, 0x01 if enabled else 0x00)


def query_game_mode() -> bytes:
    return build_command(Op.GAME_MODE_QUERY)


def set_game_mode(enabled: bool) -> bytes:
    return build_command(Op.GAME_MODE_SET, 0x01 if enabled else 0x00)


def query_low_latency() -> bytes:
    return build_command(Op.LOW_LATENCY_QUERY)


def set_low_latency(enabled: bool) -> bytes:
    return build_command(Op.LOW_LATENCY_SET, 0x01 if enabled else 0x00)


def parse_state_update(message: ParsedMessage) -> ParsedStateUpdate:
    payload = message.payload
    if message.op_code == Op.BATTERY and len(payload) >= 3:
        return ParsedStateUpdate(
            left_battery=payload[0],
            right_battery=payload[1],
            case_battery=(payload[2] if payload[2] != 0 else None),
        )
    if message.op_code == Op.ANC_QUERY and len(payload) >= 1:
        return ParsedStateUpdate(anc_mode=AncMode.from_value(payload[0]))
    if message.op_code == Op.EQ_QUERY and len(payload) >= 1:
        return ParsedStateUpdate(eq_mode=EqMode.from_value(payload[0]))
    if message.op_code == Op.GAME_MODE_QUERY and len(payload) >= 1:
        return ParsedStateUpdate(game_mode_enabled=(payload[0] == 0x01))
    if message.op_code == Op.LOW_LATENCY_QUERY and len(payload) >= 1:
        return ParsedStateUpdate(low_latency_enabled=(payload[0] == 0x01))
    if message.op_code == Op.VERSION and len(payload) >= 2:
        return ParsedStateUpdate(firmware=FirmwareVersion(main=payload[1], sub=payload[0]))
    return ParsedStateUpdate()


def paced_startup_queries() -> Sequence[bytes]:
    return (
        query_firmware(),
        query_battery(),
        query_anc(),
        query_eq(),
        query_game_mode(),
        query_low_latency(),
    )
