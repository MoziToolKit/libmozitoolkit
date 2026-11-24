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
fn test_dimension_discovery_supports_dim_and_modern_layouts() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("mtk_save_dims_{}_{}", std::process::id(), unique));
    std::fs::create_dir_all(root.join("DIM-1/region")).unwrap();
    std::fs::create_dir_all(root.join("DIM1/region")).unwrap();

    let dims = SaveLoader::discover_dimensions(&root);
    assert!(dims.iter().any(|d| d == "the_nether"));
    assert!(dims.iter().any(|d| d == "the_end"));

    std::fs::create_dir_all(root.join("dimensions/minecraft/the_nether/region")).unwrap();
    std::fs::create_dir_all(root.join("dimensions/minecraft/the_end/region")).unwrap();
    let dims = SaveLoader::discover_dimensions(&root);
    assert_eq!(dims.iter().filter(|d| *d == "the_nether").count(), 1);
    assert_eq!(dims.iter().filter(|d| *d == "the_end").count(), 1);

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

/// Regions are decoded on rayon workers, but a progress callback is host code: Python re-enters
/// its interpreter (GIL) and Blender re-enters the main thread from it. Invoking it on a pool
/// thread deadlocked the real host ("import save freezes"). The loader must therefore report only
/// from the thread the caller is on.
#[test]
fn progress_callback_runs_on_the_calling_thread() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let Some(test_save_path) = save_world() else {
        mtk_testkit::skip("no Minecraft save world available (set MTK_TEST_SAVE)");
        return;
    };

    let caller_thread = std::thread::current().id();
    let events = AtomicUsize::new(0);
    let off_thread = AtomicUsize::new(0);
    let callback = |_report: mtk_core::ProgressReport| {
        events.fetch_add(1, Ordering::Relaxed);
        if std::thread::current().id() != caller_thread {
            off_thread.fetch_add(1, Ordering::Relaxed);
        }
    };
    let progress: mtk_core::ProgressCallback<'_> = &callback;

    // A selection this large is what makes rayon actually hand chunks to pool threads; a tiny
    // box can run entirely on the calling thread and hide the regression.
    let mut storage = VoxelStorage::new();
    let loaded = SaveLoader::load_box_into_storage_with_progress(
        &test_save_path,
        "overworld",
        IVec3::new(-64, -64, -64),
        IVec3::new(511, 319, 511),
        &mut storage,
        Some(progress),
    )
    .expect("save import must succeed");

    assert!(loaded > 0, "the test save must yield sections");
    assert!(
        events.load(Ordering::Relaxed) > 0,
        "the loader must report progress at least once"
    );
    assert_eq!(
        off_thread.load(Ordering::Relaxed),
        0,
        "progress callback ran on a rayon worker; hosts re-enter the GIL / main thread there"
    );
}
