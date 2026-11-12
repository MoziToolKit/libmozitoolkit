//! # End-to-End Loopback Tests (RFC v3.1 / v3.2)
//!
//! Tests the full closed loop between the native Server encoder (transmitter)
//! and the native Client assembler (receiver) across protocol binary codecs.

use glam::IVec3;
use mtk_model::BlockEntityData;
use mtk_sync::server::{
    decide_handshake, encode_baseline_from_raw, encode_baseline_from_storage, encode_delta_op,
    BaselineEncoderConfig, HandshakeContext,
};
use mtk_sync::{
    compute_sparse_state_hash_for_storage, decode_packet, encode_packet, BaselineAssembler,
    DeltaChangeEntry, Packet, SyncDecision, PROTOCOL_VERSION,
};
use mtk_voxel::VoxelStorage;

#[test]
fn test_loopback_storage_baseline_assembly() {
    let mut storage = VoxelStorage::new();
    let min_pos = IVec3::new(10, -5, 20);
    let size = IVec3::new(8, 6, 8); // 384 voxels

    // Populate with heterogeneous blocks
    storage.set_block(10, -5, 20, "minecraft:stone", Some("minecraft:plains"));
    storage.set_block(11, -5, 20, "minecraft:dirt", Some("minecraft:plains"));
    storage.set_block(12, -4, 21, "minecraft:oak_planks", Some("minecraft:plains"));
    storage.set_block(
        15,
        0,
        25,
        "minecraft:diamond_block",
        Some("minecraft:desert"),
    );

    // Add a block entity
    let sign_data = BlockEntityData::new_sign(
        vec!["Line 1".into(), "Line 2".into()],
        None::<String>,
        false,
    );
    storage.set_block_entity(11, -5, 20, Some(sign_data));

    let dimension = "minecraft:overworld";
    let config = BaselineEncoderConfig {
        max_chunk_size: 128, // Small chunk size to force multiple chunks
        compression_mode: 1, // Zstd
        compression_level: 3,
        epoch: 42,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 100,
        palette_revision: 1,
    };

    // 1. Server encodes baseline stream
    let baseline_stream =
        encode_baseline_from_storage(&storage, min_pos, size, dimension, &[], &config)
            .expect("Server baseline encoding must succeed");

    assert!(
        baseline_stream.total_chunks > 1,
        "Should produce multiple chunks"
    );
    assert_ne!(baseline_stream.state_hash, 0);

    // 2. Transmit through binary serialization loop
    let mut client_assembler: Option<BaselineAssembler> = None;
    let mut end_pkt: Option<Packet> = None;

    for pkt in &baseline_stream.packets {
        let wire_bytes = encode_packet(pkt);
        let decoded = decode_packet(&wire_bytes).expect("Packet decode must succeed");

        match decoded {
            Packet::BaselineBegin { .. } => {
                client_assembler =
                    Some(BaselineAssembler::new(&decoded).expect("Begin must succeed"));
            }
            Packet::BaselineChunk { .. } => {
                let asm = client_assembler.as_mut().expect("Assembler must exist");
                asm.process_chunk(&decoded)
                    .expect("Process chunk must succeed");
            }
            Packet::BaselineEnd { .. } => {
                end_pkt = Some(decoded);
            }
            Packet::BaselineEntities { entities, .. } => {
                let asm = client_assembler.as_mut().expect("Assembler must exist");
                for (rel_pos, be) in entities {
                    asm.block_entities.insert(rel_pos, be);
                }
            }
            _ => {}
        }
    }

    // 3. Client finalizes and validates state_hash SSOT
    let asm = client_assembler.expect("Assembler must be initialized");
    let end = end_pkt.expect("BaselineEnd must be received");
    let reconstructed = asm.finalize(&end).expect("Finalize must succeed");

    // 4. Assert 100% equivalence
    assert_eq!(reconstructed.get_block(10, -5, 20), "minecraft:stone");
    assert_eq!(reconstructed.get_block(11, -5, 20), "minecraft:dirt");
    assert_eq!(reconstructed.get_block(12, -4, 21), "minecraft:oak_planks");
    assert_eq!(
        reconstructed.get_block(15, 0, 25),
        "minecraft:diamond_block"
    );
    assert_eq!(reconstructed.get_block(10, -4, 20), "minecraft:air");

    let reconstructed_hash =
        compute_sparse_state_hash_for_storage(&reconstructed, min_pos, size, dimension);
    assert_eq!(
        reconstructed_hash, baseline_stream.state_hash,
        "Reconstructed state_hash must match server baseline state_hash exactly"
    );
}

#[test]
fn test_loopback_raw_array_baseline_assembly() {
    let min_pos = IVec3::new(0, 64, 0);
    let size = IVec3::new(4, 4, 4); // 64 voxels
    let total_voxels = (size.x * size.y * size.z) as usize;

    let mut raw_states = vec![0u32; total_voxels];
    raw_states[0] = 1; // stone
    raw_states[5] = 2; // dirt
    raw_states[63] = 3; // gold_block

    let state_names = vec![
        "minecraft:air".to_string(),
        "minecraft:stone".to_string(),
        "minecraft:dirt".to_string(),
        "minecraft:gold_block".to_string(),
    ];

    let qw = (size.x / 4).max(1) as usize;
    let qh = (size.y / 4).max(1) as usize;
    let qd = (size.z / 4).max(1) as usize;
    let raw_biomes = vec![0u32; qw * qh * qd];
    let biome_names = vec!["minecraft:plains".to_string()];

    let config = BaselineEncoderConfig {
        max_chunk_size: 256 * 1024,
        compression_mode: 1,
        compression_level: 3,
        epoch: 1,
        snapshot_uuid: [2u8; 16],
        snapshot_seq: 1,
        palette_revision: 1,
    };

    let stream = encode_baseline_from_raw(
        min_pos,
        size,
        "minecraft:overworld",
        &raw_states,
        &state_names,
        &raw_biomes,
        &biome_names,
        &[],
        &[],
        &config,
    )
    .expect("Raw encoding must succeed");

    assert_eq!(stream.total_chunks, 1);

    // Client assembler processes packets
    let mut client_assembler: Option<BaselineAssembler> = None;
    let mut end_pkt: Option<Packet> = None;

    for pkt in &stream.packets {
        let wire_bytes = encode_packet(pkt);
        let decoded = decode_packet(&wire_bytes).expect("Decode must succeed");
        match decoded {
            Packet::BaselineBegin { .. } => {
                client_assembler = Some(BaselineAssembler::new(&decoded).unwrap());
            }
            Packet::BaselineChunk { .. } => {
                client_assembler
                    .as_mut()
                    .unwrap()
                    .process_chunk(&decoded)
                    .unwrap();
            }
            Packet::BaselineEnd { .. } => {
                end_pkt = Some(decoded);
            }
            _ => {}
        }
    }

    let reconstructed = client_assembler
        .unwrap()
        .finalize(&end_pkt.unwrap())
        .expect("Finalize must succeed");

    assert_eq!(reconstructed.get_block(0, 64, 0), "minecraft:stone");
    assert_eq!(reconstructed.get_block(3, 67, 3), "minecraft:gold_block");
    let reconstructed_hash =
        compute_sparse_state_hash_for_storage(&reconstructed, min_pos, size, "minecraft:overworld");
    assert_eq!(reconstructed_hash, stream.state_hash);
}

fn make_test_hello(
    token: &str,
    uuid: [u8; 16],
    seq: u64,
    hash: u64,
    min_pos: IVec3,
    size: IVec3,
) -> Packet {
    Packet::Hello {
        min_version: PROTOCOL_VERSION as u16,
        max_version: PROTOCOL_VERSION as u16,
        capabilities: 0,
        flags: 0,
        cached_world_uuid: uuid,
        cached_epoch: 10,
        cached_seq: seq,
        cached_min: min_pos,
        cached_size: size,
        cached_state_hash: hash,
        auth_token: token.to_string(),
        cached_dimension: "minecraft:overworld".to_string(),
        subscribed_visual_entities: vec![],
    }
}

#[test]
fn test_loopback_handshake_decision() {
    let server_uuid = [0xAA; 16];
    let min_pos = IVec3::new(100, 60, 200);
    let size = IVec3::new(16, 16, 16);
    let current_hash = 0x1234567890ABCDEF;

    let ctx = HandshakeContext {
        server_uuid,
        epoch: 10,
        sequence: 42,
        min_pos,
        size,
        state_hash: current_hash,
        dimension: "minecraft:overworld".to_string(),
        expected_auth_token: Some("secret123".to_string()),
    };

    // 1. Valid cached hello -> VerifiedSkipRebuild
    let valid_cached_hello =
        make_test_hello("secret123", server_uuid, 42, current_hash, min_pos, size);
    let res = decide_handshake(&valid_cached_hello, &ctx).expect("Decision must succeed");
    assert_eq!(res.decision, SyncDecision::VerifiedSkipRebuild);

    // 2. Wrong auth token -> Rejected
    let wrong_token_hello =
        make_test_hello("wrong_pwd", server_uuid, 42, current_hash, min_pos, size);
    let res = decide_handshake(&wrong_token_hello, &ctx).expect("Decision must succeed");
    assert_eq!(res.decision, SyncDecision::Rejected);

    // 3. Stale sequence -> FullBaselineRequired
    let stale_seq_hello =
        make_test_hello("secret123", server_uuid, 40, current_hash, min_pos, size);
    let res = decide_handshake(&stale_seq_hello, &ctx).expect("Decision must succeed");
    assert_eq!(res.decision, SyncDecision::FullBaselineRequired);

    // 4. Zero cache -> FullBaselineRequired
    let zero_cache_hello = make_test_hello("secret123", [0u8; 16], 0, 0, min_pos, size);
    let res = decide_handshake(&zero_cache_hello, &ctx).expect("Decision must succeed");
    assert_eq!(res.decision, SyncDecision::FullBaselineRequired);
}

#[test]
fn test_loopback_delta_batch_encode_and_decode() {
    let changes = vec![
        DeltaChangeEntry {
            rel_pos: IVec3::new(1, 2, 3),
            state_id: 10,
        },
        DeltaChangeEntry {
            rel_pos: IVec3::new(4, 5, 6),
            state_id: 20,
        },
    ];

    let packets = encode_delta_op(1, 100, 1, changes.clone());
    assert_eq!(packets.len(), 1);

    let wire_bytes = encode_packet(&packets[0]);
    let decoded = decode_packet(&wire_bytes).expect("Decode delta must succeed");

    if let Packet::DeltaOp {
        epoch,
        seq_id,
        changes: decoded_changes,
        ..
    } = decoded
    {
        assert_eq!(epoch, 1);
        assert_eq!(seq_id, 100);
        assert_eq!(decoded_changes, changes);
    } else {
        panic!("Decoded packet must be DeltaOp");
    }
}
