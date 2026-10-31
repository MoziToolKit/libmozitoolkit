//! # Canonical State Hash Calculator (RFC v3.1 Section 3)
//!
//! Provides the Single Source of Truth (SSOT) for world state verification using xxHash64.

use glam::IVec3;
use xxhash_rust::xxh64::Xxh64;

pub use mtk_model::normalize_canonical_blockstate;

/// Computes the authoritative canonical state_hash across the 3D bounding volume.
///
/// Coordinate traversal strictly adheres to RFC Section 3.1:
/// - X is fastest-varying axis
/// - Z is next-fastest axis
/// - Y is slowest-varying axis (bottom-up scanning of horizontal slices)
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
mod tests {
    use super::*;

    #[test]
    fn test_canonical_hash_deterministic() {
        let min_pos = IVec3::new(0, 64, 0);
        let size = IVec3::new(2, 2, 2);

        let h1 = compute_canonical_state_hash(
            min_pos,
            size,
            "minecraft:overworld",
            |_, _, _| "minecraft:stone".to_string(),
            |_, _, _| "minecraft:plains".to_string(),
        );

        let h2 = compute_canonical_state_hash(
            min_pos,
            size,
            "minecraft:overworld",
            |_, _, _| "minecraft:stone".to_string(),
            |_, _, _| "minecraft:plains".to_string(),
        );

        assert_eq!(h1, h2);
        assert_ne!(h1, 0);
    }

    #[test]
    fn test_air_family_normalization_matches_java_ssot() {
        // Java normalizes by block type (`state.is(...)`), so property variants of the
        // air family must collapse to air as well. Otherwise the client hash diverges
        // from the server whenever a selection contains a bubble column.
        for raw in [
            "minecraft:air",
            "minecraft:cave_air",
            "minecraft:void_air",
            "minecraft:structure_void",
            "minecraft:bubble_column",
            "minecraft:bubble_column[drag=true]",
            "minecraft:bubble_column[drag=false]",
            "bubble_column[drag=true]",
        ] {
            assert_eq!(
                normalize_canonical_blockstate(raw),
                "minecraft:air",
                "raw state {raw} must canonicalise to air"
            );
        }
        // Non-air states keep their full property string.
        assert_eq!(
            normalize_canonical_blockstate("minecraft:oak_stairs[facing=north]"),
            "minecraft:oak_stairs[facing=north]"
        );
    }
}
