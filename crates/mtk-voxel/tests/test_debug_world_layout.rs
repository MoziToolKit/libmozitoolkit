//! The debug world layout must match vanilla `DebugLevelSource`, and generation must be a pure
//! function of the inputs.
//!
//! The previous golden-hash test only ever skipped: it called the environment-driven
//! `VoxelStorage::create_debug_world()`, which produced nothing unless `MC_ASSETS_DIR` happened to
//! point at an unpacked client. Asserting the *layout* rules is both version-independent and
//! actually executed.

use mtk_voxel::{VoxelStorage, DEBUG_WORLD_Y};

fn synthetic_states(count: usize) -> Vec<String> {
    let mut states = Vec::with_capacity(count);
    for index in 0..count {
        states.push(format!("minecraft:test_block_{index:03}"));
    }
    states
}

/// Vanilla `DebugLevelSource`: `GRID_WIDTH = ceil(sqrt(N))`, blocks at `(2*i+1, 70, 2*j+1)` with
/// `i = index / GRID_WIDTH` and `j = index % GRID_WIDTH`.
#[test]
fn the_layout_matches_vanilla_debug_level_source() {
    let states = synthetic_states(40);
    let storage = VoxelStorage::create_debug_world_from_states(&states).expect("debug world");

    let grid_width = (40f32).sqrt().ceil() as i32; // 7
    let grid_height = ((40f32) / (grid_width as f32)).ceil() as i32; // 6

    for (index, state) in states.iter().enumerate() {
        let i = index as i32 / grid_width;
        let j = index as i32 % grid_width;
        assert_eq!(
            storage.get_block(2 * i + 1, DEBUG_WORLD_Y, 2 * j + 1),
            state,
            "index {index} must sit at (2*{i}+1, {DEBUG_WORLD_Y}, 2*{j}+1)"
        );
        // One-block spacing: the neighbouring grid cells stay empty.
        assert_eq!(
            storage.get_block(2 * i + 2, DEBUG_WORLD_Y, 2 * j + 1),
            "minecraft:air"
        );
        assert_eq!(
            storage.get_block(2 * i + 1, DEBUG_WORLD_Y, 2 * j + 2),
            "minecraft:air"
        );
    }

    let (min_x, min_y, min_z, size_x, size_y, size_z) = storage.get_bounds();
    assert_eq!((min_x, min_y, min_z), (0, DEBUG_WORLD_Y - 1, 0));
    assert_eq!(size_y, 3);
    assert_eq!(size_x, 2 * grid_height + 2);
    assert_eq!(size_z, 2 * grid_width + 2);
}

#[test]
fn air_states_are_skipped_but_keep_their_grid_slot() {
    let states = vec![
        "minecraft:air".to_string(),
        "minecraft:stone".to_string(),
        "minecraft:cave_air".to_string(),
        "minecraft:granite".to_string(),
    ];
    let storage = VoxelStorage::create_debug_world_from_states(&states).expect("debug world");
    let grid_width = (4f32).sqrt().ceil() as i32; // 2

    assert_eq!(
        storage.get_block(1, DEBUG_WORLD_Y, 1),
        "minecraft:air",
        "index 0 is air and must leave its slot empty"
    );
    assert_eq!(
        storage.get_block(1, DEBUG_WORLD_Y, 3),
        "minecraft:stone",
        "index 1 keeps the slot after the skipped one"
    );
    assert_eq!(
        storage.get_block(3, DEBUG_WORLD_Y, 1),
        "minecraft:air",
        "index 2 is cave_air"
    );
    assert_eq!(storage.get_block(3, DEBUG_WORLD_Y, 3), "minecraft:granite");
    let _ = grid_width;
}

#[test]
fn generation_is_deterministic() {
    let states = synthetic_states(24);
    let first = VoxelStorage::create_debug_world_from_states(&states).expect("debug world");
    let second = VoxelStorage::create_debug_world_from_states(&states).expect("debug world");
    assert_eq!(first.get_bounds(), second.get_bounds());
    assert_eq!(
        first.get_all_non_empty_sections(),
        second.get_all_non_empty_sections()
    );
}

/// The real client blockstate set produces the same rules at scale (no hash: the list changes
/// between game versions, the layout does not).
#[test]
fn the_real_blockstate_set_follows_the_same_layout() {
    let Some(root) = mtk_testkit::real_assets_root() else {
        mtk_testkit::skip("no unpacked jar (set MTK_TEST_ASSETS or place ../Minecraft_Source)");
        return;
    };
    let storage =
        VoxelStorage::create_debug_world_from_dir(&root).expect("debug world from real assets");

    let (min_x, min_y, min_z, size_x, size_y, size_z) = storage.get_bounds();
    assert_eq!((min_x, min_y, min_z), (0, DEBUG_WORLD_Y - 1, 0));
    assert_eq!(size_y, 3);
    assert!(
        size_x > 100 && size_z > 100,
        "got {size_x}x{size_y}x{size_z}"
    );
    assert!(
        storage.get_all_non_empty_sections().len() > 50,
        "the real blockstate set must span many sections"
    );

    // Every block sits on the odd-coordinate grid at y = 70, and the plane is well filled (air
    // states in the blockstate list legitimately leave their slot empty).
    let mut populated = 0usize;
    for x in 0..size_x {
        for z in 0..size_z {
            let block = storage.get_block(x, DEBUG_WORLD_Y, z);
            if block == "minecraft:air" {
                continue;
            }
            populated += 1;
            assert!(
                x % 2 == 1 && z % 2 == 1,
                "block {block} at ({x}, {z}) is off the odd-coordinate grid"
            );
        }
    }
    // Only the odd/odd grid cells can hold a block; almost all of them should.
    let grid_cells = ((size_x / 2) * (size_z / 2)) as usize;
    assert!(
        populated * 10 >= grid_cells * 9,
        "the debug world grid should be nearly full: {populated}/{grid_cells}"
    );
    for x in 0..size_x {
        for z in 0..size_z {
            for y in [DEBUG_WORLD_Y - 1, DEBUG_WORLD_Y + 1] {
                assert_eq!(
                    storage.get_block(x, y, z),
                    "minecraft:air",
                    "the debug world is a single plane at y = {DEBUG_WORLD_Y}"
                );
            }
        }
    }
}
