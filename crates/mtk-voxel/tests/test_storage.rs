use glam::IVec3;
use mtk_voxel::{SectionStorage, VoxelStorage};

#[test]
fn test_section_palette_mutation() {
    let mut sec = SectionStorage::new(IVec3::new(0, 0, 0));
    assert!(sec.is_empty());
    assert_eq!(sec.palette.len(), 1);
    assert_eq!(sec.palette[0], "minecraft:air");

    sec.set_local(0, 0, 0, "minecraft:stone");
    sec.set_local(1, 0, 0, "minecraft:oak_planks");
    sec.set_local(2, 0, 0, "minecraft:stone"); // Reuse palette index

    assert_eq!(sec.palette.len(), 3);
    assert_eq!(sec.non_air_count, 3);
    assert_eq!(sec.get_local_state(0, 0, 0), "minecraft:stone");
    assert_eq!(sec.get_local_state(1, 0, 0), "minecraft:oak_planks");
    assert_eq!(sec.get_local_state(2, 0, 0), "minecraft:stone");

    // Clearing back to air
    sec.set_local(0, 0, 0, "minecraft:air");
    sec.set_local(1, 0, 0, "minecraft:air");
    sec.set_local(2, 0, 0, "minecraft:air");
    assert!(sec.is_empty());
    assert_eq!(sec.non_air_count, 0);

    sec.clear();
    assert_eq!(sec, SectionStorage::new(IVec3::ZERO));
}

#[test]
fn test_world_bounds_and_pruning() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 32, 32, 32);

    world.set_block(5, 5, 5, "minecraft:diamond_block", None);
    world.set_block(25, 25, 25, "minecraft:gold_block", None);

    assert_eq!(world.get_block(5, 5, 5), "minecraft:diamond_block");
    assert_eq!(world.get_block(25, 25, 25), "minecraft:gold_block");

    // Shrink bounds to [0..16, 0..16, 0..16]
    world.set_bounds(0, 0, 0, 16, 16, 16);

    assert_eq!(world.get_block(5, 5, 5), "minecraft:diamond_block");
    // Out-of-bounds block should be pruned
    assert_eq!(world.get_block(25, 25, 25), "minecraft:air");
}

#[test]
fn test_section_snapshot_ingestion() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    let palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let mut grid = vec![0u16; 4096];
    grid[0] = 1; // (0,0,0) is stone

    world.set_section_snapshot(0, 0, 0, 0, 0, 0, 16, 16, 16, &palette, &grid, None, None);
    assert_eq!(world.get_block(0, 0, 0), "minecraft:stone");
    assert!(world.dirty_sections.contains(&IVec3::new(0, 0, 0)));
}

#[test]
fn test_manifest_metadata_export_import() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 32, 16, 16);
    world.set_block(1, 2, 3, "minecraft:iron_block", None);

    let json_meta = world.export_manifest_metadata();
    assert_eq!(json_meta["size_x"], 32);

    let mut restored = VoxelStorage::new();
    assert!(restored.import_manifest_metadata(&json_meta));
    assert_eq!(restored.min_x, 0);
    assert_eq!(restored.size_x, 32);
}

#[test]
fn test_section_snapshot_dynamic_bounds_expansion() {
    let mut world = VoxelStorage::new();
    // Initially bounds are 0
    assert_eq!(world.get_bounds(), (0, 0, 0, 0, 0, 0));

    let palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let mut grid = vec![0u16; 4096];
    grid[0] = 1;

    // Snapshot 1 at (0, 0, 0)
    world.set_section_snapshot(0, 0, 0, 0, 0, 0, 16, 16, 16, &palette, &grid, None, None);
    assert_eq!(world.get_bounds(), (0, 0, 0, 16, 16, 16));

    // Snapshot 2 at (1, 0, 0)
    world.set_section_snapshot(1, 0, 0, 16, 0, 0, 16, 16, 16, &palette, &grid, None, None);
    // Bounds must automatically expand to cover both sections [0..32, 0..16, 0..16]
    assert_eq!(world.get_bounds(), (0, 0, 0, 32, 16, 16));
    assert!(world.contains(0, 0, 0));
    assert!(world.contains(20, 5, 5));
}

#[test]
fn test_section_snapshot_3d_biome_ingestion_and_meshing() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    let palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let mut grid = vec![0u16; 4096];
    grid[0] = 1;

    let biome_palette = vec![
        "minecraft:taiga".to_string(),
        "minecraft:lush_caves".to_string(),
    ];
    let mut biome_indices = vec![0u16; 4096];
    for idx in 0..4096 {
        let rem = idx % (16 * 16);
        let ly = rem / 16;
        if ly < 8 {
            biome_indices[idx] = 1; // lush_caves for lower half
        } else {
            biome_indices[idx] = 0; // taiga for upper half
        }
    }

    world.set_section_snapshot(
        0,
        0,
        0,
        0,
        0,
        0,
        16,
        16,
        16,
        &palette,
        &grid,
        Some(&biome_palette),
        Some(&biome_indices),
    );

    let sec = world
        .sections
        .get(&IVec3::new(0, 0, 0))
        .expect("section must exist");
    assert!(
        sec.biomes.is_some(),
        "sec.biomes must be populated from snapshot"
    );
    let sb = sec.biomes.as_ref().unwrap();
    assert!(
        sb.varies_vertically(),
        "3D biomes must vary vertically across layers"
    );

    assert_eq!(world.get_biome(0, 2, 0), "minecraft:lush_caves");
    assert_eq!(world.get_biome(0, 14, 0), "minecraft:taiga");

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    assert!(padded.biome_data.is_some());
    let cols = padded.biome_data.as_ref().unwrap();
    // 3D Quart mode must produce 1024 smoothed columns (4 vertical layers x 256)
    assert_eq!(
        cols.len(),
        1024,
        "PaddedArray with 3D biomes must have 1024 smoothed column entries"
    );
}

#[test]
fn test_finalize_biome_metadata_derives_column_map_and_primary() {
    use mtk_voxel::SectionBiomes;

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 32, 16, 16);

    // Two sections side by side. Left column set is horizontally uniform (`desert`),
    // right one is vertically varying (`taiga` surface above `lush_caves`).
    for (sx, make) in [(0i32, 0usize), (1i32, 1usize)] {
        let coord = IVec3::new(sx, 0, 0);
        let mut sec = SectionStorage::new(coord);
        sec.set_local(0, 0, 0, "minecraft:stone");

        let mut biomes = SectionBiomes::uniform("minecraft:desert");
        if make == 1 {
            for qx in 0..4 {
                for qz in 0..4 {
                    biomes.set_sampled_quart_biome(qx, 3, qz, "minecraft:taiga");
                    biomes.set_sampled_quart_biome(qx, 0, qz, "minecraft:lush_caves");
                    biomes.set_sampled_quart_biome(qx, 1, qz, "minecraft:lush_caves");
                    biomes.set_sampled_quart_biome(qx, 2, qz, "minecraft:lush_caves");
                }
            }
        }
        sec.biomes = Some(biomes);
        world.sections.insert(coord, sec);
    }

    world.finalize_biome_metadata();

    // One entry per block column inside the active bounds.
    assert_eq!(world.biome_column_map.len(), 32 * 16);
    assert_eq!(
        world.get_biome(3, 8, 3),
        "minecraft:desert",
        "left section surface biome"
    );
    assert_eq!(
        world.biome_column_map.get(&[20, 3]).map(String::as_str),
        Some("minecraft:taiga"),
        "column map must use the top quart layer of the highest section"
    );
    assert_eq!(
        world.get_biome(20, 12, 3),
        "minecraft:taiga",
        "right section surface quart biome"
    );
    assert_eq!(
        world.get_biome(20, 2, 3),
        "minecraft:lush_caves",
        "right section underground quart biome"
    );
    // Dominant primary is the surface biome of the majority of columns.
    assert_eq!(world.primary_biome.as_deref(), Some("minecraft:desert"));
}
