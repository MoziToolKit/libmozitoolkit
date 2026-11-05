use glam::IVec3;
use mtk_save::region::RegionFile;
use mtk_save::SaveLoader;
use mtk_testkit::save_world;
use mtk_voxel::VoxelStorage;

#[test]
fn test_region_chunk_indexing() {
    assert_eq!(RegionFile::chunk_index(0, 0), 0);
    assert_eq!(RegionFile::chunk_index(1, 0), 1);
    assert_eq!(RegionFile::chunk_index(0, 1), 32);
    assert_eq!(RegionFile::chunk_index(31, 31), 1023);

    // Negative coordinates wrap using Euclidean remainder
    assert_eq!(RegionFile::chunk_index(-1, -1), 31 + 31 * 32);
    assert_eq!(RegionFile::chunk_index(-32, 0), 0);
}

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
fn test_region_file_reads_gzip_chunk() {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use flate2::write::GzEncoder;
    use flate2::Compression;

    let payload = b"gzip-compressed chunk payload".repeat(16);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&payload).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() + 5 < mtk_save::region::SECTOR_SIZE);

    // 4 KiB location header + one data sector. Chunk (0,0) starts at sector 2.
    let mut bytes = vec![0u8; mtk_save::region::SECTOR_SIZE * 3];
    bytes[0..3].copy_from_slice(&[0, 0, 2]);
    bytes[3] = 1;
    let data_offset = mtk_save::region::SECTOR_SIZE * 2;
    let declared_len = (compressed.len() + 1) as u32;
    bytes[data_offset..data_offset + 4].copy_from_slice(&declared_len.to_be_bytes());
    bytes[data_offset + 4] = 1; // GZip
    bytes[data_offset + 5..data_offset + 5 + compressed.len()].copy_from_slice(&compressed);

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "mtk_save_gzip_{}_{}.mca",
        std::process::id(),
        unique
    ));
    std::fs::write(&path, &bytes).unwrap();

    let mut region = RegionFile::open(&path).unwrap();
    let decoded = region
        .read_chunk_decompressed(0, 0)
        .unwrap()
        .expect("gzip chunk should be present");
    assert_eq!(decoded, payload);
    drop(region);
    std::fs::remove_file(&path).unwrap();
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
    assert!(SaveLoader::resolve_dimension_region_dir(&root, "the_nether").is_none());
    assert!(SaveLoader::resolve_dimension_region_dir(&root, "the_end").is_none());

    std::fs::create_dir_all(root.join("dimensions/minecraft/the_nether/region")).unwrap();
    std::fs::create_dir_all(root.join("dimensions/minecraft/the_end/region")).unwrap();
    let dims = SaveLoader::discover_dimensions(&root);
    assert!(dims.iter().any(|d| d == "the_nether"));
    assert!(dims.iter().any(|d| d == "the_end"));

    std::fs::remove_dir_all(&root).unwrap();
}
