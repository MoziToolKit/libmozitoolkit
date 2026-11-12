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
///
/// Note: Visual entities (paintings, item frames, armor stands) belong to scene packages (.mtkscene)
/// and DCC representations, not discrete block voxel state synchronized via RFC v3.1 block delta streams.
/// Therefore they do not affect `compute_sparse_state_hash`.
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

/// Computes the authoritative canonical sparse state_hash directly from raw memory arrays.
/// Guarantees bit-for-bit equivalence with `compute_sparse_state_hash_for_storage`.
pub fn compute_sparse_state_hash_from_raw(
    min_pos: IVec3,
    size: IVec3,
    dimension_str: &str,
    raw_states: &[u32],
    state_palette: &[String],
    raw_biomes: &[u32],
    biome_palette: &[String],
    entity_map: &std::collections::HashMap<IVec3, &str>,
) -> u64 {
    let quart_grid = super::quart::QuartGrid::from_bounds(min_pos, size);
    let total_voxels = (size.x as usize) * (size.y as usize) * (size.z as usize);

    let non_air_iter =
        raw_states
            .iter()
            .take(total_voxels)
            .enumerate()
            .filter_map(|(idx, &raw_id)| {
                let state_str = if (raw_id as usize) < state_palette.len() {
                    &state_palette[raw_id as usize]
                } else {
                    "minecraft:air"
                };

                let norm_state = normalize_canonical_blockstate(state_str);
                if norm_state == "minecraft:air" {
                    return None;
                }

                let (lx, ly, lz) = super::quart::voxel_index_to_relative_coords(idx, size);

                let q_idx = quart_grid.relative_voxel_to_quart_index(lx, ly, lz);
                let raw_b_id = raw_biomes.get(q_idx).copied().unwrap_or(0);
                let biome = if (raw_b_id as usize) < biome_palette.len() {
                    &biome_palette[raw_b_id as usize]
                } else {
                    "minecraft:plains"
                };

                let entity_json = entity_map
                    .get(&IVec3::new(lx as i32, ly as i32, lz as i32))
                    .copied();

                Some(SparseBlockEntry {
                    rel_x: lx,
                    rel_y: ly,
                    rel_z: lz,
                    state: state_str,
                    biome,
                    entity_json,
                })
            });

    compute_sparse_state_hash(min_pos, size, dimension_str, non_air_iter)
}

/// Helper to extract u64 high and u64 low from a standard UUID string.
pub fn parse_uuid_bits(uuid_str: &str) -> (u64, u64) {
    let clean: String = uuid_str.chars().filter(|c| *c != '-').collect();
    if clean.len() == 32 {
        let high = u64::from_str_radix(&clean[0..16], 16).unwrap_or(0);
        let low = u64::from_str_radix(&clean[16..32], 16).unwrap_or(0);
        (high, low)
    } else {
        (0, 0)
    }
}

/// Helper to format most_sig and least_sig u64 into canonical 8-4-4-4-12 UUID string.
pub fn format_uuid(most: u64, least: u64) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (most >> 32) as u32,
        (most >> 16) as u16,
        most as u16,
        (least >> 48) as u16,
        least & 0x0000_ffff_ffff_ffff
    )
}

/// Maps optional Direction to canonical RFC v3.2 facing byte:
/// 0:Down, 1:Up, 2:North, 3:South, 4:West, 5:East, 255:None.
pub fn direction_to_facing_byte(dir: Option<mtk_core::Direction>) -> u8 {
    match dir {
        Some(mtk_core::Direction::Down) => 0,
        Some(mtk_core::Direction::Up) => 1,
        Some(mtk_core::Direction::North) => 2,
        Some(mtk_core::Direction::South) => 3,
        Some(mtk_core::Direction::West) => 4,
        Some(mtk_core::Direction::East) => 5,
        None => 255,
    }
}

/// Maps RFC v3.2 facing byte back to Direction.
pub fn facing_byte_to_direction(facing: u8) -> Option<mtk_core::Direction> {
    match facing {
        0 => Some(mtk_core::Direction::Down),
        1 => Some(mtk_core::Direction::Up),
        2 => Some(mtk_core::Direction::North),
        3 => Some(mtk_core::Direction::South),
        4 => Some(mtk_core::Direction::West),
        5 => Some(mtk_core::Direction::East),
        _ => None,
    }
}

/// Computes the authoritative deterministic xxHash64 over a list of visual entities (RFC v3.2).
/// Bit-for-bit aligned with Java `VisualEntityEntry.computeEntitiesHash`.
pub fn compute_visual_entities_hash(entities: &[mtk_model::EntityInstance]) -> u64 {
    if entities.is_empty() {
        return 0;
    }

    let mut sorted: Vec<&mtk_model::EntityInstance> = entities.iter().collect();
    sorted.sort_by(|a, b| {
        let u_a = a.uuid.as_deref().unwrap_or("");
        let u_b = b.uuid.as_deref().unwrap_or("");
        u_a.cmp(u_b)
    });

    let mut hasher = Xxh64::new(0);
    for e in sorted {
        let uuid_str = e.uuid.as_deref().unwrap_or("");
        let (most_sig, least_sig) = parse_uuid_bits(uuid_str);
        hasher.update(&most_sig.to_le_bytes());
        hasher.update(&least_sig.to_le_bytes());

        let id_bytes = e.id.as_bytes();
        hasher.update(&(id_bytes.len() as u16).to_le_bytes());
        hasher.update(id_bytes);

        hasher.update(&e.position.x.to_bits().to_le_bytes());
        hasher.update(&e.position.y.to_bits().to_le_bytes());
        hasher.update(&e.position.z.to_bits().to_le_bytes());

        hasher.update(&e.rotation.x.to_bits().to_le_bytes());
        hasher.update(&e.rotation.y.to_bits().to_le_bytes());
        hasher.update(&e.rotation.z.to_bits().to_le_bytes());

        hasher.update(&e.scale.x.to_bits().to_le_bytes());
        hasher.update(&e.scale.y.to_bits().to_le_bytes());
        hasher.update(&e.scale.z.to_bits().to_le_bytes());

        let facing = direction_to_facing_byte(e.facing);
        hasher.update(&[facing]);

        let payload = match &e.kind {
            mtk_model::EntityKindData::Generic { payload, .. } if !payload.is_empty() => {
                payload.clone()
            }
            _ => serde_json::to_vec(&e.kind).unwrap_or_default(),
        };
        hasher.update(&(payload.len() as u32).to_le_bytes());
        if !payload.is_empty() {
            hasher.update(&payload);
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

#[cfg(test)]
#[path = "hasher_tests.rs"]
mod tests;
