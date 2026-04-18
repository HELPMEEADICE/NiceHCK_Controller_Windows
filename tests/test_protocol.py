from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from core.models import AncMode, CodecMode, EqMode
from core.protocol import (
    PacketStreamParser,
    ParsedMessage,
    Op,
    paced_startup_queries,
    parse_state_update,
    query_anc,
    query_battery,
    query_eq,
    query_game_mode,
    query_low_latency,
    query_firmware,
    set_anc,
    set_codec,
    set_eq,
    set_game_mode,
    set_legacy_codec_lhdc,
    set_low_latency,
)


def test_build_query_packets() -> None:
    assert query_firmware() == bytes.fromhex("4E 03 00 00 03 00")
    assert query_battery() == bytes.fromhex("4E 03 00 00 05 00")
    assert query_anc() == bytes.fromhex("4E 03 00 00 01 01")
    assert query_eq() == bytes.fromhex("4E 03 00 00 07 01")
    assert query_game_mode() == bytes.fromhex("4E 03 00 00 08 01")
    assert query_low_latency() == bytes.fromhex("4E 03 00 00 06 01")


def test_build_set_packets() -> None:
    assert set_anc(AncMode.TRANSPARENT) == bytes.fromhex("4E 05 00 00 01 02 01 00")
    assert set_eq(EqMode.GAME) == bytes.fromhex("4E 04 00 00 07 02 04")
    assert set_codec(CodecMode.AAC) == bytes.fromhex("4E 04 00 00 04 02 00")
    assert set_codec(CodecMode.LHDC) == bytes.fromhex("4E 04 00 00 04 02 01")
    assert set_codec(CodecMode.SBC) == bytes.fromhex("4E 04 00 00 04 02 02")
    assert set_legacy_codec_lhdc(True) == bytes.fromhex("4E 04 00 00 04 00 01")
    assert set_legacy_codec_lhdc(False) == bytes.fromhex("4E 04 00 00 04 00 00")
    assert set_game_mode(True) == bytes.fromhex("4E 04 00 00 08 02 01")
    assert set_game_mode(False) == bytes.fromhex("4E 04 00 00 08 02 00")
    assert set_low_latency(True) == bytes.fromhex("4E 04 00 00 06 02 01")
    assert set_low_latency(False) == bytes.fromhex("4E 04 00 00 06 02 00")


def test_parser_handles_split_and_sticky_packets() -> None:
    parser = PacketStreamParser()
    chunk1 = bytes.fromhex("00 FF 4E 04 00 00 01")
    chunk2 = bytes.fromhex("01 03 4E 04 00 00 06 01 01")

    assert parser.feed(chunk1) == []
    messages = parser.feed(chunk2)

    assert len(messages) == 2
    assert messages[0].op_code == Op.ANC_QUERY
    assert messages[0].payload == bytes([0x03])
    assert messages[1].op_code == Op.LOW_LATENCY_QUERY
    assert messages[1].payload == bytes([0x01])


def test_parse_state_updates() -> None:
    battery = parse_state_update(ParsedMessage(op_code=Op.BATTERY, payload=bytes([80, 75, 60])))
    battery_unknown_case = parse_state_update(ParsedMessage(op_code=Op.BATTERY, payload=bytes([80, 75, 0])))
    anc = parse_state_update(ParsedMessage(op_code=Op.ANC_QUERY, payload=bytes([0x11])))
    eq = parse_state_update(ParsedMessage(op_code=Op.EQ_QUERY, payload=bytes([0x04])))
    gm = parse_state_update(ParsedMessage(op_code=Op.GAME_MODE_QUERY, payload=bytes([0x01])))
    ll = parse_state_update(ParsedMessage(op_code=Op.LOW_LATENCY_QUERY, payload=bytes([0x01])))
    fw = parse_state_update(ParsedMessage(op_code=Op.VERSION, payload=bytes([0x08, 0x04])))

    assert battery.left_battery == 80
    assert battery.right_battery == 75
    assert battery.case_battery == 60
    assert battery_unknown_case.case_battery is None
    assert anc.anc_mode == AncMode.WIND_SUPPRESSION
    assert eq.eq_mode == EqMode.GAME
    assert gm.game_mode_enabled is True
    assert ll.low_latency_enabled is True
    assert fw.firmware is not None
    assert fw.firmware.main == 4
    assert fw.firmware.sub == 8


def test_startup_queries_include_battery() -> None:
    assert paced_startup_queries() == (
        query_firmware(),
        query_battery(),
        query_anc(),
        query_eq(),
        query_game_mode(),
        query_low_latency(),
    )
