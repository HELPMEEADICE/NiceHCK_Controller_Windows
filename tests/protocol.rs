use nicehck_controller::models::{AncMode, CodecMode, EqMode};
use nicehck_controller::protocol::{
    Op, PacketStreamParser, ParsedMessage, paced_startup_queries, parse_state_update, query_anc,
    query_battery, query_eq, query_firmware, query_game_mode, query_low_latency, set_anc,
    set_codec, set_eq, set_game_mode, set_legacy_codec_lhdc, set_low_latency,
};

#[test]
fn builds_query_packets() {
    assert_eq!(query_firmware(), [0x4e, 3, 0, 0, 3, 0]);
    assert_eq!(query_battery(), [0x4e, 3, 0, 0, 5, 0]);
    assert_eq!(query_anc(), [0x4e, 3, 0, 0, 1, 1]);
    assert_eq!(query_eq(), [0x4e, 3, 0, 0, 7, 1]);
    assert_eq!(query_game_mode(), [0x4e, 3, 0, 0, 8, 1]);
    assert_eq!(query_low_latency(), [0x4e, 3, 0, 0, 6, 1]);
}

#[test]
fn builds_set_packets() {
    assert_eq!(set_anc(AncMode::Transparent), [0x4e, 5, 0, 0, 1, 2, 1, 0]);
    assert_eq!(set_eq(EqMode::Game), [0x4e, 4, 0, 0, 7, 2, 4]);
    assert_eq!(set_codec(CodecMode::Aac), [0x4e, 4, 0, 0, 4, 2, 0]);
    assert_eq!(set_codec(CodecMode::Lhdc), [0x4e, 4, 0, 0, 4, 2, 1]);
    assert_eq!(set_codec(CodecMode::Sbc), [0x4e, 4, 0, 0, 4, 2, 2]);
    assert_eq!(set_legacy_codec_lhdc(true), [0x4e, 4, 0, 0, 4, 0, 1]);
    assert_eq!(set_legacy_codec_lhdc(false), [0x4e, 4, 0, 0, 4, 0, 0]);
    assert_eq!(set_game_mode(true), [0x4e, 4, 0, 0, 8, 2, 1]);
    assert_eq!(set_game_mode(false), [0x4e, 4, 0, 0, 8, 2, 0]);
    assert_eq!(set_low_latency(true), [0x4e, 4, 0, 0, 6, 2, 1]);
    assert_eq!(set_low_latency(false), [0x4e, 4, 0, 0, 6, 2, 0]);
}

#[test]
fn parser_handles_split_sticky_and_noisy_packets() {
    let mut parser = PacketStreamParser::default();
    assert!(parser.feed(&[0, 0xff, 0x4e, 4, 0, 0, 1]).is_empty());
    let messages = parser.feed(&[1, 3, 0x4e, 4, 0, 0, 6, 1, 1]);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].op_code, Op::AncQuery as u16);
    assert_eq!(messages[0].payload, [3]);
    assert_eq!(messages[1].op_code, Op::LowLatencyQuery as u16);
    assert_eq!(messages[1].payload, [1]);
}

#[test]
fn parser_resynchronizes_after_invalid_length() {
    let mut parser = PacketStreamParser::default();
    let messages = parser.feed(&[0x4e, 0xff, 0xff, 0, 0, 0, 0x4e, 4, 0, 0, 1, 1, 3]);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].op_code, Op::AncQuery as u16);
    assert_eq!(messages[0].payload, [3]);
}

#[test]
fn parses_state_updates_and_preserves_unknown_modes() {
    let battery = parse_state_update(&message(Op::Battery, &[80, 75, 60]));
    let battery_without_case = parse_state_update(&message(Op::Battery, &[80, 75, 0]));
    let anc = parse_state_update(&message(Op::AncQuery, &[0x11]));
    let unknown_eq = parse_state_update(&message(Op::EqQuery, &[0xfe]));
    let firmware = parse_state_update(&message(Op::Version, &[8, 4]));

    assert_eq!(battery.left_battery, Some(80));
    assert_eq!(battery.right_battery, Some(75));
    assert_eq!(battery.case_battery, Some(Some(60)));
    assert_eq!(battery_without_case.case_battery, Some(None));
    assert_eq!(anc.anc_mode, Some(AncMode::WindSuppression));
    assert_eq!(unknown_eq.eq_mode, Some(EqMode::Unknown(0xfe)));
    assert_eq!(firmware.firmware.expect("firmware").display(), "4.8");
}

#[test]
fn startup_queries_include_battery_in_order() {
    assert_eq!(
        paced_startup_queries(),
        vec![
            query_firmware(),
            query_battery(),
            query_anc(),
            query_eq(),
            query_game_mode(),
            query_low_latency(),
        ]
    );
}

fn message(op: Op, payload: &[u8]) -> ParsedMessage {
    ParsedMessage {
        op_code: op as u16,
        payload: payload.to_vec(),
    }
}
