use std::fs;

use mtk_core::Direction;
use mtk_model::{BakedElement, BakedFace, BakedModel, BakedModelDatabase, BakedVariantGroup};

fn create_sample_cube(state: &str, tex: &str) -> BakedModel {
    let mut model = BakedModel {
        block_state: state.to_string(),
        is_cube: true,
        is_opaque: true,
        ..Default::default()
    };
    let mut faces = std::collections::HashMap::new();
    for dir in Direction::ALL {
        let face = BakedFace {
            direction: dir,
            texture: tex.to_string(),
            cullface: Some(dir),
            ..Default::default()
        };
        faces.insert(dir, face);
    }
    model.elements.push(BakedElement {
        from_pos: [0.0, 0.0, 0.0],
        to_pos: [16.0, 16.0, 16.0],
        faces,
    });
    model.rebuild_face_buckets();
    model
}

#[test]
fn test_database_compact_serialization_and_zstd() {
    let mut db = BakedModelDatabase::new();

    // 1. Single variant model
    let stone = create_sample_cube("minecraft:stone", "minecraft:block/stone");
    db.insert("minecraft:stone".to_string(), stone.clone());

    // 2. Multi variant model (e.g. 4 rotations with weights)
    let dirt_0 = create_sample_cube("minecraft:dirt", "minecraft:block/dirt");
    let dirt_1 = create_sample_cube("minecraft:dirt", "minecraft:block/dirt_rot1");
    let dirt_2 = create_sample_cube("minecraft:dirt", "minecraft:block/dirt_rot2");
    let dirt_3 = create_sample_cube("minecraft:dirt", "minecraft:block/dirt_rot3");
    let dirt_group = BakedVariantGroup::new(
        "minecraft:dirt".to_string(),
        vec![dirt_0, dirt_1, dirt_2, dirt_3],
        vec![1, 1, 1, 1],
    );
    db.insert_variant_group("minecraft:dirt".to_string(), dirt_group);

    // 3. Serialize with to_binary (which compresses with zstd using postcard)
    let compressed_bytes = db.to_binary().expect("to_binary should succeed");

    // Verify zstd magic header [0x28, 0xB5, 0x2F, 0xFD]
    assert!(compressed_bytes.len() >= 4);
    assert_eq!(&compressed_bytes[..4], &[0x28, 0xb5, 0x2f, 0xfd]);

    // 4. Deserialize with from_binary
    let loaded =
        BakedModelDatabase::from_binary(&compressed_bytes).expect("from_binary should succeed");

    // Verify models map
    assert_eq!(loaded.models.len(), 2);
    assert!(loaded.models.contains_key("minecraft:stone"));
    assert!(loaded.models.contains_key("minecraft:dirt"));

    // Verify variant_groups map (both single and multi)
    assert_eq!(loaded.variant_groups.len(), 2);

    let stone_group = loaded.get_variant_group("minecraft:stone").unwrap();
    assert_eq!(stone_group.len(), 1);
    assert_eq!(stone_group.select_primary().block_state, "minecraft:stone");

    let dirt_loaded_group = loaded.get_variant_group("minecraft:dirt").unwrap();
    assert_eq!(dirt_loaded_group.len(), 4);
    assert_eq!(dirt_loaded_group.weights, vec![1, 1, 1, 1]);

    // Verify face buckets are intact
    assert!(!loaded.models["minecraft:stone"].culled_faces[Direction::Up.to_index()].is_empty());
}

#[test]
fn test_rejects_uncompressed_pre_v3_payload() {
    let mut db = BakedModelDatabase::new();
    let oak = create_sample_cube("minecraft:oak_planks", "minecraft:block/oak_planks");
    db.insert("minecraft:oak_planks".to_string(), oak);

    let uncompressed_bytes = postcard::to_allocvec(&db).expect("serialize should succeed");
    assert_ne!(&uncompressed_bytes[..4], &[0x28, 0xb5, 0x2f, 0xfd]);

    let err = BakedModelDatabase::from_binary(&uncompressed_bytes)
        .expect_err("legacy uncompressed/bincode payload must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("legacy bincode caches"),
        "error must explain the version boundary, got: {message}"
    );
}

#[test]
fn test_real_cache_benchmark_if_available() {
    let Some(real_cache_path) = mtk_testkit::models_cache_bin() else {
        eprintln!("Real cache file not found, skipping benchmark (set MTK_TEST_MODELS_CACHE)");
        return;
    };

    let raw_file_bytes = fs::read(&real_cache_path).expect("Failed to read real models.bin");
    if raw_file_bytes.len() < 4 || raw_file_bytes[..4] != [0x28, 0xb5, 0x2f, 0xfd] {
        let err = BakedModelDatabase::from_binary(&raw_file_bytes)
            .expect_err("legacy models.bin must be rejected");
        assert!(err.to_string().contains("legacy bincode caches"));
        eprintln!("Skipping benchmark: $MTK_TEST_MODELS_CACHE is a legacy uncompressed cache");
        return;
    }
    let old_size = raw_file_bytes.len();
    println!(
        "\n[Benchmark] Original models.bin size: {:.2} MB ({} bytes)",
        old_size as f64 / 1_048_576.0,
        old_size
    );

    // Load original file
    let t0 = std::time::Instant::now();
    let loaded_db = BakedModelDatabase::from_binary(&raw_file_bytes)
        .expect("Failed to load original models.bin");
    let load_dur = t0.elapsed();
    println!(
        "[Benchmark] Loaded {} models in {:.2?}",
        loaded_db.models.len(),
        load_dur
    );

    // Re-serialize with new compact format and zstd compression
    let t1 = std::time::Instant::now();
    let new_bytes = loaded_db
        .to_binary()
        .expect("Failed to serialize with compact zstd");
    let save_dur = t1.elapsed();
    let new_size = new_bytes.len();
    println!(
        "[Benchmark] New compressed models.bin size: {:.2} MB ({} bytes), took {:.2?}",
        new_size as f64 / 1_048_576.0,
        new_size,
        save_dur
    );
    let ratio = (old_size as f64 - new_size as f64) / old_size as f64 * 100.0;
    println!(
        "[Benchmark] Space saved: {:.2}% (compression ratio: {:.2}x)",
        ratio,
        old_size as f64 / new_size as f64
    );

    // Decompress and verify
    let t2 = std::time::Instant::now();
    let reloaded =
        BakedModelDatabase::from_binary(&new_bytes).expect("Failed to reload compressed data");
    let reload_dur = t2.elapsed();
    println!("[Benchmark] Reloaded from compressed in {:.2?}", reload_dur);

    assert_eq!(loaded_db.models.len(), reloaded.models.len());
    assert_eq!(
        loaded_db.variant_groups.len(),
        reloaded.variant_groups.len()
    );
}
