use std::collections::HashMap;

use mtk_model::BakedModelDatabase;
use mtk_resource::ResourceLocation;
use mtk_texture::atlas::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation};
use mtk_texture::RgbaBuffer;
use mtk_voxel::SparseVoxelPayload;

use super::create_sample_cube;
use crate::scene::exporter::{build_scene_interchange, SceneExporterConfig};

#[test]
fn test_build_scene_interchange_pipeline() {
    let mut source_map = AtlasAddressMap::new();
    let chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
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
    source_map.chunks.push(chunk_meta);

    let loc_stone = AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [0, 0, 16, 16],
        frame_0_uv_bounds: [0.0, 0.0, 0.25, 0.25],
        ..Default::default()
    };
    let res_stone = ResourceLocation::new("minecraft", "block/stone");
    source_map.sprites.insert(res_stone, loc_stone);

    let loc_dirt = AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [16, 0, 16, 16],
        frame_0_uv_bounds: [0.25, 0.0, 0.5, 0.25],
        ..Default::default()
    };
    let res_dirt = ResourceLocation::new("minecraft", "block/dirt");
    source_map.sprites.insert(res_dirt, loc_dirt);

    let mut source_images = HashMap::new();
    let mut fake_source_sheet = RgbaBuffer::new(64, 64);
    fake_source_sheet.blit(
        &RgbaBuffer::solid(16, 16, 255, 0, 0, 255),
        0,
        0,
        0,
        0,
        16,
        16,
    );
    fake_source_sheet.blit(
        &RgbaBuffer::solid(16, 16, 139, 69, 19, 255),
        0,
        0,
        16,
        0,
        16,
        16,
    );
    source_images.insert("blocks_chunk_001.png".to_string(), fake_source_sheet);

    let stone_model = create_sample_cube("minecraft:stone", "minecraft:block/stone");
    let dirt_model = create_sample_cube("minecraft:dirt", "minecraft:block/dirt");
    let bedrock_model = create_sample_cube("minecraft:bedrock", "minecraft:block/bedrock");

    let mut source_models = BakedModelDatabase::new();
    source_models.insert("minecraft:stone".to_string(), stone_model);
    source_models.insert("minecraft:dirt".to_string(), dirt_model);
    source_models.insert("minecraft:bedrock".to_string(), bedrock_model);

    let mut voxels = SparseVoxelPayload::new();
    voxels.bounds = [0, 0, 0, 16, 16, 16];
    voxels.push(0, 0, 0, "minecraft:stone", "minecraft:plains", None, None);

    let config = SceneExporterConfig::default();
    let result = build_scene_interchange(
        &voxels,
        &source_models,
        &source_map,
        &mut source_images,
        &config,
        None,
    )
    .expect("Build scene interchange");

    assert_eq!(result.tree_shaken_models.len(), 1);
    assert!(result.tree_shaken_models.get("minecraft:stone").is_some());
    assert!(result.tree_shaken_models.get("minecraft:bedrock").is_none());

    assert_eq!(result.sprite_count, 1);
    assert_eq!(result.mini_atlas_textures.len(), 1);

    let remapped_stone = result.tree_shaken_models.get("minecraft:stone").unwrap();
    let up_face = remapped_stone.culled_faces[0][0].clone();
    assert!(up_face.atlas_uvs.is_some());
    let atlas_uvs = up_face.atlas_uvs.unwrap();
    assert!(atlas_uvs[0].x >= 0.0 && atlas_uvs[0].x <= 1.0);
    assert!(atlas_uvs[0].y >= 0.0 && atlas_uvs[0].y <= 1.0);
}

#[test]
fn test_uv_coordinate_formula_orientation() {
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
        ..Default::default()
    };
    source_map
        .sprites
        .insert(ResourceLocation::new("minecraft", "block/stone"), loc_stone);

    let mut source_images = HashMap::new();
    source_images.insert(
        "blocks_chunk_001.png".to_string(),
        RgbaBuffer::solid(32, 32, 100, 100, 100, 255),
    );

    let mut source_models = BakedModelDatabase::new();
    source_models.insert(
        "minecraft:stone".to_string(),
        create_sample_cube("minecraft:stone", "minecraft:block/stone"),
    );

    let mut voxels = SparseVoxelPayload::new();
    voxels.push(0, 0, 0, "minecraft:stone", "minecraft:plains", None, None);

    let config = SceneExporterConfig::default();
    let result = build_scene_interchange(
        &voxels,
        &source_models,
        &source_map,
        &mut source_images,
        &config,
        None,
    )
    .expect("Build scene interchange");

    let loc = result
        .mini_atlas_map
        .lookup_str("minecraft:block/stone")
        .unwrap();
    assert_eq!(loc.frame_0_uv_bounds[0], 0.0);
    assert_eq!(loc.frame_0_uv_bounds[1], 0.0);
    assert_eq!(loc.frame_0_uv_bounds[2], 1.0);
    assert_eq!(loc.frame_0_uv_bounds[3], 1.0);
}

#[test]
fn test_export_with_overlay_and_animated_strips() {
    let mut source_map = AtlasAddressMap::new();
    let static_chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 32,
        height: 32,
        has_normal: false,
        has_specular: false,
        has_overlay: true,
        has_orm: false,
        has_aux: false,
        has_tint: true,
        pbr: None,
        textures: Vec::new(),
    };
    let anim_chunk_meta = AtlasChunkMeta {
        chunk_id: 1,
        category: "blocks".to_string(),
        is_animated: true,
        category_chunk_index: 1,
        width: 16,
        height: 32,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: true,
        pbr: None,
        textures: Vec::new(),
    };
    source_map.chunks.push(static_chunk_meta);
    source_map.chunks.push(anim_chunk_meta);

    let loc_grass_side = AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [0, 0, 16, 16],
        has_overlay: true,
        ..Default::default()
    };
    source_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/grass_block_side"),
        loc_grass_side,
    );

    let loc_water_static = AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [16, 0, 16, 16],
        is_animated: false,
        ..Default::default()
    };
    source_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_still"),
        loc_water_static,
    );

    let loc_water_anim = AtlasSpriteLocation {
        chunk_id: 1,
        is_animated: true,
        strip_pixel_rect: [0, 0, 16, 32],
        frame_size: [16, 16],
        frame_count: 2,
        ..Default::default()
    };
    source_map.anim_sprites.insert(
        ResourceLocation::new("minecraft", "block/water_still"),
        loc_water_anim,
    );

    let mut source_images = HashMap::new();
    let mut static_sheet = RgbaBuffer::new(32, 32);
    static_sheet.blit(
        &RgbaBuffer::solid(16, 16, 120, 80, 50, 255),
        0,
        0,
        0,
        0,
        16,
        16,
    );
    source_images.insert("blocks_chunk_001.png".to_string(), static_sheet);

    let mut overlay_sheet = RgbaBuffer::new(32, 32);
    overlay_sheet.blit(
        &RgbaBuffer::solid(16, 16, 100, 200, 50, 255),
        0,
        0,
        0,
        0,
        16,
        16,
    );
    source_images.insert("blocks_chunk_001_overlay.png".to_string(), overlay_sheet);

    let anim_sheet = RgbaBuffer::solid(16, 32, 50, 100, 200, 255);
    source_images.insert("blocks_anim_chunk_001.png".to_string(), anim_sheet);

    let mut source_models = BakedModelDatabase::new();
    source_models.insert(
        "minecraft:grass_block".to_string(),
        create_sample_cube("minecraft:grass_block", "minecraft:block/grass_block_side"),
    );
    source_models.insert(
        "minecraft:water[level=0]".to_string(),
        create_sample_cube("minecraft:water[level=0]", "minecraft:block/water_still"),
    );

    let mut voxels = SparseVoxelPayload::new();
    voxels.push(
        0,
        0,
        0,
        "minecraft:grass_block",
        "minecraft:plains",
        None,
        None,
    );
    voxels.push(
        0,
        1,
        0,
        "minecraft:water[level=0]",
        "minecraft:plains",
        None,
        None,
    );

    let config = SceneExporterConfig::default();
    let result = build_scene_interchange(
        &voxels,
        &source_models,
        &source_map,
        &mut source_images,
        &config,
        None,
    )
    .expect("Build scene interchange with overlay and anim");

    assert!(result.metadata.has_animated_atlas);
    assert!(result
        .mini_atlas_textures
        .iter()
        .any(|(name, _)| name.contains("_overlay.png")));
    assert!(result
        .mini_atlas_textures
        .iter()
        .any(|(name, _)| name.contains("scene_anim_chunk_") || name.contains("scene_anim_")));
    assert!(result
        .mini_atlas_map
        .anim_sprites
        .contains_key(&ResourceLocation::new("minecraft", "block/water_still")));
}
