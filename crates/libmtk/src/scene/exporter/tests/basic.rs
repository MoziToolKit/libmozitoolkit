use std::collections::HashMap;

use mtk_model::BakedModelDatabase;
use mtk_resource::ResourceLocation;
use mtk_texture::RgbaBuffer;
use mtk_texture::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation};
use mtk_voxel::SparseVoxelPayload;

use super::create_sample_cube;
use crate::scene::exporter::{
    build_scene_interchange, CtmMode, SceneExportRequest, SceneExporterConfig,
};

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
    let result = build_scene_interchange(SceneExportRequest {
        voxels: &voxels,
        source_models: &source_models,
        source_atlas_map: &source_map,
        atlas_source: &mut source_images,
        config: &config,
        dynamic_entity_atlases: None,
        dynamic_entity_slots: None,
        ctm_mode: CtmMode::Disabled,
    })
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
    let result = build_scene_interchange(SceneExportRequest {
        voxels: &voxels,
        source_models: &source_models,
        source_atlas_map: &source_map,
        atlas_source: &mut source_images,
        config: &config,
        dynamic_entity_atlases: None,
        dynamic_entity_slots: None,
        ctm_mode: CtmMode::Disabled,
    })
    .expect("Build scene interchange");

    let fallback_loc = result.mini_atlas_map.lookup_str("mozi:fallback").unwrap();
    assert_eq!(fallback_loc.frame_0_uv_bounds[0], 0.0);
    assert_eq!(fallback_loc.frame_0_uv_bounds[1], 0.0);
    assert_eq!(fallback_loc.frame_0_uv_bounds[2], 0.5);
    assert_eq!(fallback_loc.frame_0_uv_bounds[3], 1.0);

    let loc = result
        .mini_atlas_map
        .lookup_str("minecraft:block/stone")
        .unwrap();
    assert_eq!(loc.frame_0_uv_bounds[0], 0.5);
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
    let result = build_scene_interchange(SceneExportRequest {
        voxels: &voxels,
        source_models: &source_models,
        source_atlas_map: &source_map,
        atlas_source: &mut source_images,
        config: &config,
        dynamic_entity_atlases: None,
        dynamic_entity_slots: None,
        ctm_mode: CtmMode::Disabled,
    })
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

use crate::scene::exporter::cropper::{
    AtlasLayerSource, BiomeAssetSource, CtmAssetSource, EntityAssetSource, SceneAtlasSource,
    SceneSourceCapabilities,
};
use crate::MtkError;
use mtk_texture::BannerBaker;

struct MockAtlasSource {
    textures: HashMap<String, RgbaBuffer>,
    colormaps: HashMap<String, Vec<u8>>,
    biome_resolver_json: Option<String>,
    ctm_rules_json: Option<String>,
}

impl AtlasLayerSource for MockAtlasSource {
    fn get_albedo(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        Ok(self.textures.get(&format!("{}.png", chunk_stem)).cloned())
    }
    fn get_normal(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        Ok(self.textures.get(&format!("{}_n.png", chunk_stem)).cloned())
    }
    fn get_orm(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        Ok(self
            .textures
            .get(&format!("{}_orm.png", chunk_stem))
            .cloned())
    }
    fn get_aux(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        Ok(self
            .textures
            .get(&format!("{}_aux.png", chunk_stem))
            .cloned())
    }
    fn get_overlay(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        Ok(self
            .textures
            .get(&format!("{}_overlay.png", chunk_stem))
            .cloned())
    }
}

impl BiomeAssetSource for MockAtlasSource {
    fn get_colormap(&mut self, name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        Ok(self.colormaps.get(name).cloned())
    }
    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError> {
        Ok(self.biome_resolver_json.clone())
    }
}

impl CtmAssetSource for MockAtlasSource {
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError> {
        Ok(self.ctm_rules_json.clone())
    }
}

impl EntityAssetSource for MockAtlasSource {
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError> {
        Ok(None)
    }
}

impl SceneAtlasSource for MockAtlasSource {
    fn capabilities(&self) -> SceneSourceCapabilities {
        SceneSourceCapabilities {
            has_colormaps: !self.colormaps.is_empty(),
            has_biome_resolver: self.biome_resolver_json.is_some(),
            has_ctm_rules: self.ctm_rules_json.is_some(),
            has_banner_baker: false,
        }
    }
}

#[test]
fn test_scene_export_pbr_channels_and_colormaps_roundtrip() {
    use crate::scene::exporter::export_scene_package;
    use crate::scene::package::ScenePackageReader;

    let mut source_map = AtlasAddressMap::new();
    let chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 16,
        height: 16,
        has_normal: true,
        has_specular: false,
        has_overlay: true,
        has_orm: true,
        has_aux: true,
        has_tint: true,
        pbr: None,
        textures: Vec::new(),
    };
    source_map.chunks.push(chunk_meta);

    let loc_grass = AtlasSpriteLocation {
        chunk_id: 0,
        pixel_rect: [0, 0, 16, 16],
        frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
        has_normal: true,
        has_overlay: true,
        ..Default::default()
    };
    source_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/grass_block_side"),
        loc_grass,
    );

    let mut textures = HashMap::new();
    textures.insert(
        "blocks_chunk_001.png".to_string(),
        RgbaBuffer::solid(16, 16, 100, 150, 50, 255),
    );
    textures.insert(
        "blocks_chunk_001_n.png".to_string(),
        RgbaBuffer::solid(16, 16, 128, 128, 255, 255),
    );
    textures.insert(
        "blocks_chunk_001_orm.png".to_string(),
        RgbaBuffer::solid(16, 16, 255, 128, 0, 64),
    );
    textures.insert(
        "blocks_chunk_001_aux.png".to_string(),
        RgbaBuffer::solid(16, 16, 20, 0, 0, 0),
    );
    textures.insert(
        "blocks_chunk_001_overlay.png".to_string(),
        RgbaBuffer::solid(16, 16, 200, 255, 100, 255),
    );

    let custom_grass_png = RgbaBuffer::solid(256, 256, 10, 220, 30, 255)
        .to_png_bytes()
        .unwrap();
    let mut colormaps = HashMap::new();
    colormaps.insert("grass".to_string(), custom_grass_png);

    let dummy_resolver_json = mtk_material::BiomeResolver::new().to_json().unwrap();

    let mut source = MockAtlasSource {
        textures,
        colormaps,
        biome_resolver_json: Some(dummy_resolver_json),
        ctm_rules_json: None,
    };

    let mut source_models = BakedModelDatabase::new();
    source_models.insert(
        "minecraft:grass_block".to_string(),
        create_sample_cube("minecraft:grass_block", "minecraft:block/grass_block_side"),
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

    let temp_file = std::env::temp_dir().join(format!(
        "test_pbr_colormap_{}_{}.mtkscene",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let config = SceneExporterConfig::default();
    let export_result = export_scene_package(
        &temp_file,
        SceneExportRequest {
            voxels: &voxels,
            source_models: &source_models,
            source_atlas_map: &source_map,
            atlas_source: &mut source,
            config: &config,
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
            ctm_mode: CtmMode::Disabled,
        },
    )
    .expect("Export scene package");

    // 1. Verify Mini-Atlas contains all 5 PBR texture layers
    assert!(export_result
        .mini_atlas_textures
        .iter()
        .any(|(n, _)| n.ends_with("chunk_001.png")));
    assert!(export_result
        .mini_atlas_textures
        .iter()
        .any(|(n, _)| n.ends_with("_n.png")));
    assert!(export_result
        .mini_atlas_textures
        .iter()
        .any(|(n, _)| n.ends_with("_orm.png")));
    assert!(export_result
        .mini_atlas_textures
        .iter()
        .any(|(n, _)| n.ends_with("_aux.png")));
    assert!(export_result
        .mini_atlas_textures
        .iter()
        .any(|(n, _)| n.ends_with("_overlay.png")));

    // 2. Open package and verify colormaps and BiomeResolver (no implicit global side-effects)
    let reader = ScenePackageReader::open_file(&temp_file).expect("Open exported package");
    assert!(reader.has_colormaps());
    assert!(reader.read_colormap("grass").unwrap().is_some());
    assert!(reader.read_biome_resolver().unwrap().is_some());

    // Read colormaps and verify custom grass color (10, 220, 30)
    let cms = reader.read_colormaps().unwrap().expect("colormaps present");
    let sampled_grass = cms.sample_grass_linear(0.8, 0.4);
    // Component R in [0..1] should be close to 10/255 ~ 0.039 (converted to linear space)
    assert!(sampled_grass[0] < 0.1);
    assert!(sampled_grass[1] > 0.5);

    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn test_scene_export_ctm_rules_and_subtiles_roundtrip() {
    use crate::scene::exporter::{export_scene_package, CtmMode, SceneExportRequest};
    use crate::scene::package::ScenePackageReader;
    use mtk_resource::CtmRule;

    let mut textures = HashMap::new();
    // 1. Base texture for glass
    textures.insert(
        "blocks_chunk_001.png".to_string(),
        RgbaBuffer::solid(16, 16, 255, 255, 255, 100),
    );

    let mut source_map = AtlasAddressMap::new();
    let chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 16,
        height: 16,
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

    // Register base texture
    source_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/glass"),
        AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 16, 16],
            frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            ..Default::default()
        },
    );

    // Register CTM sub-tile 0
    source_map.sprites.insert(
        ResourceLocation::new("minecraft", "optifine/ctm/glass/0"),
        AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 16, 16],
            frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            ..Default::default()
        },
    );

    let ctm_props = "matchBlocks=glass\nmethod=horizontal\ntiles=0";
    let rule = CtmRule::parse_properties(
        "optifine/ctm/glass/glass.properties",
        "minecraft",
        ctm_props,
    )
    .expect("Parse CTM rule");

    let mut source = MockAtlasSource {
        textures,
        colormaps: HashMap::new(),
        biome_resolver_json: None,
        ctm_rules_json: None,
    };

    let mut source_models = BakedModelDatabase::new();
    source_models.insert(
        "minecraft:glass".to_string(),
        create_sample_cube("minecraft:glass", "minecraft:block/glass"),
    );

    let mut voxels = SparseVoxelPayload::new();
    voxels.push(0, 0, 0, "minecraft:glass", "minecraft:plains", None, None);

    let temp_file = std::env::temp_dir().join(format!(
        "test_ctm_export_{}_{}.mtkscene",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let config = SceneExporterConfig::default();
    let export_result = export_scene_package(
        &temp_file,
        SceneExportRequest {
            voxels: &voxels,
            source_models: &source_models,
            source_atlas_map: &source_map,
            atlas_source: &mut source,
            config: &config,
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
            ctm_mode: CtmMode::Explicit(&[rule]),
        },
    )
    .expect("Export scene package with CTM");

    // 1. Verify CTM rule was preserved in export result
    assert_eq!(export_result.ctm_rules.len(), 1);
    assert!(!export_result.ctm_rules.is_empty());

    // 2. Verify Mini-Atlas mapped the CTM sub-tile!
    let subtile_loc = ResourceLocation::new("minecraft", "optifine/ctm/glass/0");
    assert!(
        export_result.mini_atlas_map.lookup(&subtile_loc).is_some(),
        "Mini-atlas must contain the CTM sub-tile"
    );

    // 3. Open scene package and verify CtmSolver reconstruction
    let reader = ScenePackageReader::open_file(&temp_file).expect("Open exported package");
    assert!(reader.has_ctm_rules());
    let solver = reader
        .read_ctm_solver()
        .expect("Read solver")
        .expect("Solver exists");
    assert_eq!(solver.rules.len(), 1);

    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn test_scene_export_banner_with_patterns_and_custom_baker() {
    use crate::scene::exporter::cropper::SceneAtlasSource;
    use crate::scene::exporter::export_scene_package;
    use mtk_model::banner::BannerPatternLayer;
    use mtk_model::BlockEntityData;
    use mtk_texture::BannerBaker;

    struct BannerTestAtlasSource {
        baker: BannerBaker,
    }

    impl AtlasLayerSource for BannerTestAtlasSource {
        fn get_albedo(
            &mut self,
            _chunk_id: u16,
            _stem: &str,
        ) -> Result<Option<RgbaBuffer>, crate::MtkError> {
            Ok(None)
        }
        fn get_normal(
            &mut self,
            _chunk_id: u16,
            _stem: &str,
        ) -> Result<Option<RgbaBuffer>, crate::MtkError> {
            Ok(None)
        }
        fn get_orm(
            &mut self,
            _chunk_id: u16,
            _stem: &str,
        ) -> Result<Option<RgbaBuffer>, crate::MtkError> {
            Ok(None)
        }
        fn get_aux(
            &mut self,
            _chunk_id: u16,
            _stem: &str,
        ) -> Result<Option<RgbaBuffer>, crate::MtkError> {
            Ok(None)
        }
        fn get_overlay(
            &mut self,
            _chunk_id: u16,
            _stem: &str,
        ) -> Result<Option<RgbaBuffer>, crate::MtkError> {
            Ok(None)
        }
    }

    impl BiomeAssetSource for BannerTestAtlasSource {
        fn get_colormap(&mut self, _name: &str) -> Result<Option<Vec<u8>>, crate::MtkError> {
            Ok(None)
        }
        fn get_biome_resolver_json(&mut self) -> Result<Option<String>, crate::MtkError> {
            Ok(None)
        }
    }

    impl CtmAssetSource for BannerTestAtlasSource {
        fn get_ctm_rules_json(&mut self) -> Result<Option<String>, crate::MtkError> {
            Ok(None)
        }
    }

    impl EntityAssetSource for BannerTestAtlasSource {
        fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, crate::MtkError> {
            Ok(Some(self.baker.clone()))
        }
    }

    impl SceneAtlasSource for BannerTestAtlasSource {
        fn capabilities(&self) -> SceneSourceCapabilities {
            SceneSourceCapabilities {
                has_banner_baker: true,
                ..Default::default()
            }
        }
    }

    let mut baker = BannerBaker::default();
    // Register a pattern with an opaque pixel at (10, 10)
    let mut flower_pattern = RgbaBuffer::new(64, 64);
    flower_pattern.set_pixel(10, 10, [255, 255, 255, 255]);
    baker.register_pattern_sprite("flower", flower_pattern);

    let mut source = BannerTestAtlasSource { baker };
    let source_map = AtlasAddressMap::new();

    let mut source_models = BakedModelDatabase::new();
    let banner_key = "minecraft:banner/white+flower#blue";
    source_models.insert(
        "minecraft:white_banner".to_string(),
        create_sample_cube("minecraft:white_banner", banner_key),
    );

    let mut voxels = SparseVoxelPayload::new();
    let banner_be = BlockEntityData::Banner {
        base_color: "white".to_string(),
        patterns: vec![BannerPatternLayer::new("flower", "blue")],
    };
    voxels.push(
        0,
        0,
        0,
        "minecraft:white_banner",
        "minecraft:plains",
        Some(&banner_be),
        None,
    );

    let temp_file = std::env::temp_dir().join(format!(
        "test_banner_pattern_{}_{}.mtkscene",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let config = SceneExporterConfig::default();
    let export_result = export_scene_package(
        &temp_file,
        SceneExportRequest {
            voxels: &voxels,
            source_models: &source_models,
            source_atlas_map: &source_map,
            atlas_source: &mut source,
            config: &config,
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
            ctm_mode: CtmMode::Disabled,
        },
    )
    .expect("Export banner scene package");

    // Verify mini atlas contains the banner texture
    let banner_loc = ResourceLocation::new("minecraft", "banner/white+flower#blue");
    assert!(
        export_result.mini_atlas_map.lookup(&banner_loc).is_some(),
        "Mini-atlas must map the synthesized banner texture"
    );

    // Verify mini-atlas texture has blue pixels from the flower pattern
    let (_tex_name, png_bytes) = &export_result.mini_atlas_textures[0];
    let decoded_atlas = RgbaBuffer::from_png_bytes(png_bytes).unwrap();
    // Blue dye color in vanilla banner is predominantly blue (B > 100, R < 100)
    let has_blue_pixel = decoded_atlas
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[2] > 100 && p[0] < 100 && p[3] > 0);
    assert!(
        has_blue_pixel,
        "Mini-atlas must contain the blue tinted flower pattern, not just white cloth!"
    );

    let _ = std::fs::remove_file(&temp_file);
}
