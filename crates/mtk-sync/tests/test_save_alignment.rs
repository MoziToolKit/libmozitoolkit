//! # Golden alignment: Anvil save import vs Live Sync baseline replay
//!
//! These tests lock down the contract that "live sync is just a streaming variant of
//! save import": for the same world region, a RFC v3.1 baseline stream (encoded exactly
//! like `CanonicalVoxelScanner` + the Java chunker) must assemble into a `VoxelStorage`
//! that is indistinguishable from `SaveLoader::load_box_into_storage`.
//!
//! Two layers:
//! 1. A hermetic synthetic world that always runs (CI-safe, no Minecraft assets).
//! 2. An opt-in real-save test gated on `MTK_TEST_SAVE` that compares an actual `.mca`
//!    import against the stream generated from that same box.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::IVec3;
use mtk_core::AttributeData;
use mtk_cull::FaceCuller;
use mtk_material::BiomeResolver;
use mtk_sync::{
    compute_sparse_state_hash_for_storage, LiveSyncSession, Packet, SyncDecision, SyncLifecycle,
};
use mtk_voxel::{
    MesherConfig, ModelSource, SectionBiomes, SectionMesher, SectionStorage, VoxelStorage,
};

const DIMENSION: &str = "minecraft:overworld";
const CHUNK_SIZE: usize = 256 * 1024;

// ---------------------------------------------------------------------------
// Baseline stream generator (mirrors CanonicalVoxelScanner + the Java chunker)
// ---------------------------------------------------------------------------

struct BaselineStream {
    packets: Vec<Packet>,
    /// World quart-cell coordinates that actually appear in the biome stream.
    sampled_cells: HashSet<IVec3>,
}

/// Encodes `storage[min ..= max]` into the exact packet sequence a v3.1 server emits.
fn build_baseline_stream(
    storage: &VoxelStorage,
    min: IVec3,
    size: IVec3,
    epoch: u64,
    snapshot_seq: u64,
    snapshot_uuid: [u8; 16],
) -> BaselineStream {
    let state_hash = compute_sparse_state_hash_for_storage(storage, min, size, DIMENSION);

    // --- block stream: X fastest, Z next, Y slowest; air is always palette slot 0 ---
    let total_voxels = (size.x as usize) * (size.y as usize) * (size.z as usize);
    let mut block_palette = vec!["minecraft:air".to_string()];
    let mut block_lookup: HashMap<String, u16> = HashMap::new();
    block_lookup.insert("minecraft:air".to_string(), 0);
    let mut voxel_indices: Vec<u16> = Vec::with_capacity(total_voxels);
    for y in min.y..min.y + size.y {
        for z in min.z..min.z + size.z {
            for x in min.x..min.x + size.x {
                let state = storage.get_block(x, y, z);
                let idx = *block_lookup.entry(state.to_string()).or_insert_with(|| {
                    block_palette.push(state.to_string());
                    (block_palette.len() - 1) as u16
                });
                voxel_indices.push(idx);
            }
        }
    }
    let palette_bits: u8 = if block_palette.len() <= 256 { 8 } else { 16 };

    let mut payload: Vec<u8> = Vec::with_capacity(total_voxels * (palette_bits as usize / 8));
    if palette_bits == 8 {
        for idx in &voxel_indices {
            payload.push(*idx as u8);
        }
    } else {
        for idx in &voxel_indices {
            payload.extend_from_slice(&idx.to_le_bytes());
        }
    }

    // --- biome stream: qy -> qz -> qx, sampled at `min + 4q` ---
    let qw = (size.x + 3) / 4;
    let qh = (size.y + 3) / 4;
    let qd = (size.z + 3) / 4;

    let mut biome_palette: Vec<String> = Vec::new();
    let mut biome_lookup: HashMap<String, u8> = HashMap::new();
    let mut biome_indices: Vec<u8> = Vec::new();
    let mut sampled_cells: HashSet<IVec3> = HashSet::new();
    for qy in 0..qh {
        for qz in 0..qd {
            for qx in 0..qw {
                let wx = min.x + qx * 4;
                let wy = min.y + qy * 4;
                let wz = min.z + qz * 4;
                sampled_cells.insert(IVec3::new(wx >> 2, wy >> 2, wz >> 2));
                let biome = storage.get_biome(wx, wy, wz).to_string();
                let idx = *biome_lookup.entry(biome.clone()).or_insert_with(|| {
                    biome_palette.push(biome);
                    (biome_palette.len() - 1) as u8
                });
                biome_indices.push(idx);
            }
        }
    }
    let biome_bits: u8 = if biome_palette.len() > 1 { 8 } else { 0 };
    if biome_bits > 0 {
        payload.extend_from_slice(&biome_indices);
    }

    // --- fixed 256 KiB chunking (WebSocketServerManager.streamBaseline) ---
    let bytes_per_voxel = std::cmp::max(1, palette_bits as usize / 8);
    let voxels_per_chunk = CHUNK_SIZE / bytes_per_voxel;
    let total_bytes = payload.len();
    let total_chunks = std::cmp::max(1, total_bytes.div_ceil(CHUNK_SIZE)) as u32;

    let mut packets = Vec::with_capacity(total_chunks as usize + 2);
    packets.push(Packet::BaselineBegin {
        epoch,
        snapshot_uuid,
        snapshot_seq,
        min_pos: min,
        size,
        state_hash,
        palette_revision: 1,
        total_chunks,
        total_encoded_bytes: total_bytes as u64,
        total_decoded_bytes: total_bytes as u64,
        compression_mode: 0,
        palette_bits_per_block: palette_bits,
        biome_bits_per_quart: biome_bits,
        dimension: DIMENSION.to_string(),
        block_palette: block_palette.clone(),
        biome_palette: biome_palette.clone(),
    });

    for chunk_index in 0..total_chunks {
        let offset = chunk_index as usize * CHUNK_SIZE;
        let len = std::cmp::min(CHUNK_SIZE, total_bytes - offset);
        let chunk_payload = payload[offset..offset + len].to_vec();
        // Java clamps the cursor to totalVoxels (biome-only trailing chunks).
        let first_voxel_index = std::cmp::min(
            chunk_index as u64 * voxels_per_chunk as u64,
            total_voxels as u64,
        );
        let voxel_count = std::cmp::min(
            voxels_per_chunk as u64,
            (total_voxels as u64).saturating_sub(first_voxel_index),
        ) as u32;

        let mut hasher = xxhash_rust::xxh64::Xxh64::new(0);
        hasher.update(&chunk_payload);

        packets.push(Packet::BaselineChunk {
            epoch,
            snapshot_uuid,
            chunk_index,
            total_chunks,
            first_voxel_index,
            voxel_count,
            payload_len: len as u32,
            decoded_target_bytes: len as u32,
            chunk_xxhash64: hasher.digest(),
            payload: chunk_payload,
        });
    }

    packets.push(Packet::BaselineEnd {
        epoch,
        snapshot_uuid,
        snapshot_seq,
        total_chunks_sent: total_chunks,
        final_state_hash: state_hash,
        status: 0,
    });

    BaselineStream {
        packets,
        sampled_cells,
    }
}

/// Replays a baseline stream through the real `LiveSyncSession` and returns the
/// assembled storage plus the world quart cells covered by the biome stream.
fn replay_baseline(
    storage: &VoxelStorage,
    min: IVec3,
    size: IVec3,
) -> (VoxelStorage, HashSet<IVec3>) {
    let epoch = 1u64;
    let snapshot_seq = 10u64;
    let stream = build_baseline_stream(storage, min, size, epoch, snapshot_seq, [7u8; 16]);

    let session = LiveSyncSession::new(None, None, None);
    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [3u8; 16],
        epoch,
        server_seq: snapshot_seq,
        current_min: min,
        current_size: size,
        current_state_hash: 0,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: DIMENSION.to_string(),
    });
    for packet in stream.packets {
        session.process_packet_direct(packet);
    }
    assert_eq!(
        session.lifecycle(),
        SyncLifecycle::Live,
        "baseline replay must validate its canonical state hash"
    );
    (session.get_storage(), stream.sampled_cells)
}

// ---------------------------------------------------------------------------
// Synthetic hermetic world
// ---------------------------------------------------------------------------

const SURFACE_BIOMES: [&str; 3] = ["minecraft:plains", "minecraft:desert", "minecraft:taiga"];

fn hash2(x: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (z as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

fn surface_height(x: i32, z: i32) -> i32 {
    (hash2(x >> 2, z >> 2) % 11) as i32 + 6
}

fn surface_biome(x: i32, z: i32) -> &'static str {
    SURFACE_BIOMES[(hash2(x >> 5, z >> 5) % SURFACE_BIOMES.len() as u32) as usize]
}

/// Builds a chunk-aligned world with fully-specified 4x4x4 section biomes, exactly like
/// an Anvil chunk: horizontal surface biomes, underground cave biomes and a real surface.
fn build_synthetic_world(min: IVec3, size: IVec3) -> VoxelStorage {
    let mut storage = VoxelStorage::new();
    storage.set_bounds(min.x, min.y, min.z, size.x, size.y, size.z);

    let (min_sx, max_sx) = (min.x >> 4, (min.x + size.x - 1) >> 4);
    let (min_sy, max_sy) = (min.y >> 4, (min.y + size.y - 1) >> 4);
    let (min_sz, max_sz) = (min.z >> 4, (min.z + size.z - 1) >> 4);

    for sy in min_sy..=max_sy {
        for sz in min_sz..=max_sz {
            for sx in min_sx..=max_sx {
                let coord = IVec3::new(sx, sy, sz);
                let mut section = SectionStorage::new(coord);

                for lx in 0..16 {
                    for lz in 0..16 {
                        let wx = sx * 16 + lx;
                        let wz = sz * 16 + lz;
                        let surface = surface_height(wx, wz);
                        for ly in 0..16 {
                            let wy = sy * 16 + ly;
                            let state = if wy < surface - 1 {
                                "minecraft:stone"
                            } else if wy == surface - 1 {
                                "minecraft:grass_block"
                            } else {
                                "minecraft:air"
                            };
                            if state != "minecraft:air" {
                                section.set_local(lx as usize, ly as usize, lz as usize, state);
                            }
                        }
                    }
                }

                let mut biomes = SectionBiomes::blank(Vec::new());
                for qy in 0..4 {
                    for qz in 0..4 {
                        for qx in 0..4 {
                            let wx = sx * 16 + qx * 4;
                            let wy = sy * 16 + qy * 4;
                            let wz = sz * 16 + qz * 4;
                            // Below the surface -> cave biome, otherwise the surface biome.
                            let biome = if wy + 3 < surface_height(wx, wz) {
                                "minecraft:lush_caves"
                            } else {
                                surface_biome(wx, wz)
                            };
                            biomes.set_sampled_quart_biome(
                                qx as usize,
                                qy as usize,
                                qz as usize,
                                biome,
                            );
                        }
                    }
                }
                section.biomes = Some(biomes);
                storage.sections.insert(coord, section);
            }
        }
    }

    storage.finalize_biome_metadata();
    storage
}

/// Applies the RFC v3.1 canonical block-state normalisation (air family -> air, the same
/// transform the server applies via `CanonicalVoxelScanner.getCanonicalBlock`). Save imports
/// keep raw states such as `bubble_column[drag=true]`, so the golden comparison must run on
/// the canonical form that actually travels over the wire.
fn canonicalize_blocks(storage: &VoxelStorage) -> VoxelStorage {
    let mut out = VoxelStorage::new();
    out.min_x = storage.min_x;
    out.min_y = storage.min_y;
    out.min_z = storage.min_z;
    out.size_x = storage.size_x;
    out.size_y = storage.size_y;
    out.size_z = storage.size_z;
    out.has_explicit_bounds = storage.has_explicit_bounds;
    for (coord, section) in &storage.sections {
        let mut canonical = SectionStorage::new(*coord);
        for lx in 0..16 {
            for ly in 0..16 {
                for lz in 0..16 {
                    canonical.set_local(lx, ly, lz, section.get_local_state(lx, ly, lz));
                }
            }
        }
        canonical.biomes = section.biomes.clone();
        out.sections.insert(*coord, canonical);
    }
    out.biome_column_map = storage.biome_column_map.clone();
    out.primary_biome = storage.primary_biome.clone();
    out
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

fn assert_blocks_equal(expected: &VoxelStorage, actual: &VoxelStorage, min: IVec3, size: IVec3) {
    for y in min.y..min.y + size.y {
        for z in min.z..min.z + size.z {
            for x in min.x..min.x + size.x {
                assert_eq!(
                    expected.get_block(x, y, z),
                    actual.get_block(x, y, z),
                    "block mismatch at ({x},{y},{z})"
                );
            }
        }
    }
}

fn assert_biomes_equal_on_sampled_cells(
    expected: &VoxelStorage,
    actual: &VoxelStorage,
    sampled: &HashSet<IVec3>,
    min: IVec3,
    size: IVec3,
) {
    for y in min.y..min.y + size.y {
        for z in min.z..min.z + size.z {
            for x in min.x..min.x + size.x {
                if !sampled.contains(&IVec3::new(x >> 2, y >> 2, z >> 2)) {
                    continue;
                }
                assert_eq!(
                    expected.get_biome(x, y, z),
                    actual.get_biome(x, y, z),
                    "biome mismatch at sampled position ({x},{y},{z})"
                );
            }
        }
    }
}

fn assert_all_sections_fully_assigned(storage: &VoxelStorage) {
    for (coord, section) in storage.sections.iter() {
        // All-air sections are not required to carry biome data (the Anvil loader drops
        // them too); only sections with voxels must be self-describing.
        if section.is_empty() {
            continue;
        }
        let biomes = section
            .biomes
            .as_ref()
            .unwrap_or_else(|| panic!("section {coord:?} lost its biome container"));
        assert!(
            biomes.is_fully_assigned(),
            "section {coord:?} has unassigned quart cells (placeholder leak)"
        );
    }
}

fn assert_biome_metadata_equal(expected: &VoxelStorage, actual: &VoxelStorage) {
    let mut diffs: Vec<([i32; 2], String, String)> = Vec::new();
    for (key, expected_biome) in &expected.biome_column_map {
        match actual.biome_column_map.get(key) {
            Some(actual_biome) if actual_biome == expected_biome => {}
            Some(actual_biome) => diffs.push((*key, expected_biome.clone(), actual_biome.clone())),
            None => diffs.push((*key, expected_biome.clone(), "<missing>".to_string())),
        }
    }
    for (key, actual_biome) in &actual.biome_column_map {
        if !expected.biome_column_map.contains_key(key) {
            diffs.push((*key, "<missing>".to_string(), actual_biome.clone()));
        }
    }
    diffs.sort();
    assert!(
        diffs.is_empty(),
        "surface column biome map diverged in {} of {} columns; first 8: {:?}",
        diffs.len(),
        expected.biome_column_map.len(),
        &diffs[..diffs.len().min(8)]
    );
    assert_eq!(
        expected.primary_biome, actual.primary_biome,
        "dominant primary biome diverged"
    );
}

fn tint_attributes(mesh: &mtk_core::MeshData) -> Vec<([i32; 3], [f32; 4])> {
    let colors = match mesh
        .custom_attributes
        .get("mtk_biome_tint_color")
        .map(|a| &a.data)
    {
        Some(AttributeData::Float4(v)) => v,
        _ => return Vec::new(),
    };
    let coords = |name: &str| -> Vec<i32> {
        match mesh.custom_attributes.get(name).map(|a| &a.data) {
            Some(AttributeData::Int32(v)) => v.clone(),
            _ => Vec::new(),
        }
    };
    let xs = coords("mtk_block_x");
    let ys = coords("mtk_block_y");
    let zs = coords("mtk_block_z");
    colors
        .iter()
        .enumerate()
        .map(|(i, color)| {
            (
                [
                    xs.get(i).copied().unwrap_or(0),
                    ys.get(i).copied().unwrap_or(0),
                    zs.get(i).copied().unwrap_or(0),
                ],
                *color,
            )
        })
        .collect()
}

fn assert_mesh_tint_equal(expected: &VoxelStorage, actual: &VoxelStorage) {
    let config = MesherConfig {
        biome_resolver: Some(Arc::new(BiomeResolver::new())),
        origin_centered: false,
        ..Default::default()
    };
    let culler = FaceCuller::default();

    let mut coords: Vec<IVec3> = expected
        .sections
        .keys()
        .chain(actual.sections.keys())
        .copied()
        .collect();
    coords.sort_by_key(|c| (c.y, c.z, c.x));
    coords.dedup();

    for coord in coords {
        let mesh_of = |storage: &VoxelStorage| {
            let padded = storage.get_section_padded_array(coord);
            SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config)
        };
        let a = tint_attributes(&mesh_of(expected));
        let b = tint_attributes(&mesh_of(actual));
        assert_eq!(a, b, "per-face biome tint diverged in section {coord:?}");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Hermetic golden test: save-style ground truth vs baseline replay on a quart-aligned
/// (but not section-aligned) selection. Everything must match exactly.
#[test]
fn test_baseline_replay_matches_save_style_storage_quart_aligned() {
    let min = IVec3::new(4, -16, 8);
    let size = IVec3::new(48, 32, 48);
    let expected = build_synthetic_world(min, size);
    let (actual, sampled) = replay_baseline(&expected, min, size);

    assert_blocks_equal(&expected, &actual, min, size);
    assert_biomes_equal_on_sampled_cells(&expected, &actual, &sampled, min, size);
    assert_all_sections_fully_assigned(&actual);
    assert_biome_metadata_equal(&expected, &actual);
    assert_mesh_tint_equal(&expected, &actual);
}

/// Hermetic golden test on a selection whose bounds cut through quart cells (min is not
/// 4-aligned), while still fully covering every intersected quart cell. Exercises the
/// section-local quart anchoring and partial-border sections; everything must match exactly.
#[test]
fn test_baseline_replay_matches_save_style_storage_unaligned() {
    let min = IVec3::new(5, -16, 9);
    let size = IVec3::new(43, 32, 43);
    let expected = build_synthetic_world(min, size);
    let (actual, sampled) = replay_baseline(&expected, min, size);

    assert_blocks_equal(&expected, &actual, min, size);
    assert_biomes_equal_on_sampled_cells(&expected, &actual, &sampled, min, size);
    assert_all_sections_fully_assigned(&actual);
    assert_biome_metadata_equal(&expected, &actual);
    assert_mesh_tint_equal(&expected, &actual);
}

/// When the selection's maximum edge falls into a quart cell that the selection-anchored
/// sample grid cannot reach (possible when `min % 4 != 0`), the protocol cannot transmit
/// that cell. The contract is then: every **sampled** cell must match exactly and the
/// section must still be filled from the nearest sample instead of leaking palette[0].
#[test]
fn test_baseline_replay_partial_edge_falls_back_to_nearest_sample() {
    let min = IVec3::new(5, -13, 9);
    // max_y = 16 lands in quart cell 4 while the last sample is at y=15 (cell 3).
    let size = IVec3::new(46, 30, 47);
    let expected = build_synthetic_world(min, size);
    let (actual, sampled) = replay_baseline(&expected, min, size);

    assert_blocks_equal(&expected, &actual, min, size);
    assert_biomes_equal_on_sampled_cells(&expected, &actual, &sampled, min, size);
    assert_all_sections_fully_assigned(&actual);
}

/// Opt-in end-to-end test against a real Anvil save (`MTK_TEST_SAVE`).
///
/// It imports a bounded box with `SaveLoader`, generates the live-sync stream from that
/// very storage (exactly what a server would send for the same region) and asserts the
/// assembled storage and per-face biome tint are indistinguishable.
#[test]
fn test_real_save_import_and_live_sync_align() {
    let Some(save_dir) = mtk_testkit::save_world() else {
        eprintln!("Skipping: no Minecraft save available (set MTK_TEST_SAVE)");
        return;
    };
    let min = IVec3::new(-32, -64, -32);
    let size = IVec3::new(64, 192, 64);

    let mut raw = VoxelStorage::new();
    mtk_save::SaveLoader::load_box_into_storage(
        &save_dir,
        "overworld",
        min,
        min + size - IVec3::ONE,
        &mut raw,
    )
    .expect("save import must succeed");
    let expected = canonicalize_blocks(&raw);

    let (actual, sampled) = replay_baseline(&expected, min, size);

    assert_blocks_equal(&expected, &actual, min, size);
    assert_biomes_equal_on_sampled_cells(&expected, &actual, &sampled, min, size);
    assert_all_sections_fully_assigned(&actual);
    assert_biome_metadata_equal(&expected, &actual);
    assert_mesh_tint_equal(&expected, &actual);
}
