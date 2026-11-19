use mtk_sync::protocol::{
    decode_packet, encode_packet, format_uuid, parse_canonical_uuid, ErrorCode, Opcode, Packet,
    ProtocolError, PROTOCOL_MAGIC, PROTOCOL_VERSION,
};

#[test]
fn test_canonical_uuid_parsing() {
    // 1. Rejects garbage
    assert!(parse_canonical_uuid("").is_err());
    assert!(parse_canonical_uuid("not-a-uuid").is_err());
    assert!(parse_canonical_uuid("12345").is_err());
    assert!(parse_canonical_uuid("00000000-0000-0000-0000-00000000000g").is_err());
    assert!(parse_canonical_uuid("c0ffee00-1234-5678-9abc-def012345678-extra").is_err());

    // 2. Rejects multibyte input without panicking
    assert!(parse_canonical_uuid("你好世界123456789012345678901234567890").is_err());
    assert!(parse_canonical_uuid("🦀0000000-0000-0000-0000-000000000000").is_err());
    assert!(parse_canonical_uuid("00000000-0000-0000-0000-00000000000🦀").is_err());
    assert!(parse_canonical_uuid("äöüß1234-5678-9abc-def0-123456789abc").is_err());

    // 3. Round-trips a known UUID in both dashed and 32-hex form
    let dashed = "c0ffee00-1234-5678-9abc-def012345678";
    let (most, least) = parse_canonical_uuid(dashed).expect("dashed uuid must parse");
    let formatted = format_uuid(most, least);
    assert_eq!(formatted, dashed);

    let hex32 = "c0ffee00123456789abcdef012345678";
    let (most2, least2) = parse_canonical_uuid(hex32).expect("32-hex uuid must parse");
    assert_eq!((most2, least2), (most, least));
    assert_eq!(format_uuid(most2, least2), dashed);
}

#[test]
fn test_encode_string_too_long() {
    let long_str = "x".repeat(70_000);
    let pkt = Packet::Error {
        epoch: 1,
        error_code: ErrorCode::InvalidBounds,
        message: long_str,
    };

    let res = encode_packet(&pkt);
    match res {
        Err(ProtocolError::StringTooLong { actual, limit }) => {
            assert_eq!(actual, 70_000);
            assert_eq!(limit, u16::MAX as usize);
        }
        other => panic!("expected StringTooLong, got {:?}", other),
    }
}

#[test]
fn test_decode_corrupt_entity_payload() {
    // Construct a binary DeltaEntity frame with corrupt JSON block entity payload
    let mut buf = Vec::new();
    buf.extend_from_slice(&PROTOCOL_MAGIC);
    buf.push(PROTOCOL_VERSION);
    buf.push(Opcode::DeltaEntity as u8);

    let epoch: u64 = 1;
    let seq_id: u64 = 100;
    let rx: i32 = 0;
    let ry: i32 = 64;
    let rz: i32 = 0;
    buf.extend_from_slice(&epoch.to_le_bytes());
    buf.extend_from_slice(&seq_id.to_le_bytes());
    buf.extend_from_slice(&(rx as u32).to_le_bytes());
    buf.extend_from_slice(&(ry as u32).to_le_bytes());
    buf.extend_from_slice(&(rz as u32).to_le_bytes());

    let corrupt_json = b"{ not valid json at all ::: ";
    buf.extend_from_slice(&(corrupt_json.len() as u32).to_le_bytes());
    buf.extend_from_slice(corrupt_json);

    let res = decode_packet(&buf);
    assert!(
        matches!(res, Err(ProtocolError::InvalidPayload { .. })),
        "expected InvalidPayload, got {:?}",
        res
    );
}
