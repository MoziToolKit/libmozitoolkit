//! # Canonical State Hash Calculator (RFC v3.1 / v3.2 Section 3)
//!
//! Provides the Single Source of Truth (SSOT) for world state verification using xxHash64.

use glam::IVec3;
use xxhash_rust::xxh64::Xxh64;

use mtk_model::normalize_canonical_blockstate;

/// Canonical encoding version for sparse state hash: 0x02.
pub const SPARSE_CANONICAL_VERSION: u8 = 0x02;

/// Canonical Sparse Block Entry for authoritative fingerprinting.
pub struct SparseBlockEntry<'a> {
    pub rel_x: u32,
    pub rel_y: u32,
    pub rel_z: u32,
    pub state: &'a str,
    pub biome: &'a str,
    pub entity_json: Option<&'a str>,
}

/// Computes the authoritative canonical sparse state_hash across the 3D bounding volume.
///
/// Only non-air blocks are hashed. Air blocks and empty spaces are completely ignored.
/// Non-air blocks MUST be provided in canonical (Y-slowest, Z-next, X-fastest) order.
pub fn compute_sparse_state_hash<'a, I>(
    min_pos: IVec3,
    size: IVec3,
    dimension_str: &str,
    non_air_blocks: I,
) -> u64
where
    I: IntoIterator<Item = SparseBlockEntry<'a>>,
{
    let mut hasher = Xxh64::new(0);

    // 1. Metadata Header
    hasher.update(&[SPARSE_CANONICAL_VERSION]);
    let dim_bytes = dimension_str.as_bytes();
    hasher.update(&(dim_bytes.len() as u16).to_le_bytes());
    hasher.update(dim_bytes);
    hasher.update(&min_pos.x.to_le_bytes());
    hasher.update(&min_pos.y.to_le_bytes());
    hasher.update(&min_pos.z.to_le_bytes());
    hasher.update(&(size.x as u32).to_le_bytes());
    hasher.update(&(size.y as u32).to_le_bytes());
    hasher.update(&(size.z as u32).to_le_bytes());

    // 2. Stream of non-air blocks in (Y, Z, X) order
    for entry in non_air_blocks {
        hasher.update(&entry.rel_x.to_le_bytes());
        hasher.update(&entry.rel_y.to_le_bytes());
        hasher.update(&entry.rel_z.to_le_bytes());

        let norm_state = normalize_canonical_blockstate(entry.state);
        let state_bytes = norm_state.as_bytes();
        hasher.update(&(state_bytes.len() as u16).to_le_bytes());
        hasher.update(state_bytes);

        let trimmed_biome = entry.biome.trim();
        let biome_bytes = if trimmed_biome.is_empty() {
            b"minecraft:plains".as_slice()
        } else {
            trimmed_biome.as_bytes()
        };
        hasher.update(&(biome_bytes.len() as u16).to_le_bytes());
        hasher.update(biome_bytes);

        if let Some(json) = entry.entity_json.filter(|s| !s.trim().is_empty()) {
            let json_bytes = json.as_bytes();
            hasher.update(&(json_bytes.len() as u32).to_le_bytes());
            hasher.update(json_bytes);
        } else {
            hasher.update(&0u32.to_le_bytes());
        }
    }

    hasher.digest()
}

/// Computes the sparse authoritative canonical state_hash directly from a `VoxelStorage`.
pub fn compute_sparse_state_hash_for_storage(
    storage: &mtk_voxel::VoxelStorage,
    min_pos: IVec3,
    size: IVec3,
    dimension_str: &str,
) -> u64 {
    let mut hasher = Xxh64::new(0);

    // 1. Metadata Header
    hasher.update(&[SPARSE_CANONICAL_VERSION]);
    let dim_bytes = dimension_str.as_bytes();
    hasher.update(&(dim_bytes.len() as u16).to_le_bytes());
    hasher.update(dim_bytes);
    hasher.update(&min_pos.x.to_le_bytes());
    hasher.update(&min_pos.y.to_le_bytes());
    hasher.update(&min_pos.z.to_le_bytes());
    hasher.update(&(size.x as u32).to_le_bytes());
    hasher.update(&(size.y as u32).to_le_bytes());
    hasher.update(&(size.z as u32).to_le_bytes());

    let max_x = min_pos.x + size.x;
    let max_y = min_pos.y + size.y;
    let max_z = min_pos.z + size.z;

    for y in min_pos.y..max_y {
        let sec_y = y.div_euclid(16);
        let ly = y.rem_euclid(16) as usize;
        let rel_y = (y - min_pos.y) as u32;

        for z in min_pos.z..max_z {
            let sec_z = z.div_euclid(16);
            let lz = z.rem_euclid(16) as usize;
            let rel_z = (z - min_pos.z) as u32;

            let mut x = min_pos.x;
            while x < max_x {
                let sec_x = x.div_euclid(16);
                let next_boundary_x = (sec_x + 1) * 16;
                let next_x = max_x.min(next_boundary_x);

                let sec_coord = IVec3::new(sec_x, sec_y, sec_z);
                if let Some(sec) = storage.sections.get(&sec_coord) {
                    if !sec.is_empty() {
                        for cur_x in x..next_x {
                            let lx = cur_x.rem_euclid(16) as usize;
                            let state = sec.get_local_state(lx, ly, lz);
                            let norm_state = normalize_canonical_blockstate(state);
                            if norm_state != "minecraft:air" {
                                let rel_x = (cur_x - min_pos.x) as u32;
                                hasher.update(&rel_x.to_le_bytes());
                                hasher.update(&rel_y.to_le_bytes());
                                hasher.update(&rel_z.to_le_bytes());

                                let state_bytes = norm_state.as_bytes();
                                hasher.update(&(state_bytes.len() as u16).to_le_bytes());
                                hasher.update(state_bytes);

                                let biome = sec
                                    .biomes
                                    .as_ref()
                                    .and_then(|b| b.get_biome(lx, ly, lz))
                                    .unwrap_or_else(|| storage.get_biome(cur_x, y, z))
                                    .trim();
                                let biome_bytes = if biome.is_empty() {
                                    b"minecraft:plains".as_slice()
                                } else {
                                    biome.as_bytes()
                                };
                                hasher.update(&(biome_bytes.len() as u16).to_le_bytes());
                                hasher.update(biome_bytes);

                                let local_idx = mtk_voxel::block_index(lx, ly, lz) as u16;
                                if let Some(entity_data) = sec.get_block_entity(local_idx) {
                                    let json_str = entity_data.to_visual_json();
                                    if !json_str.trim().is_empty() {
                                        let json_bytes = json_str.as_bytes();
                                        hasher.update(&(json_bytes.len() as u32).to_le_bytes());
                                        hasher.update(json_bytes);
                                    } else {
                                        hasher.update(&0u32.to_le_bytes());
                                    }
                                } else {
                                    hasher.update(&0u32.to_le_bytes());
                                }
                            }
                        }
                    }
                }
                x = next_x;
            }
        }
    }

    hasher.digest()
}

/// Computes the sparse authoritative canonical state_hash directly from a `VoxelPointCloud`.
pub fn compute_sparse_state_hash_for_point_cloud(
    cloud: &mtk_voxel::VoxelPointCloud,
    min_pos: IVec3,
    size: IVec3,
    dimension_str: &str,
) -> u64 {
    let mut hasher = Xxh64::new(0);

    // 1. Metadata Header
    hasher.update(&[SPARSE_CANONICAL_VERSION]);
    let dim_bytes = dimension_str.as_bytes();
    hasher.update(&(dim_bytes.len() as u16).to_le_bytes());
    hasher.update(dim_bytes);
    hasher.update(&min_pos.x.to_le_bytes());
    hasher.update(&min_pos.y.to_le_bytes());
    hasher.update(&min_pos.z.to_le_bytes());
    hasher.update(&(size.x as u32).to_le_bytes());
    hasher.update(&(size.y as u32).to_le_bytes());
    hasher.update(&(size.z as u32).to_le_bytes());

    let max_x = min_pos.x + size.x;
    let max_y = min_pos.y + size.y;
    let max_z = min_pos.z + size.z;

    let count = cloud.len();
    if count == 0 {
        return hasher.digest();
    }

    // Filter points inside the bounding box and sort in canonical (Y, Z, X) order
    let mut indices: Vec<usize> = (0..count)
        .filter(|&i| {
            let x = cloud.block_x[i];
            let y = cloud.block_y[i];
            let z = cloud.block_z[i];
            x >= min_pos.x
                && x < max_x
                && y >= min_pos.y
                && y < max_y
                && z >= min_pos.z
                && z < max_z
        })
        .collect();

    indices.sort_by(|&a, &b| {
        cloud.block_y[a]
            .cmp(&cloud.block_y[b])
            .then_with(|| cloud.block_z[a].cmp(&cloud.block_z[b]))
            .then_with(|| cloud.block_x[a].cmp(&cloud.block_x[b]))
    });

    for i in indices {
        let state = cloud.state_at(i).unwrap_or("minecraft:air");
        let norm_state = normalize_canonical_blockstate(state);
        if norm_state == "minecraft:air" {
            continue;
        }

        let bx = cloud.block_x[i];
        let by = cloud.block_y[i];
        let bz = cloud.block_z[i];

        let rel_x = (bx - min_pos.x) as u32;
        let rel_y = (by - min_pos.y) as u32;
        let rel_z = (bz - min_pos.z) as u32;

        hasher.update(&rel_x.to_le_bytes());
        hasher.update(&rel_y.to_le_bytes());
        hasher.update(&rel_z.to_le_bytes());

        let state_bytes = norm_state.as_bytes();
        hasher.update(&(state_bytes.len() as u16).to_le_bytes());
        hasher.update(state_bytes);

        let biome = cloud.biome_at(i).unwrap_or("minecraft:plains").trim();
        let biome_bytes = if biome.is_empty() {
            b"minecraft:plains".as_slice()
        } else {
            biome.as_bytes()
        };
        hasher.update(&(biome_bytes.len() as u16).to_le_bytes());
        hasher.update(biome_bytes);

        let entity_json = if !cloud.entity_palette.is_empty() && i < cloud.entity_indices.len() {
            let ent_idx = cloud.entity_indices[i];
            if ent_idx >= 0 {
                cloud
                    .entity_palette
                    .get(ent_idx as usize)
                    .map(|s| s.as_str())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(json) = entity_json.filter(|s| !s.trim().is_empty()) {
            let json_bytes = json.as_bytes();
            hasher.update(&(json_bytes.len() as u32).to_le_bytes());
            hasher.update(json_bytes);
        } else {
            hasher.update(&0u32.to_le_bytes());
        }
    }

    hasher.digest()
}

/// Legacy dense canonical state_hash across the 3D bounding volume (RFC Section 3.1).
pub fn compute_canonical_state_hash<FBlock, FBiome>(
    min_pos: IVec3,
    size: IVec3,
    dimension_str: &str,
    mut block_provider: FBlock,
    mut quart_biome_provider: FBiome,
) -> u64
where
    FBlock: FnMut(i32, i32, i32) -> String,
    FBiome: FnMut(i32, i32, i32) -> String,
{
    let mut hasher = Xxh64::new(0);

    // 1. Metadata Header
    hasher.update(&[0x01]); // Canonical Encoding Version: u8
    let dim_bytes = dimension_str.as_bytes();
    hasher.update(&(dim_bytes.len() as u16).to_le_bytes());
    hasher.update(dim_bytes);
    hasher.update(&min_pos.x.to_le_bytes());
    hasher.update(&min_pos.y.to_le_bytes());
    hasher.update(&min_pos.z.to_le_bytes());
    hasher.update(&(size.x as u32).to_le_bytes());
    hasher.update(&(size.y as u32).to_le_bytes());
    hasher.update(&(size.z as u32).to_le_bytes());

    let max_x = min_pos.x + size.x;
    let max_y = min_pos.y + size.y;
    let max_z = min_pos.z + size.z;

    // 2. Continuous 3D Voxel Stream (X-fastest, Z-next, Y-slowest)
    for y in min_pos.y..max_y {
        for z in min_pos.z..max_z {
            for x in min_pos.x..max_x {
                let state_str = block_provider(x, y, z);
                let normalized = normalize_canonical_blockstate(&state_str);
                let bytes = normalized.as_bytes();
                hasher.update(&(bytes.len() as u16).to_le_bytes());
                hasher.update(bytes);
            }
        }
    }

    // 3. Continuous 3D Quart Biome Stream (4x4x4 downsampled grid)
    let qw = (size.x + 3) / 4;
    let qh = (size.y + 3) / 4;
    let qd = (size.z + 3) / 4;

    for qy in 0..qh {
        for qz in 0..qd {
            for qx in 0..qw {
                let biome_str = quart_biome_provider(qx, qy, qz);
                let trimmed = biome_str.trim();
                let bytes = if trimmed.is_empty() {
                    b"minecraft:plains".as_slice()
                } else {
                    trimmed.as_bytes()
                };
                hasher.update(&(bytes.len() as u16).to_le_bytes());
                hasher.update(bytes);
            }
        }
    }

    hasher.digest()
}

#[cfg(test)]
#[path = "hasher_tests.rs"]
mod tests;
