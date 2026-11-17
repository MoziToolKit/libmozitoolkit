//! Security regression tests for baseline stream assembler unbounded memory guards (S-03 & S-04).

use glam::IVec3;
use mtk_sync::protocol::*;
use mtk_sync::BaselineAssembler;

#[test]
fn test_baseline_begin_with_max_chunks_rejected() {
    let begin_pkt = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 1,
        min_pos: IVec3::new(0, 0, 0),
        size: IVec3::new(2, 2, 2),
        state_hash: 0,
        palette_revision: 1,
        total_chunks: u32::MAX,
        total_encoded_bytes: 100,
        total_decoded_bytes: 100,
        compression_mode: 0,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 0,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    };

    let result = BaselineAssembler::new(&begin_pkt);
    assert!(
        result.is_err(),
        "BASELINE_BEGIN with total_chunks = u32::MAX must be rejected with Err"
    );
}

#[test]
fn test_baseline_chunk_with_max_decoded_bytes_rejected() {
    let begin_pkt = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 1,
        min_pos: IVec3::new(0, 0, 0),
        size: IVec3::new(2, 2, 2),
        state_hash: 0,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: 100,
        total_decoded_bytes: 8,
        compression_mode: 1, // zstd
        palette_bits_per_block: 8,
        biome_bits_per_quart: 0,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    };

    let mut assembler = BaselineAssembler::new(&begin_pkt).expect("Assembler init should succeed");

    // Attacker-controlled chunk declaring decoded_target_bytes = u32::MAX
    let valid_zstd = zstd::encode_all(&[0u8; 8][..], 0).unwrap();
    let chunk_pkt = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: valid_zstd.len() as u32,
        decoded_target_bytes: u32::MAX,
        chunk_xxhash64: 0,
        payload: valid_zstd,
    };

    let result = assembler.process_chunk(&chunk_pkt);
    assert!(
        result.is_err(),
        "BASELINE_CHUNK with decoded_target_bytes = u32::MAX must be rejected with Err"
    );
}

#[test]
fn test_uncompressed_chunk_with_max_decoded_bytes_rejected() {
    let begin_pkt = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 1,
        min_pos: IVec3::new(0, 0, 0),
        size: IVec3::new(2, 2, 2),
        state_hash: 0,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: 8,
        total_decoded_bytes: 8,
        compression_mode: 0, // uncompressed
        palette_bits_per_block: 8,
        biome_bits_per_quart: 0,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    };

    let mut assembler = BaselineAssembler::new(&begin_pkt).expect("Assembler init should succeed");

    let chunk_pkt = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: 8,
        decoded_target_bytes: u32::MAX,
        chunk_xxhash64: 0,
        payload: vec![0u8; 8],
    };

    let result = assembler.process_chunk(&chunk_pkt);
    assert!(
        result.is_err(),
        "Uncompressed BASELINE_CHUNK with decoded_target_bytes = u32::MAX must be rejected with Err"
    );
}

#[test]
fn test_decompression_bomb_rejected_by_take_bound() {
    let begin_pkt = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 1,
        min_pos: IVec3::new(0, 0, 0),
        size: IVec3::new(2, 2, 2),
        state_hash: 0,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: 100,
        total_decoded_bytes: 8,
        compression_mode: 1, // zstd
        palette_bits_per_block: 8,
        biome_bits_per_quart: 0,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    };

    let mut assembler = BaselineAssembler::new(&begin_pkt).expect("Assembler init should succeed");

    // Encodes 1024 bytes, but chunk claims decoded_target_bytes is only 8 bytes
    let bomb_payload = zstd::encode_all(&[0u8; 1024][..], 0).unwrap();
    let chunk_pkt = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: bomb_payload.len() as u32,
        decoded_target_bytes: 8,
        chunk_xxhash64: 0,
        payload: bomb_payload,
    };

    let result = assembler.process_chunk(&chunk_pkt);
    assert!(
        result.is_err(),
        "Decompression bomb with actual decompressed bytes > decoded_target_bytes must be rejected with Err"
    );
}

#[test]
fn test_cumulative_voxel_indices_exceeding_volume_rejected() {
    let begin_pkt = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 1,
        min_pos: IVec3::new(0, 0, 0),
        size: IVec3::new(2, 2, 2), // 8 voxels total
        state_hash: 0,
        palette_revision: 1,
        total_chunks: 2,
        total_encoded_bytes: 16,
        total_decoded_bytes: 16,
        compression_mode: 0, // uncompressed
        palette_bits_per_block: 8,
        biome_bits_per_quart: 0,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    };

    let mut assembler = BaselineAssembler::new(&begin_pkt).expect("Assembler init should succeed");

    // Chunk 0: 6 voxels (valid, within total 8)
    let chunk_0 = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        chunk_index: 0,
        total_chunks: 2,
        first_voxel_index: 0,
        voxel_count: 6,
        payload_len: 6,
        decoded_target_bytes: 6,
        chunk_xxhash64: 0,
        payload: vec![0u8; 6],
    };
    assembler
        .process_chunk(&chunk_0)
        .expect("Chunk 0 should succeed");

    // Chunk 1: 3 voxels (6 + 3 = 9 > 8 total_voxels)
    let chunk_1 = Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        chunk_index: 1,
        total_chunks: 2,
        first_voxel_index: 6,
        voxel_count: 3,
        payload_len: 3,
        decoded_target_bytes: 3,
        chunk_xxhash64: 0,
        payload: vec![0u8; 3],
    };

    let result = assembler.process_chunk(&chunk_1);
    assert!(
        result.is_err(),
        "Chunk pushing cumulative voxel count over volume limit must return Err"
    );
}
