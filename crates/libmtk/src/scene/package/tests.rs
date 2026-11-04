use super::*;
use crate::scene::metadata::SceneMetadata;
use crate::MtkError;
use mtk_model::{BakedModelDatabase, BlockEntityData};
use mtk_package::{ChunkWriteOptions, PackageProfile};
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;
use std::collections::HashMap;

#[test]
fn test_scene_package_roundtrip() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_scene_{}_{}.mtkscene",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut voxels = SparseVoxelPayload::new();
    voxels.bounds = [0, 0, 0, 16, 16, 16];
    voxels.push(
        1,
        2,
        3,
        "minecraft:stone",
        "minecraft:plains",
        None,
        Some(15),
    );
    voxels.push(
        1,
        3,
        3,
        "minecraft:item_frame[facing=north]",
        "minecraft:plains",
        Some(&BlockEntityData::new_item_frame(
            "minecraft:compass",
            1,
            0,
            false,
            false,
        )),
        Some(14),
    );

    let mut metadata = SceneMetadata::new(voxels.bounds, voxels.len());
    metadata.unique_states_count = voxels.state_palette.len();
    metadata.unique_biomes_count = voxels.biome_palette.len();
    metadata.unique_entities_count = voxels.entity_palette.len();

    let dummy_png = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG header stub
    let textures = vec![("atlas_0.png".to_string(), dummy_png.clone())];

    let mut model_db = BakedModelDatabase::new();
    model_db.insert(
        "minecraft:stone".to_string(),
        mtk_model::baker::baked_model::BakedModel {
            block_state: "minecraft:stone".to_string(),
            is_cube: true,
            is_opaque: true,
            ..Default::default()
        },
    );

    let mut write_req = SceneWriteRequest::new(&metadata, &voxels, &textures);
    write_req.models = Some(&model_db);
    write_scene_package(&temp_path, write_req).expect("Write scene package");

    let reader = ScenePackageReader::open_file(&temp_path).expect("Open scene package");
    assert_eq!(reader.metadata().voxel_count, 2);
    assert_eq!(reader.metadata().bounds, [0, 0, 0, 16, 16, 16]);
    assert!(!reader.has_ctm_rules());
    assert!(reader.read_ctm_solver().unwrap().is_none());

    let read_models = reader
        .read_models()
        .expect("Read models")
        .expect("Models present");
    assert_eq!(read_models.len(), 1);
    assert!(read_models.get("minecraft:stone").is_some());

    let read_voxels = reader.read_sparse_voxels().expect("Read sparse voxels");
    assert_eq!(read_voxels.len(), 2);
    assert_eq!(read_voxels.state_palette, voxels.state_palette);
    assert_eq!(read_voxels.columns.x, voxels.columns.x);

    let read_textures = reader.read_atlas_textures().expect("Read textures");
    assert_eq!(read_textures.len(), 1);
    assert!(read_textures.contains_key("atlas/textures/atlas_0.png"));
    assert_eq!(
        read_textures.get("atlas/textures/atlas_0.png").unwrap(),
        &dummy_png
    );

    // Verify cohesive unpack() delivers the full bundle in one pass
    let bundle = reader.unpack().expect("Unpack scene bundle");
    assert_eq!(bundle.sparse_voxels.len(), 2);
    assert_eq!(bundle.texture_names.len(), 1);
    assert_eq!(bundle.texture_names[0], "atlas/textures/atlas_0.png");
    assert_eq!(
        bundle.metadata.format_version,
        crate::scene::metadata::SCENE_PACKAGE_FORMAT_VERSION
    );

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_scene_package_gates_profile_and_version() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_gate_{}_{}.mtkscene",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    // 1. Write an AssetCache profile package
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::AssetCache,
        [0u8; 16],
    )
    .expect("Create writer");
    let mut metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    // Must fail with InvalidSceneProfile
    let err = match ScenePackageReader::open_file(&temp_path) {
        Ok(_) => panic!("Expected error, got Ok"),
        Err(e) => e,
    };
    match err {
        MtkError::InvalidSceneProfile(PackageProfile::AssetCache) => {}
        other => panic!("Expected InvalidSceneProfile, got {:?}", other),
    }

    // 2. Write with wrong format_version (e.g. 99)
    metadata.format_version = 99;
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::SceneInterchange,
        [0u8; 16],
    )
    .expect("Create writer");
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    let err = match ScenePackageReader::open_file(&temp_path) {
        Ok(_) => panic!("Expected error, got Ok"),
        Err(e) => e,
    };
    match err {
        MtkError::UnsupportedSceneFormatVersion(99) => {}
        other => panic!(
            "Expected UnsupportedSceneFormatVersion(99), got {:?}",
            other
        ),
    }

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_scene_package_corrupted_colormap_error() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_corrupt_cm_{}.mtkscene",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::SceneInterchange,
        [0u8; 16],
    )
    .unwrap();
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();
    writer
        .add_chunk(
            *b"CMAP",
            "biome/colormap/grass",
            b"not a valid png file",
            &ChunkWriteOptions::raw(),
        )
        .unwrap();
    writer.finish().unwrap();

    let reader = ScenePackageReader::open_file(&temp_path).unwrap();
    let res = reader.read_colormaps();
    assert!(res.is_err(), "Corrupted colormap must return Err, not Ok");

    let validation = reader.validate().expect("validate report");
    assert!(!validation.is_valid);
    assert!(!validation.errors.is_empty());

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_validate_missing_atlas_mapping_and_models() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_val_missing_{}.mtkscene",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::SceneInterchange,
        [0u8; 16],
    )
    .unwrap();
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    let reader = ScenePackageReader::open_file(&temp_path).unwrap();
    let report = reader.validate().unwrap();
    assert!(!report.is_valid);
    assert!(report.errors.iter().any(|e| e.contains("atlas/mapping")));
    assert!(report.errors.iter().any(|e| e.contains("models/database")));

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_validate_missing_texture_slice_and_unmapped_model_texture() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_val_textures_{}.mtkscene",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::SceneInterchange,
        [0u8; 16],
    )
    .unwrap();
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();

    let mut atlas_map = AtlasAddressMap::new();
    atlas_map.chunks.push(mtk_texture::AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 0,
        width: 512,
        height: 512,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: vec![mtk_texture::AtlasTextureSlot {
            role: "albedo".to_string(),
            file_name: "atlas_0.png".to_string(),
            colorspace: "sRGB".to_string(),
        }],
    });
    writer
        .add_json_chunk(
            *b"MAPG",
            "atlas/mapping",
            &atlas_map,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    let mut model_db = BakedModelDatabase::new();
    let mut model = mtk_model::baker::baked_model::BakedModel {
        block_state: "minecraft:stone".to_string(),
        ..Default::default()
    };
    model.faces[0].texture = "minecraft:block/missing_stone".to_string();
    model_db.insert("minecraft:stone".to_string(), model);

    let model_bytes = model_db.to_binary().unwrap();
    writer
        .add_chunk(
            *b"MODL",
            "models/database",
            &model_bytes,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();
    writer.finish().unwrap();

    let reader = ScenePackageReader::open_file(&temp_path).unwrap();
    let report = reader.validate().unwrap();

    // 1. Missing texture slice error (atlas_0.png was declared in map but not written to package)
    assert!(!report.is_valid);
    assert!(report.errors.iter().any(|e| e.contains("atlas_0.png")));

    // 2. Unmapped texture warning (missing_stone is not in atlas map)
    assert!(report.warnings.iter().any(|w| w.contains("missing_stone")));

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_validate_dynamic_entity_slots_missing_chunk() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_val_dyn_slots_{}.mtkscene",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::SceneInterchange,
        [0u8; 16],
    )
    .unwrap();
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();

    let atlas_map = AtlasAddressMap::new();
    writer
        .add_json_chunk(
            *b"MAPG",
            "atlas/mapping",
            &atlas_map,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();
    let model_db = BakedModelDatabase::new();
    let model_bytes = model_db.to_binary().unwrap();
    writer
        .add_chunk(
            *b"MODL",
            "models/database",
            &model_bytes,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    let mut slots = HashMap::new();
    slots.insert("custom_skin_steve".to_string(), (0xFFF0u16, 0u16));
    writer
        .add_json_chunk(*b"ESLT", "entity/slots", &slots, &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    let reader = ScenePackageReader::open_file(&temp_path).unwrap();
    let report = reader.validate().unwrap();
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("custom_skin_steve")));

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_validate_dynamic_entity_slots_valid() {
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!(
        "test_validate_dyn_slots_valid_{}.mtkscene",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        &temp_path,
        PackageProfile::SceneInterchange,
        [0u8; 16],
    )
    .unwrap();
    writer
        .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
        .unwrap();

    let mut map = AtlasAddressMap::new();
    let chunk_meta = mtk_texture::AtlasChunkMeta {
        chunk_id: 0,
        category: "scene".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 64,
        height: 64,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    };
    map.chunks.push(chunk_meta);

    let sprite_loc = mtk_texture::AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [0, 0, 64, 64],
        frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
        ..Default::default()
    };
    map.sprites.insert(
        mtk_resource::ResourceLocation::new("minecraft", "skin/player_hero"),
        sprite_loc,
    );

    writer
        .add_json_chunk(*b"MAPG", "atlas/mapping", &map, &ChunkWriteOptions::raw())
        .unwrap();
    writer
        .add_chunk(
            *b"TX2D",
            "atlas/textures/scene_chunk_001.png",
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    let model_db = BakedModelDatabase::new();
    let model_bytes = model_db.to_binary().unwrap();
    writer
        .add_chunk(
            *b"MODL",
            "models/database",
            &model_bytes,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();

    let mut slots = HashMap::new();
    slots.insert("player_Hero".to_string(), (0xFFF0u16, 0u16));
    writer
        .add_json_chunk(*b"ESLT", "entity/slots", &slots, &ChunkWriteOptions::raw())
        .unwrap();
    writer.finish().unwrap();

    let reader = ScenePackageReader::open_file(&temp_path).unwrap();
    let report = reader.validate().unwrap();
    assert!(
        report.is_valid,
        "Expected valid report, but got errors: {:?}",
        report.errors
    );
    assert!(report.errors.is_empty());

    let _ = std::fs::remove_file(&temp_path);
}
