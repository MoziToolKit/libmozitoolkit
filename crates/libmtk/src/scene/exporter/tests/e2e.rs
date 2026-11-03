use std::collections::HashMap;

use mtk_model::{BakedModelDatabase, BlockEntityData};
use mtk_resource::ResourceLocation;
use mtk_texture::atlas::{
    AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, DYNAMIC_ATLAS_HEIGHT,
    DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START,
};
use mtk_texture::RgbaBuffer;
use mtk_voxel::SparseVoxelPayload;

use super::create_sample_cube;
use crate::scene::exporter::{build_scene_interchange, export_scene_package, SceneExporterConfig};

#[test]
fn test_export_dynamic_entity_atlas_stitching() {
    let mut source_map = AtlasAddressMap::new();
    let chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 32,
        height: 32,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    };
    source_map.chunks.push(chunk_meta);

    // Add 0xFFF0 dynamic entity chunk to source map
    let dyn_chunk_meta = AtlasChunkMeta {
        chunk_id: DYNAMIC_CHUNK_START,
        category: "entities".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: DYNAMIC_ATLAS_WIDTH,
        height: DYNAMIC_ATLAS_HEIGHT,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    };
    source_map.chunks.push(dyn_chunk_meta);

    let skin_key = "test_player_skin";
    let skin_loc = AtlasSpriteLocation {
        chunk_id: DYNAMIC_CHUNK_START,
        pixel_rect: [0, 0, 64, 64],
        ..Default::default()
    };
    source_map.sprites.insert(
        ResourceLocation::new("minecraft", format!("skin/{}", skin_key)),
        skin_loc,
    );

    let mut source_images = HashMap::new();
    source_images.insert(
        "blocks_chunk_001.png".to_string(),
        RgbaBuffer::solid(32, 32, 100, 100, 100, 255),
    );

    let mut dynamic_entity_atlases = HashMap::new();
    let mut dyn_sheet = RgbaBuffer::new(DYNAMIC_ATLAS_WIDTH, DYNAMIC_ATLAS_HEIGHT);
    dyn_sheet.blit(
        &RgbaBuffer::solid(64, 64, 250, 180, 140, 255),
        0,
        0,
        0,
        0,
        64,
        64,
    );
    dynamic_entity_atlases.insert(DYNAMIC_CHUNK_START, dyn_sheet.pixels);

    let mut source_models = BakedModelDatabase::new();
    let skull_model = create_sample_cube(
        "minecraft:player_head[rotation=0]",
        &format!("minecraft:skin/{}", skin_key),
    );
    source_models.insert("minecraft:player_head[rotation=0]".to_string(), skull_model);

    let mut voxels = SparseVoxelPayload::new();
    let skull_entity = BlockEntityData::Skull {
        kind: mtk_model::entity::skull::SkullKind::Player,
        profile: Some(mtk_model::entity::skull::PlayerProfileData {
            id: Some(skin_key.to_string()),
            name: Some("TestPlayer".to_string()),
            skin_url: None,
            skin_base64: None,
        }),
        rotation: Some(0),
    };
    voxels.push(
        0,
        0,
        0,
        "minecraft:player_head[rotation=0]",
        "minecraft:plains",
        Some(&skull_entity),
        None,
    );

    let config = SceneExporterConfig::default();
    let result = build_scene_interchange(
        &voxels,
        &source_models,
        &source_map,
        &mut source_images,
        &config,
        Some(&dynamic_entity_atlases),
    )
    .expect("Build scene interchange with dynamic entities");

    // Verify that the skull model was remapped to mini-atlas chunk 0, NOT 0xFFF0!
    let remapped = result
        .tree_shaken_models
        .get("minecraft:player_head[rotation=0]")
        .unwrap();
    let face = &remapped.culled_faces[0][0];
    assert_eq!(
        face.atlas_chunk_id,
        Some(0),
        "Dynamic entity face must be stitched into normal mini-atlas chunk 0"
    );
}

#[test]
fn test_export_scene_package_file_roundtrip() {
    let temp_path = std::env::temp_dir().join(format!(
        "test_export_scene_{}_{}.mtkscene",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut source_map = AtlasAddressMap::new();
    let chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 32,
        height: 32,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    };
    source_map.chunks.push(chunk_meta);

    let loc_stone = AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [0, 0, 16, 16],
        frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
        ..Default::default()
    };
    source_map
        .sprites
        .insert(ResourceLocation::new("minecraft", "block/stone"), loc_stone);

    let mut source_images = HashMap::new();
    let mut fake_source_sheet = RgbaBuffer::new(32, 32);
    fake_source_sheet.blit(
        &RgbaBuffer::solid(16, 16, 100, 100, 100, 255),
        0,
        0,
        0,
        0,
        16,
        16,
    );
    source_images.insert("blocks_chunk_001.png".to_string(), fake_source_sheet);

    let mut source_models = BakedModelDatabase::new();
    source_models.insert(
        "minecraft:stone".to_string(),
        create_sample_cube("minecraft:stone", "minecraft:block/stone"),
    );

    let mut voxels = SparseVoxelPayload::new();
    voxels.bounds = [10, 60, 10, 5, 5, 5];
    voxels.push(
        10,
        60,
        10,
        "minecraft:stone",
        "minecraft:plains",
        None,
        Some(15),
    );

    let config = SceneExporterConfig::default();
    let export_res = export_scene_package(
        &temp_path,
        &voxels,
        &source_models,
        &source_map,
        &mut source_images,
        &config,
        None,
        None,
    )
    .expect("Export scene package file");

    assert_eq!(export_res.sprite_count, 1);

    // Read package back from disk
    let reader = crate::scene::ScenePackageReader::open_file(&temp_path)
        .expect("Open exported scene package");
    assert_eq!(reader.metadata().voxel_count, 1);
    assert_eq!(reader.metadata().bounds, [10, 60, 10, 5, 5, 5]);

    let read_voxels = reader.read_sparse_voxels().expect("Read voxels");
    assert_eq!(read_voxels.len(), 1);
    assert_eq!(read_voxels.state_palette, vec!["minecraft:stone"]);

    let read_models = reader
        .read_models()
        .expect("Read models")
        .expect("Models must exist");
    assert_eq!(read_models.len(), 1);
    assert!(read_models.get("minecraft:stone").is_some());

    let read_mapping = reader
        .read_atlas_mapping()
        .expect("Read atlas mapping")
        .expect("Mapping must exist");
    assert!(read_mapping.lookup_str("minecraft:block/stone").is_some());

    let read_textures = reader.read_atlas_textures().expect("Read atlas textures");
    assert_eq!(read_textures.len(), 1);

    let _ = std::fs::remove_file(&temp_path);
}
