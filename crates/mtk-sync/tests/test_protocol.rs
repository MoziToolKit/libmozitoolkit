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

#[test]
fn test_negative_coordinates_roundtrip_entities_and_deltas() {
    let pot_entity = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [
            Some("minecraft:angler_pottery_sherd".to_string()),
            None,
            None,
            None,
        ],
    };

    let test_coords = vec![
        IVec3::new(-1, -1, -1),
        IVec3::new(-64, -128, -256),
        IVec3::new(-30_000_000, -64, 30_000_000),
        IVec3::new(i32::MIN, -1, i32::MAX),
        IVec3::new(i32::MIN + 1, 0, -100_000),
        IVec3::new(-12345, 67890, -13579),
    ];

    // 1. DeltaEntity round-trip with negative coordinates
    for &pos in &test_coords {
        let delta = Packet::DeltaEntity {
            epoch: 42,
            seq_id: 1001,
            rel_pos: pos,
            entity: Some(pot_entity.clone()),
        };
        let encoded = encode_packet(&delta);
        let decoded = decode_packet(&encoded).expect("DeltaEntity decode failed");
        assert_eq!(decoded, delta, "DeltaEntity mismatch for coord {:?}", pos);

        let delta_none = Packet::DeltaEntity {
            epoch: 42,
            seq_id: 1002,
            rel_pos: pos,
            entity: None,
        };
        let encoded_none = encode_packet(&delta_none);
        let decoded_none = decode_packet(&encoded_none).expect("DeltaEntity None decode failed");
        assert_eq!(
            decoded_none, delta_none,
            "DeltaEntity None mismatch for coord {:?}",
            pos
        );
    }

    // 2. BaselineEntities round-trip with all negative coordinates
    let baseline_entries: Vec<(IVec3, mtk_model::BlockEntityData)> = test_coords
        .iter()
        .map(|&pos| (pos, pot_entity.clone()))
        .collect();

    let baseline = Packet::BaselineEntities {
        epoch: 99,
        snapshot_uuid: [0x55; 16],
        entities: baseline_entries.clone(),
    };
    let encoded_bl = encode_packet(&baseline);
    let decoded_bl = decode_packet(&encoded_bl).expect("BaselineEntities decode failed");
    assert_eq!(
        decoded_bl, baseline,
        "BaselineEntities negative coords round-trip mismatch"
    );

    // 3. DeltaOp round-trip with negative coordinates
    let delta_changes: Vec<DeltaChangeEntry> = test_coords
        .iter()
        .enumerate()
        .map(|(i, &pos)| DeltaChangeEntry {
            rel_pos: pos,
            state_id: (i + 1) as u16,
        })
        .collect();
    let hash = compute_delta_batch_hash(&delta_changes);
    let delta_op = Packet::DeltaOp {
        epoch: 10,
        seq_id: 500,
        palette_revision: 3,
        count: delta_changes.len() as u16,
        batch_xxhash64: hash,
        changes: delta_changes.clone(),
    };
    let encoded_op = encode_packet(&delta_op);
    let decoded_op = decode_packet(&encoded_op).expect("DeltaOp decode failed");
    assert_eq!(
        decoded_op, delta_op,
        "DeltaOp negative coords round-trip mismatch"
    );
}

#[test]
fn test_raw_negative_coordinate_decoding_endianness() {
    // Manually construct raw little-endian bytes for DeltaEntity
    let mut raw_bytes = Vec::new();
    raw_bytes.extend_from_slice(&PROTOCOL_MAGIC);
    raw_bytes.push(PROTOCOL_VERSION);
    raw_bytes.push(Opcode::DeltaEntity as u8);
    raw_bytes.extend_from_slice(&100u64.to_le_bytes());
    raw_bytes.extend_from_slice(&200u64.to_le_bytes());
    raw_bytes.extend_from_slice(&(-500i32).to_le_bytes());
    raw_bytes.extend_from_slice(&(-64i32).to_le_bytes());
    raw_bytes.extend_from_slice(&(-30_000_000i32).to_le_bytes());
    raw_bytes.extend_from_slice(&0u32.to_le_bytes());

    let packet = decode_packet(&raw_bytes).expect("Failed decoding manual DeltaEntity");
    match packet {
        Packet::DeltaEntity {
            rel_pos,
            epoch,
            seq_id,
            entity,
        } => {
            assert_eq!(epoch, 100);
            assert_eq!(seq_id, 200);
            assert_eq!(rel_pos, IVec3::new(-500, -64, -30_000_000));
            assert!(entity.is_none());
        }
        _ => panic!("Expected DeltaEntity packet"),
    }

    // Manually construct raw little-endian bytes for BaselineEntities
    let mut raw_baseline = Vec::new();
    raw_baseline.extend_from_slice(&PROTOCOL_MAGIC);
    raw_baseline.push(PROTOCOL_VERSION);
    raw_baseline.push(Opcode::BaselineEntities as u8);
    raw_baseline.extend_from_slice(&1u64.to_le_bytes()); // epoch
    raw_baseline.extend_from_slice(&[0xAA; 16]); // snapshot_uuid
    raw_baseline.extend_from_slice(&1u32.to_le_bytes()); // count = 1
    raw_baseline.extend_from_slice(&(i32::MIN).to_le_bytes()); // rx
    raw_baseline.extend_from_slice(&(-1i32).to_le_bytes()); // ry
    raw_baseline.extend_from_slice(&(i32::MAX).to_le_bytes()); // rz
    let pot = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [None, None, None, None],
    };
    let pot_bytes = serde_json::to_vec(&pot).unwrap();
    raw_baseline.extend_from_slice(&(pot_bytes.len() as u32).to_le_bytes());
    raw_baseline.extend_from_slice(&pot_bytes);

    let decoded_baseline =
        decode_packet(&raw_baseline).expect("Failed decoding manual BaselineEntities");
    match decoded_baseline {
        Packet::BaselineEntities { entities, .. } => {
            assert_eq!(entities.len(), 1);
            assert_eq!(entities[0].0, IVec3::new(i32::MIN, -1, i32::MAX));
        }
        _ => panic!("Expected BaselineEntities packet"),
    }
}

#[test]
fn test_sync_codec_throughput_benchmark() {
    let pot_entity = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [None, None, None, None],
    };
    let delta = Packet::DeltaEntity {
        epoch: 1,
        seq_id: 100,
        rel_pos: IVec3::new(-16, -64, -128),
        entity: Some(pot_entity),
    };
    let encoded = encode_packet(&delta);

    // Warm-up
    for _ in 0..1000 {
        let _ = decode_packet(&encoded).unwrap();
    }

    let iterations = 100_000;
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let pkt = decode_packet(&encoded).unwrap();
        std::hint::black_box(pkt);
    }
    let elapsed = start.elapsed();
    let total_secs = elapsed.as_secs_f64();
    let throughput = iterations as f64 / total_secs;
    let avg_ns = (total_secs * 1e9) / iterations as f64;
    let mb_per_sec = (iterations * encoded.len()) as f64 / (1024.0 * 1024.0 * total_secs);

    println!("\n============================================================");
    println!(" RFC v3.1 Codec DeltaEntity Decoding Throughput");
    println!(" Iterations: {}", iterations);
    println!(" Encoded Packet Size: {} bytes", encoded.len());
    println!(
        " Total Elapsed: {:.4} s ({:.2} ms)",
        total_secs,
        total_secs * 1000.0
    );
    println!(" Average Latency: {:.2} ns / packet", avg_ns);
    println!(" Throughput: {:.2} packets / sec", throughput);
    println!(" Throughput: {:.2} MB / sec", mb_per_sec);
    println!("============================================================");

    assert!(
        throughput > 200_000.0,
        "Throughput must exceed 200k packets/sec"
    );
}
