//! Regression tests for baseline assembly alignment, sequence handling and
//! state-hash validation robustness.

use glam::IVec3;
use mtk_sync::protocol::constants::*;
use mtk_sync::protocol::hasher::{compute_sparse_state_hash, SparseBlockEntry};
use mtk_sync::protocol::packet::*;
use mtk_sync::{BaselineAssembler, LiveSyncSession, SyncLifecycle};

/// Builds a single-chunk baseline payload for `size` voxels plus optional biome indices.
fn build_chunk(voxel_indices: &[u8], biome_indices: &[u8]) -> (Vec<u8>, u64) {
    let mut payload = Vec::with_capacity(voxel_indices.len() + biome_indices.len());
    payload.extend_from_slice(voxel_indices);
    payload.extend_from_slice(biome_indices);

    let mut hasher = xxhash_rust::xxh64::Xxh64::new(0);
    hasher.update(&payload);
    (payload, hasher.digest())
}

fn send_baseline(
    session: &LiveSyncSession,
    min_pos: IVec3,
    size: IVec3,
    voxel_indices: &[u8],
    biome_indices: &[u8],
    biome_bits: u8,
    state_hash: u64,
) {
    let total_voxels = (size.x * size.y * size.z) as usize;
    assert_eq!(total_voxels, voxel_indices.len());

    let (payload, chunk_hash) = build_chunk(voxel_indices, biome_indices);
    let block_palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let biome_palette = vec![
        "minecraft:plains".to_string(),
        "minecraft:desert".to_string(),
    ];

    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [7u8; 16],
        epoch: 1,
        server_seq: 10,
        current_min: min_pos,
        current_size: size,
        current_state_hash: state_hash,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: "minecraft:overworld".to_string(),
    });

    session.process_packet_direct(Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [9u8; 16],
        snapshot_seq: 10,
        min_pos,
        size,
        state_hash,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: payload.len() as u64,
        total_decoded_bytes: payload.len() as u64,
        compression_mode: 0,
        palette_bits_per_block: 8,
        biome_bits_per_quart: biome_bits,
        dimension: "minecraft:overworld".to_string(),
        block_palette,
        biome_palette,
    });

    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [9u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: total_voxels as u32,
        payload_len: payload.len() as u32,
        decoded_target_bytes: payload.len() as u32,
        chunk_xxhash64: chunk_hash,
        payload,
    });

    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [9u8; 16],
        snapshot_seq: 10,
        total_chunks_sent: 1,
        final_state_hash: state_hash,
        status: 0,
    });
}

#[test]
fn test_unaligned_selection_with_multi_biomes_validates_state_hash() {
    let session = LiveSyncSession::new(None, None, None);

    // Deliberately NOT 16-aligned on X (4 % 16 != 0).
    let min_pos = IVec3::new(4, 0, 0);
    let size = IVec3::new(8, 1, 1);
    let voxels = [1u8; 8];
    // Two quart columns: qx=0 -> plains (wx=4..7), qx=1 -> desert (wx=8..11)
    let biomes = [0u8, 1u8];

    let entries = (0..8).map(|i| SparseBlockEntry {
        rel_x: i,
        rel_y: 0,
        rel_z: 0,
        state: "minecraft:stone",
        biome: if i < 4 {
            "minecraft:plains"
        } else {
            "minecraft:desert"
        },
        entity_json: None,
    });
    let expected_hash = compute_sparse_state_hash(min_pos, size, "minecraft:overworld", entries);

    send_baseline(&session, min_pos, size, &voxels, &biomes, 8, expected_hash);

    assert_eq!(
        session.lifecycle(),
        SyncLifecycle::Live,
        "Baseline on an unaligned selection must validate its canonical state hash"
    );
}

#[test]
fn test_arbitrary_unaligned_multi_biome_selection_quart_grid() {
    let session = LiveSyncSession::new(None, None, None);

    // Deliberately choose min_pos and size where min % 4 != 0 and the bounding box
    // straddles intra-quart boundaries:
    // x in [3, 5): block 3 is in quart 0 (0..3), block 4 is in quart 1 (4..7).
    // Total quarts on X: (4 >> 2) - (3 >> 2) + 1 = 2 quarts! (Old formula gave (2+3)/4 = 1).
    let min_pos = IVec3::new(3, 0, 0);
    let size = IVec3::new(2, 1, 1);
    let voxels = [1u8; 2];
    // 2 quart columns: qx=0 -> plains, qx=1 -> desert
    let biomes = [0u8, 1u8];

    let entries = [
        SparseBlockEntry {
            rel_x: 0, // x = 3, in quart 0
            rel_y: 0,
            rel_z: 0,
            state: "minecraft:stone",
            biome: "minecraft:plains",
            entity_json: None,
        },
        SparseBlockEntry {
            rel_x: 1, // x = 4, in quart 1
            rel_y: 0,
            rel_z: 0,
            state: "minecraft:stone",
            biome: "minecraft:desert",
            entity_json: None,
        },
    ];
    let expected_hash = compute_sparse_state_hash(min_pos, size, "minecraft:overworld", entries);

    send_baseline(&session, min_pos, size, &voxels, &biomes, 8, expected_hash);

    assert_eq!(
        session.lifecycle(),
        SyncLifecycle::Live,
        "Non-4-aligned selection with multiple biomes must validate state hash"
    );
}

#[test]
fn test_superseded_baseline_frames_are_ignored() {
    let session = LiveSyncSession::new(None, None, None);

    let min_pos = IVec3::new(0, 0, 0);
    let size = IVec3::new(8, 1, 1);
    let voxels = [0u8; 8];
    let expected_hash = compute_sparse_state_hash(
        min_pos,
        size,
        "minecraft:overworld",
        std::iter::empty::<SparseBlockEntry>(),
    );
    let (payload, chunk_hash) = build_chunk(&voxels, &[]);
    let block_palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let biome_palette = vec!["minecraft:plains".to_string()];

    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [7u8; 16],
        epoch: 1,
        server_seq: 10,
        current_min: min_pos,
        current_size: size,
        current_state_hash: expected_hash,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: "minecraft:overworld".to_string(),
    });

    let begin = |uuid: [u8; 16], seq: u64| Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: uuid,
        snapshot_seq: seq,
        min_pos,
        size,
        state_hash: expected_hash,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: payload.len() as u64,
        total_decoded_bytes: payload.len() as u64,
        compression_mode: 0,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette: block_palette.clone(),
        biome_palette: biome_palette.clone(),
    };

    // Stream A starts, then is superseded by stream B (the user moved the selection).
    session.process_packet_direct(begin([1u8; 16], 10));
    session.process_packet_direct(begin([2u8; 16], 11));

    // A late chunk from stream A must be ignored, not corrupt stream B.
    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: payload.len() as u32,
        decoded_target_bytes: payload.len() as u32,
        chunk_xxhash64: chunk_hash,
        payload: payload.clone(),
    });

    // A late END from stream A must be ignored as well (previously left the
    // lifecycle stuck in ValidatingHash / progress at 75%).
    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [1u8; 16],
        snapshot_seq: 10,
        total_chunks_sent: 1,
        final_state_hash: expected_hash,
        status: 0,
    });
    assert_ne!(session.lifecycle(), SyncLifecycle::Live);

    // Stream B completes normally.
    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [2u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: payload.len() as u32,
        decoded_target_bytes: payload.len() as u32,
        chunk_xxhash64: chunk_hash,
        payload,
    });
    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [2u8; 16],
        snapshot_seq: 11,
        total_chunks_sent: 1,
        final_state_hash: expected_hash,
        status: 0,
    });

    assert_eq!(session.lifecycle(), SyncLifecycle::Live);
}

/// A live-sync selection is anchored at `min_pos`, so its quart grid only covers part of
/// the sections it touches. Cells outside the sampling grid must be filled from the
/// nearest sampled biome instead of silently inheriting palette[0] (the selection's
/// min corner, usually deep_dark/lush_caves). Otherwise border sections flip between the
/// 2D (256) and 3D (1024) smoothing modes and leak the wrong tint.
#[test]
fn test_partial_section_biomes_are_filled_from_nearest_sample() {
    let session = LiveSyncSession::new(None, None, None);

    // 8x1x1 selection starting at x=4: section (0,0,0) quart cells 1 and 2 are sampled,
    // cells 0 and 3 are not. Palette is built in scan order: [plains, desert].
    let min_pos = IVec3::new(4, 0, 0);
    let size = IVec3::new(8, 1, 1);
    let voxels = [0u8; 8];
    let biomes = [0u8, 1u8];

    let expected_hash = compute_sparse_state_hash(
        min_pos,
        size,
        "minecraft:overworld",
        std::iter::empty::<SparseBlockEntry>(),
    );

    send_baseline(&session, min_pos, size, &voxels, &biomes, 8, expected_hash);
    assert_eq!(session.lifecycle(), SyncLifecycle::Live);

    let storage = session.get_storage();
    let sec = storage
        .sections
        .get(&IVec3::new(0, 0, 0))
        .expect("section must exist");
    let sb = sec.biomes.as_ref().expect("section biomes must be present");

    // Sampled cells keep their streamed values.
    assert_eq!(sb.get_quart_biome(1, 0, 0), Some("minecraft:plains"));
    assert_eq!(sb.get_quart_biome(2, 0, 0), Some("minecraft:desert"));

    // Unsampled cells are filled from the nearest sample, never left as palette[0] for
    // the whole section. Cell 3 is closest to the desert sample at cell 2.
    assert!(sb.is_fully_assigned());
    assert_eq!(sb.get_quart_biome(0, 0, 0), Some("minecraft:plains"));
    assert_eq!(sb.get_quart_biome(3, 0, 0), Some("minecraft:desert"));
    assert!(!sb.varies_vertically());

    // The shared metadata derivation must now populate the same surface column map and
    // primary biome that an Anvil save import would produce.
    assert_eq!(
        storage.biome_column_map.len(),
        8,
        "column map must cover the selection's 8 block columns"
    );
    assert_eq!(
        storage.biome_column_map.get(&[4, 0]).map(String::as_str),
        Some("minecraft:plains")
    );
    assert_eq!(
        storage.biome_column_map.get(&[8, 0]).map(String::as_str),
        Some("minecraft:desert")
    );
    assert!(storage.primary_biome.is_some());
}

/// The server splits its payload on fixed byte offsets, so a large multi-biome selection
/// can produce trailing chunks whose `first_voxel_index` is `k * voxelsPerChunk` while
/// `voxel_count == 0`. Those are pure biome continuation chunks and must not be treated
/// as a sequence gap (which used to put the client into an endless resync loop).
#[test]
fn test_trailing_biome_only_chunk_is_accepted() {
    let min_pos = IVec3::new(0, 0, 0);
    let size = IVec3::new(8, 1, 1);
    // 2 quart cells along X, 1 along Y/Z => 2 biome indices.
    let block_palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let biome_palette = vec![
        "minecraft:plains".to_string(),
        "minecraft:desert".to_string(),
    ];

    let begin = Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [5u8; 16],
        snapshot_seq: 3,
        min_pos,
        size,
        state_hash: 0,
        palette_revision: 1,
        total_chunks: 2,
        total_encoded_bytes: 10,
        total_decoded_bytes: 10,
        compression_mode: 0,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette,
        biome_palette,
    };
    let mut assembler = BaselineAssembler::new(&begin).expect("valid begin");

    let voxel_payload = vec![0u8; 8];
    let (_, voxel_hash) = build_chunk(&voxel_payload, &[]);
    assembler
        .process_chunk(&Packet::BaselineChunk {
            epoch: 1,
            snapshot_uuid: [5u8; 16],
            chunk_index: 0,
            total_chunks: 2,
            first_voxel_index: 0,
            voxel_count: 8,
            payload_len: voxel_payload.len() as u32,
            decoded_target_bytes: voxel_payload.len() as u32,
            chunk_xxhash64: voxel_hash,
            payload: voxel_payload,
        })
        .expect("voxel chunk must be accepted");

    // Trailing biome-only chunk: first_voxel_index is a byte-offset-derived cursor that
    // overshoots total_voxels (as produced by the Java server's chunker).
    let biome_payload = vec![0u8, 1u8];
    let (_, biome_hash) = build_chunk(&biome_payload, &[]);
    assembler
        .process_chunk(&Packet::BaselineChunk {
            epoch: 1,
            snapshot_uuid: [5u8; 16],
            chunk_index: 1,
            total_chunks: 2,
            first_voxel_index: 16,
            voxel_count: 0,
            payload_len: biome_payload.len() as u32,
            decoded_target_bytes: biome_payload.len() as u32,
            chunk_xxhash64: biome_hash,
            payload: biome_payload,
        })
        .expect("biome-only continuation chunk must not be a sequence gap");

    assert_eq!(assembler.chunks_received, 2);
    assert_eq!(assembler.expected_next_voxel_index, 8);
    assert_eq!(assembler.raw_quart_biome_indices, vec![0, 1]);
}
