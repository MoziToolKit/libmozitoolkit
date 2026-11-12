use crate::protocol::*;
use glam::IVec3;

#[test]
fn test_all_packets_truncated_payload_safety() {
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
        subscribed_visual_entities: Vec::new(),
    };

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

    let chunk_payload = vec![0x12; 128];
    let mut hasher = xxhash_rust::xxh64::Xxh64::new(0);
    hasher.update(&chunk_payload);
    let chunk_hash = hasher.digest();
    let chunk = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [4u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 128,
        payload_len: 128,
        decoded_target_bytes: 128,
        chunk_xxhash64: chunk_hash,
        payload: chunk_payload,
    };

    let end = Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [5u8; 16],
        snapshot_seq: 50,
        total_chunks_sent: 1,
        final_state_hash: 0x12345678,
        status: 0,
    };

    let ack = Packet::BaselineAck {
        epoch: 1,
        snapshot_uuid: [6u8; 16],
        committed_seq: 50,
        result_code: BaselineAckResult::SuccessCommitted,
    };

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
    let d_hash = compute_delta_batch_hash(&changes);
    let delta = Packet::DeltaOp {
        epoch: 2,
        seq_id: 101,
        palette_revision: 1,
        count: changes.len() as u16,
        batch_xxhash64: d_hash,
        changes,
    };

    let palette_update = Packet::PaletteUpdate {
        epoch: 2,
        seq_id: 102,
        base_revision: 1,
        new_revision: 2,
        entries: vec![
            (10, "minecraft:diamond_block".to_string()),
            (11, "minecraft:gold_block".to_string()),
        ],
    };

    let cancel = Packet::Cancel {
        epoch: 3,
        snapshot_uuid: [7u8; 16],
        reason: CancelReason::UserAbort,
    };

    let resync = Packet::ResyncRequest {
        epoch: 3,
        missing_from_seq: 10,
        missing_to_seq: 20,
        reason: ResyncReason::SequenceGap,
    };

    let ping = Packet::Ping {
        timestamp: 123456789,
    };
    let pong = Packet::Pong {
        timestamp: 123456789,
    };

    let err = Packet::Error {
        epoch: 4,
        error_code: ErrorCode::ChecksumMismatch,
        message: "Checksum failed".to_string(),
    };

    let entities = vec![(
        IVec3::new(1, 2, 3),
        mtk_model::BlockEntityData::DecoratedPot {
            sherds: [
                Some("minecraft:angler_pottery_sherd".to_string()),
                None,
                None,
                None,
            ],
        },
    )];
    let baseline_entities = Packet::BaselineEntities {
        epoch: 5,
        snapshot_uuid: [8u8; 16],
        entities,
    };

    let delta_entity = Packet::DeltaEntity {
        epoch: 5,
        seq_id: 200,
        rel_pos: IVec3::new(1, 2, 3),
        entity: Some(mtk_model::BlockEntityData::DecoratedPot {
            sherds: [None, None, None, None],
        }),
    };

    let packets = vec![
        hello,
        welcome,
        begin,
        chunk,
        end,
        ack,
        delta,
        palette_update,
        cancel,
        resync,
        ping,
        pong,
        err,
        baseline_entities,
        delta_entity,
    ];

    for (i, pkt) in packets.iter().enumerate() {
        let encoded = encode_packet(pkt);
        let roundtrip = decode_packet(&encoded).expect("Roundtrip failed");
        assert_eq!(&roundtrip, pkt, "Packet {} roundtrip mismatch", i);

        for len in 0..encoded.len() {
            let truncated = &encoded[..len];
            let res = decode_packet(truncated);
            assert!(
                res.is_err(),
                "Expected error for truncated packet {} at length {}/{}, got Ok: {:?}",
                i,
                len,
                encoded.len(),
                res
            );
            match res.unwrap_err() {
                ProtocolError::PacketTooShort { .. } => {
                    assert!(len < 4, "PacketTooShort only expected for len < 4");
                }
                ProtocolError::TruncatedPayload { .. } => {
                    assert!(len >= 4, "TruncatedPayload expected for len >= 4");
                }
                other => {
                    panic!(
                        "Unexpected error variant for packet {} at len {}: {:?}",
                        i, len, other
                    );
                }
            }
        }
    }
}

#[test]
fn test_random_corrupt_byte_slices_no_panic() {
    // Simple LCG PRNG for determinism without external dependencies
    let mut state: u64 = 0x853c49e6748fea9b;
    let mut next_u32 = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as u32
    };

    // 1. Completely random byte buffers
    for len in 0..=512 {
        for _ in 0..20 {
            let mut buf = vec![0u8; len];
            for byte in buf.iter_mut() {
                *byte = next_u32() as u8;
            }
            // Must NEVER panic
            let _ = decode_packet(&buf);
        }
    }

    // 2. Buffers with valid header prefix, random payload
    let opcodes = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0B, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12,
        0x13, 0x14,
    ];

    for &opcode in &opcodes {
        for payload_len in 0..=256 {
            for _ in 0..10 {
                let mut buf = vec![
                    PROTOCOL_MAGIC[0],
                    PROTOCOL_MAGIC[1],
                    PROTOCOL_VERSION,
                    opcode,
                ];
                for _ in 0..payload_len {
                    buf.push(next_u32() as u8);
                }
                // Must NEVER panic
                let _ = decode_packet(&buf);
            }
        }
    }
}

#[test]
fn test_corrupt_packet_mutations_no_panic() {
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
        subscribed_visual_entities: Vec::new(),
    };
    let encoded = encode_packet(&hello);

    let mut state: u64 = 0x123456789abcdef0;
    let mut next_u32 = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as u32
    };

    // Bit-flip fuzzing on valid packet
    for _ in 0..2000 {
        let mut corrupted = encoded.clone();
        let num_mutations = (next_u32() % 5) + 1;
        for _ in 0..num_mutations {
            let idx = (next_u32() as usize) % corrupted.len();
            corrupted[idx] ^= (next_u32() % 255 + 1) as u8;
        }
        let _ = decode_packet(&corrupted);
    }
}

#[test]
fn test_boundary_extreme_values_safety() {
    // 1. DeltaOp with count exceeding limit
    let mut delta_buf = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::DeltaOp as u8,
    ];
    delta_buf.extend_from_slice(&1u64.to_le_bytes()); // epoch
    delta_buf.extend_from_slice(&1u64.to_le_bytes()); // seq_id
    delta_buf.extend_from_slice(&1u32.to_le_bytes()); // palette_revision
    delta_buf.extend_from_slice(&20000u16.to_le_bytes()); // count > MAX_DELTA_BATCH_CHANGES (16384)
    delta_buf.extend_from_slice(&0u64.to_le_bytes()); // batch_xxhash64
    let res = decode_packet(&delta_buf);
    assert!(matches!(
        res,
        Err(ProtocolError::ResourceLimitExceeded { .. })
    ));

    // 2. BaselineBegin with total_decoded_bytes exceeding limit
    let mut begin_buf = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::BaselineBegin as u8,
    ];
    begin_buf.extend_from_slice(&1u64.to_le_bytes()); // epoch
    begin_buf.extend_from_slice(&[0u8; 16]); // uuid
    begin_buf.extend_from_slice(&1u64.to_le_bytes()); // seq
    begin_buf.extend_from_slice(&[0u8; 24]); // min & size
    begin_buf.extend_from_slice(&0u64.to_le_bytes()); // state_hash
    begin_buf.extend_from_slice(&0u32.to_le_bytes()); // palette_revision
    begin_buf.extend_from_slice(&1u32.to_le_bytes()); // total_chunks
    begin_buf.extend_from_slice(&1024u64.to_le_bytes()); // total_encoded_bytes
    let max_baseline_decoded_bytes = 256 * 1024 * 1024;
    begin_buf.extend_from_slice(&(max_baseline_decoded_bytes as u64 + 1).to_le_bytes()); // total_decoded_bytes > limit
    begin_buf.extend_from_slice(&[0u8; 3]); // compression_mode, palette_bits, biome_bits
    let res = decode_packet(&begin_buf);
    assert!(matches!(
        res,
        Err(ProtocolError::ResourceLimitExceeded { .. })
    ));

    // 3. BaselineChunk with huge payload_len claimed, but truncated buffer
    let mut chunk_buf = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::BaselineChunk as u8,
    ];
    chunk_buf.extend_from_slice(&1u64.to_le_bytes()); // epoch
    chunk_buf.extend_from_slice(&[0u8; 16]); // uuid
    chunk_buf.extend_from_slice(&0u32.to_le_bytes()); // chunk_index
    chunk_buf.extend_from_slice(&1u32.to_le_bytes()); // total_chunks
    chunk_buf.extend_from_slice(&0u64.to_le_bytes()); // first_voxel_index
    chunk_buf.extend_from_slice(&100u32.to_le_bytes()); // voxel_count
    chunk_buf.extend_from_slice(&0x7FFFFFFF_u32.to_le_bytes()); // huge payload_len
    chunk_buf.extend_from_slice(&100u32.to_le_bytes()); // decoded_target_bytes
    chunk_buf.extend_from_slice(&0u64.to_le_bytes()); // xxhash64
                                                      // Payload only has 10 bytes instead of claimed 2GB
    chunk_buf.extend_from_slice(&[0u8; 10]);
    let res = decode_packet(&chunk_buf);
    assert!(matches!(res, Err(ProtocolError::TruncatedPayload { .. })));

    // 4. BaselineEntities with huge count claimed, but truncated buffer
    let mut entities_buf = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::BaselineEntities as u8,
    ];
    entities_buf.extend_from_slice(&1u64.to_le_bytes()); // epoch
    entities_buf.extend_from_slice(&[0u8; 16]); // uuid
    entities_buf.extend_from_slice(&1_000_000_u32.to_le_bytes()); // claimed count 1 million
                                                                  // Buffer ends immediately here!
    let res = decode_packet(&entities_buf);
    // At line 203: if *offset + 16 > buf.len() -> TruncatedPayload
    assert!(matches!(res, Err(ProtocolError::TruncatedPayload { .. })));

    // 5. BaselineEntities with count = u32::MAX
    let mut max_entities_buf = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::BaselineEntities as u8,
    ];
    max_entities_buf.extend_from_slice(&1u64.to_le_bytes()); // epoch
    max_entities_buf.extend_from_slice(&[0u8; 16]); // uuid
    max_entities_buf.extend_from_slice(&u32::MAX.to_le_bytes()); // count = 0xFFFFFFFF
    let res2 = decode_packet(&max_entities_buf);
    assert!(matches!(res2, Err(ProtocolError::TruncatedPayload { .. })));

    // 6. Invalid UTF-8 in strings (e.g. Hello auth_token)
    let mut bad_utf8_hello = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::Hello as u8,
    ];
    // 76 bytes of fixed fields
    bad_utf8_hello.extend_from_slice(&[0u8; 76]);
    // auth_token len = 2
    bad_utf8_hello.extend_from_slice(&2u16.to_le_bytes());
    // invalid UTF-8 sequence [0xFF, 0xFF]
    bad_utf8_hello.extend_from_slice(&[0xFF, 0xFF]);
    let res_utf8 = decode_packet(&bad_utf8_hello);
    assert!(matches!(res_utf8, Err(ProtocolError::Utf8Error { .. })));

    // 7. Invalid enum in CancelReason
    let mut bad_cancel = vec![
        PROTOCOL_MAGIC[0],
        PROTOCOL_MAGIC[1],
        PROTOCOL_VERSION,
        Opcode::Cancel as u8,
    ];
    bad_cancel.extend_from_slice(&1u64.to_le_bytes()); // epoch
    bad_cancel.extend_from_slice(&[0u8; 16]); // uuid
    bad_cancel.push(0xFE); // invalid reason byte
    let res_cancel = decode_packet(&bad_cancel);
    assert!(matches!(res_cancel, Err(ProtocolError::InvalidEnum { .. })));
}
