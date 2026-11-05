use glam::IVec3;
use mtk_sync::*;

#[test]
fn test_hello_and_welcome_codec_roundtrip() {
    let hello = Packet::Hello {
        min_version: 0x0003,
        max_version: 0x0003,
        capabilities: 0x03,
        flags: 0x01,
        cached_world_uuid: [1u8; 16],
        cached_epoch: 42,
        cached_seq: 100,
        cached_min: IVec3::new(-16, 60, -16),
        cached_size: IVec3::new(32, 64, 32),
        cached_state_hash: 0xDEADBEEFCAFE,
        auth_token: "secret_token".to_string(),
        cached_dimension: "minecraft:overworld".to_string(),
    };

    let encoded = encode_packet(&hello);
    assert_eq!(&encoded[0..2], &PROTOCOL_MAGIC);
    assert_eq!(encoded[2], PROTOCOL_VERSION);
    assert_eq!(encoded[3], Opcode::Hello as u8);

    let decoded = decode_packet(&encoded).expect("Failed decoding Hello");
    assert_eq!(decoded, hello);

    let welcome = Packet::Welcome {
        negotiated_version: 0x0003,
        server_uuid: [2u8; 16],
        epoch: 42,
        server_seq: 105,
        current_min: IVec3::new(-16, 60, -16),
        current_size: IVec3::new(32, 64, 32),
        current_state_hash: 0xDEADBEEFCAFE,
        decision: SyncDecision::VerifiedSkipRebuild,
        message_or_dimension: "minecraft:overworld".to_string(),
    };

    let encoded_welcome = encode_packet(&welcome);
    let decoded_welcome = decode_packet(&encoded_welcome).expect("Failed decoding Welcome");
    assert_eq!(decoded_welcome, welcome);
}

#[test]
fn test_baseline_begin_and_end_roundtrip() {
    let begin = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [3u8; 16],
        snapshot_seq: 50,
        min_pos: IVec3::new(0, 0, 0),
        size: IVec3::new(16, 16, 16),
        state_hash: 0x12345678,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: 1024,
        total_decoded_bytes: 4096,
        compression_mode: 0,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string(), "minecraft:stone".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    };

    let encoded = encode_packet(&begin);
    let decoded = decode_packet(&encoded).expect("Failed decoding BaselineBegin");
    assert_eq!(decoded, begin);

    let end = Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [3u8; 16],
        snapshot_seq: 50,
        total_chunks_sent: 1,
        final_state_hash: 0x12345678,
        status: 0,
    };

    let encoded_end = encode_packet(&end);
    let decoded_end = decode_packet(&encoded_end).expect("Failed decoding BaselineEnd");
    assert_eq!(decoded_end, end);
}

#[test]
fn test_delta_op_codec_and_hash_roundtrip() {
    let changes = vec![
        DeltaChangeEntry {
            rel_pos: IVec3::new(0, 1, 2),
            state_id: 1,
        },
        DeltaChangeEntry {
            rel_pos: IVec3::new(3, 4, 5),
            state_id: 2,
        },
    ];

    let hash = compute_delta_batch_hash(&changes);

    let delta = Packet::DeltaOp {
        epoch: 2,
        seq_id: 101,
        palette_revision: 1,
        count: changes.len() as u16,
        batch_xxhash64: hash,
        changes,
    };

    let encoded = encode_packet(&delta);
    let decoded = decode_packet(&encoded).expect("Failed decoding DeltaOp");
    assert_eq!(decoded, delta);
}

#[test]
fn test_corrupted_chunk_hash_rejection() {
    let payload = vec![0xAB; 64];
    let chunk = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [0u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 64,
        payload_len: 64,
        decoded_target_bytes: 64,
        chunk_xxhash64: 0x1122334455667788, // Intentionally wrong
        payload,
    };

    let encoded = encode_packet(&chunk);
    let res = decode_packet(&encoded);
    assert!(matches!(res, Err(ProtocolError::ChecksumMismatch { .. })));
}

#[test]
fn test_invalid_magic_and_version() {
    let mut buf = vec![0x00, 0x00, 0x03, 0x01]; // Bad magic
    assert!(matches!(
        decode_packet(&buf),
        Err(ProtocolError::InvalidMagic(..))
    ));

    buf[0] = PROTOCOL_MAGIC[0];
    buf[1] = PROTOCOL_MAGIC[1];
    buf[2] = 0x99; // Bad version
    assert!(matches!(
        decode_packet(&buf),
        Err(ProtocolError::UnsupportedVersion(0x99))
    ));
}

#[test]
fn test_legacy_replay_deltas_decision_is_rejected() {
    assert_eq!(SyncDecision::from_u8(0x01), None);
    assert_eq!(
        SyncDecision::from_u8(0x02),
        Some(SyncDecision::FullBaselineRequired)
    );
}

#[test]
fn test_baseline_entities_and_delta_entity_roundtrip() {
    let pot_entity = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [
            Some("minecraft:angler_pottery_sherd".to_string()),
            Some("minecraft:blade_pottery_sherd".to_string()),
            Some("minecraft:brewer_pottery_sherd".to_string()),
            Some("minecraft:burn_pottery_sherd".to_string()),
        ],
    };
    let entities = vec![
        (IVec3::new(1, 2, 3), pot_entity.clone()),
        (
            IVec3::new(4, 5, 6),
            mtk_model::BlockEntityData::DecoratedPot {
                sherds: [None, None, None, None],
            },
        ),
    ];

    let baseline_entities = Packet::BaselineEntities {
        epoch: 7,
        snapshot_uuid: [9u8; 16],
        entities,
    };

    let encoded = encode_packet(&baseline_entities);
    let decoded = decode_packet(&encoded).expect("Failed decoding BaselineEntities");
    assert_eq!(decoded, baseline_entities);

    let delta_entity = Packet::DeltaEntity {
        epoch: 7,
        seq_id: 1024,
        rel_pos: IVec3::new(4, 5, 6),
        entity: Some(pot_entity),
    };

    let encoded_delta = encode_packet(&delta_entity);
    let decoded_delta = decode_packet(&encoded_delta).expect("Failed decoding DeltaEntity");
    assert_eq!(decoded_delta, delta_entity);
}
