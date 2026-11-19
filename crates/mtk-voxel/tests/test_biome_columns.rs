//! Equivalence tests for the pre-sampled biome column path.
//!
//! `VoxelStorage::get_section_padded_array` averages a pre-sampled 20x20 neighborhood grid
//! instead of running the 5x5 kernel through `get_biome` 25 times per column. These tests pin the
//! produced columns against the reference resolver at the same sample height, so the optimization
//! can never drift from the documented smoothing.

use glam::IVec3;
use mtk_voxel::{get_smoothed_column_biome, SmoothedBiomeColumn, VoxelStorage};

/// Mirrors the padding/clamping rule: samples never escape the explicit selection.
fn reference_column(storage: &VoxelStorage, wx: i32, y: i32, wz: i32) -> SmoothedBiomeColumn {
    let (min_x, _, min_z, size_x, _, size_z) = storage.get_bounds();
    let has_bounds = size_x > 0 && size_z > 0;
    let clamp_x = |bx: i32| {
        if has_bounds {
            bx.clamp(min_x, min_x + size_x - 1)
        } else {
            bx
        }
    };
    let clamp_z = |bz: i32| {
        if has_bounds {
            bz.clamp(min_z, min_z + size_z - 1)
        } else {
            bz
        }
    };
    get_smoothed_column_biome(
        |bx, y, bz| storage.get_biome(clamp_x(bx), y, clamp_z(bz)),
        wx,
        y,
        wz,
    )
}

fn section_columns(padded: &mtk_voxel::PaddedVoxelArray) -> Vec<SmoothedBiomeColumn> {
    padded
        .biome_data
        .as_ref()
        .expect("section must carry biome data")
        .clone()
}

#[test]
fn horizontal_biomes_match_the_reference_kernel() {
    let mut storage = VoxelStorage::new();
    storage.set_bounds(0, 0, 0, 32, 16, 32);

    // One flat stone layer at y = 5 gives every column the same (topmost, non-air) sample height.
    for x in 0..32 {
        for z in 0..32 {
            let biome = if (x / 8 + z / 8) % 2 == 0 {
                "minecraft:plains"
            } else if (x / 8 + z / 8) % 4 == 1 {
                "minecraft:desert"
            } else {
                "minecraft:swamp"
            };
            storage.set_block(x, 5, z, "minecraft:stone", Some(biome));
        }
    }

    // Both sections of the bottom layer: the 5x5 kernel reaches across the section border.
    for sector in [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)] {
        let padded = storage.get_section_padded_array(sector);
        let columns = section_columns(&padded);
        assert_eq!(columns.len(), 256, "2D sections keep one column per x/z");

        for lx in 0..16i32 {
            for lz in 0..16i32 {
                let wx = sector.x * 16 + lx;
                let wz = sector.z * 16 + lz;
                let expected = reference_column(&storage, wx, 5, wz);
                assert_eq!(
                    columns[(lx * 16 + lz) as usize],
                    expected,
                    "column mismatch at {wx},{wz} in section {sector:?}"
                );
            }
        }
    }
}

#[test]
fn vertical_biomes_match_the_reference_kernel_per_quart_layer() {
    let mut storage = VoxelStorage::new();
    storage.set_bounds(0, 0, 0, 16, 16, 16);

    // Two biomes in different quart layers make the section vertically varying, which switches it
    // to the 3D path: one column per quart layer, sampled at `sec_wy + qy*4 + 2`.
    for x in 0..16 {
        for z in 0..16 {
            let low = if (x / 4 + z / 4) % 2 == 0 {
                "minecraft:lush_caves"
            } else {
                "minecraft:plains"
            };
            let high = if (x / 4 + z / 4) % 2 == 0 {
                "minecraft:desert"
            } else {
                "minecraft:taiga"
            };
            storage.set_block(x, 1, z, "minecraft:stone", Some(low));
            storage.set_block(x, 14, z, "minecraft:stone", Some(high));
        }
    }

    let padded = storage.get_section_padded_array(IVec3::new(0, 0, 0));
    let columns = section_columns(&padded);
    assert_eq!(columns.len(), 1024, "3D sections keep one column per quart");

    for qy in 0..4i32 {
        let sample_y = qy * 4 + 2;
        for lx in 0..16i32 {
            for lz in 0..16i32 {
                let index = (qy * 256 + lx * 16 + lz) as usize;
                let expected = reference_column(&storage, lx, sample_y, lz);
                assert_eq!(
                    columns[index], expected,
                    "quart layer {qy} mismatch at {lx},{lz}"
                );
            }
        }
    }
}
