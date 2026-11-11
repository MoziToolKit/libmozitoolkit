use glam::IVec3;
use mtk_save::SaveLoader;
use mtk_testkit::save_world;
use mtk_voxel::VoxelStorage;

#[test]
fn test_real_world_level_dat_if_present() {
    let Some(test_save_path) = save_world() else {
        eprintln!("Skipping: no Minecraft save available (set MTK_TEST_SAVE)");
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
        eprintln!("Skipping: no Minecraft save available (set MTK_TEST_SAVE)");
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

#[test]
fn test_save_loader_detects_paintings_in_entities_mca() {
    let save_dir = match mtk_testkit::save_world() {
        Some(c) => c,
        None => {
            eprintln!("Skipping test: no test save found via mtk_testkit::save_world()");
            return;
        }
    };

    let mut storage = VoxelStorage::new();
    let min_coord = IVec3::new(-17, -61, -17);
    let max_coord = IVec3::new(19, -48, 17);

    let (loaded, entities) = SaveLoader::load_box_with_entities(
        &save_dir,
        "overworld",
        min_coord,
        max_coord,
        &mut storage,
        None,
    )
    .expect("Failed loading user save box");

    assert!(loaded > 0, "Sections should have been loaded");
    assert!(
        entities.len() >= 4,
        "Should have extracted at least 4 visual entities"
    );
    assert_eq!(
        storage.entities.len(),
        entities.len(),
        "storage.entities must be populated by load_box_with_entities"
    );

    // Verify load_box_into_storage also populates storage.entities without dropping
    let mut storage2 = VoxelStorage::new();
    let loaded2 = SaveLoader::load_box_into_storage(
        &save_dir,
        "overworld",
        min_coord,
        max_coord,
        &mut storage2,
    )
    .expect("Failed loading user save box via load_box_into_storage");
    assert_eq!(loaded2, loaded);
    assert_eq!(
        storage2.entities.len(),
        entities.len(),
        "storage2.entities must be populated by load_box_into_storage"
    );

    // 1. Unpacked painting (4x4)
    let unpacked = entities
        .iter()
        .find(|e| {
            if let mtk_model::EntityKindData::Painting(p) = &e.kind {
                p.variant == "unpacked"
            } else {
                false
            }
        })
        .expect("Unpacked painting entity should exist");
    assert_eq!(unpacked.facing, Some(mtk_core::Direction::East));
    assert_eq!(
        unpacked.position,
        glam::DVec3::new(-7.96875, -58.0, 10.0),
        "Painting position must be the calculated world-space center, not the support block"
    );
    if let mtk_model::EntityKindData::Painting(p) = &unpacked.kind {
        assert_eq!(p.width, 4);
        assert_eq!(p.height, 4);
    }

    // 2. Prairie ride painting (1x2)
    assert!(entities.iter().any(|e| {
        if let mtk_model::EntityKindData::Painting(p) = &e.kind {
            p.variant == "prairie_ride" && p.width == 1 && p.height == 2
        } else {
            false
        }
    }));

    // 3. Pool painting (2x1)
    assert!(entities.iter().any(|e| {
        if let mtk_model::EntityKindData::Painting(p) = &e.kind {
            p.variant == "pool" && p.width == 2 && p.height == 1
        } else {
            false
        }
    }));

    // 4. Sunset painting (2x1)
    assert!(entities.iter().any(|e| {
        if let mtk_model::EntityKindData::Painting(p) = &e.kind {
            p.variant == "sunset" && p.width == 2 && p.height == 1
        } else {
            false
        }
    }));

    println!("All 4 paintings successfully detected and loaded from user's save!");
}
