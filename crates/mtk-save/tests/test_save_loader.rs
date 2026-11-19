use glam::IVec3;
use mtk_save::SaveLoader;
use mtk_testkit::save_world;
use mtk_voxel::VoxelStorage;

#[test]
fn test_real_world_level_dat_if_present() {
    let Some(test_save_path) = save_world() else {
        mtk_testkit::skip("no Minecraft save world available (set MTK_TEST_SAVE)");
        return;
    };

    let level_data =
        SaveLoader::read_level_data(&test_save_path).expect("Failed reading level.dat");
    assert!(!level_data.level_name.is_empty());
    assert!(level_data.data_version >= 2844);
    println!("Loaded real level.dat: {:?}", level_data);
}

#[test]
fn test_real_world_bounded_box_loading() {
    let Some(test_save_path) = save_world() else {
        mtk_testkit::skip("no Minecraft save world available (set MTK_TEST_SAVE)");
        return;
    };

    let mut storage = VoxelStorage::new();
    let min_coord = IVec3::new(0, -64, 0);
    let max_coord = IVec3::new(15, 64, 15);

    let loaded = SaveLoader::load_box_into_storage(
        &test_save_path,
        "overworld",
        min_coord,
        max_coord,
        &mut storage,
    )
    .expect("Failed loading bounded world box");

    assert!(
        loaded > 0,
        "Should have loaded at least one non-empty chunk section"
    );
    let (bx, by, bz, sx, sy, sz) = storage.get_bounds();
    assert_eq!(bx, 0);
    assert_eq!(by, -64);
    assert_eq!(bz, 0);
    assert_eq!(sx, 16);
    assert_eq!(sy, 129);
    assert_eq!(sz, 16);
    println!(
        "Loaded {} sections. Active bounds: ({}, {}, {}, size: {}x{}x{})",
        loaded, bx, by, bz, sx, sy, sz
    );

    assert!(
        storage.primary_biome.is_some(),
        "primary_biome must be populated from chunk NBT biomes"
    );
    let pb = storage.primary_biome.as_ref().unwrap();
    println!("Loaded real primary biome: {}", pb);
    assert!(
        !storage.biome_column_map.is_empty(),
        "biome_column_map must contain column biomes"
    );
    println!("Biome columns count: {}", storage.biome_column_map.len());
    let sampled = storage.get_biome(0, 64, 0);
    println!("Sampled biome at (0, 64, 0): {}", sampled);
}

#[test]
fn test_legacy_dimension_folders_are_ignored() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("mtk_save_dims_{}_{}", std::process::id(), unique));
    std::fs::create_dir_all(root.join("DIM-1/region")).unwrap();
    std::fs::create_dir_all(root.join("DIM1/region")).unwrap();

    let dims = SaveLoader::discover_dimensions(&root);
    assert!(!dims.iter().any(|d| d == "the_nether" || d == "the_end"));

    std::fs::create_dir_all(root.join("dimensions/minecraft/the_nether/region")).unwrap();
    std::fs::create_dir_all(root.join("dimensions/minecraft/the_end/region")).unwrap();
    let dims = SaveLoader::discover_dimensions(&root);
    assert!(dims.iter().any(|d| d == "the_nether"));
    assert!(dims.iter().any(|d| d == "the_end"));

    std::fs::remove_dir_all(&root).unwrap();
}

/// Regions are decoded in parallel, so the same selection must produce byte-identical storage
/// regardless of how many worker threads rayon happens to use.
#[test]
fn parallel_region_decoding_is_scheduling_independent() {
    let Some(test_save_path) = save_world() else {
        mtk_testkit::skip("no Minecraft save world available (set MTK_TEST_SAVE)");
        return;
    };

    let load = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("thread pool");
        pool.install(|| {
            let mut storage = VoxelStorage::new();
            let loaded = SaveLoader::load_box_into_storage(
                &test_save_path,
                "overworld",
                IVec3::new(-32, -64, -32),
                IVec3::new(31, 127, 31),
                &mut storage,
            )
            .expect("save import must succeed");
            (loaded, storage)
        })
    };

    let (loaded_single, single) = load(1);
    let (loaded_multi, multi) = load(8);

    assert_eq!(loaded_single, loaded_multi, "loaded section count differs");
    assert_eq!(single.sections.len(), multi.sections.len());
    assert_eq!(single.get_bounds(), multi.get_bounds());
    assert_eq!(single.primary_biome, multi.primary_biome);
    assert_eq!(
        single.biome_column_map, multi.biome_column_map,
        "derived biome columns depend on thread scheduling"
    );

    for x in (-32..32).step_by(7) {
        for z in (-32..32).step_by(7) {
            for y in (-64..128).step_by(11) {
                assert_eq!(
                    single.get_block(x, y, z),
                    multi.get_block(x, y, z),
                    "block mismatch at {x},{y},{z}"
                );
            }
        }
    }
}
