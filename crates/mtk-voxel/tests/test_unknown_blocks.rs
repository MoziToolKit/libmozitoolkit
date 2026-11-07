use glam::IVec3;
use mtk_cull::FaceCuller;
use mtk_model::{BakedModel, BakedModelDatabase};
use mtk_resource::ResourceLocation;
use mtk_texture::{AtlasAddressMap, AtlasSpriteLocation};
use mtk_voxel::{
    MesherConfig, ModelSource, SectionMesher, UnknownBlockStrategy, VoxelPointCloud, VoxelStorage,
};
use std::sync::Arc;

#[test]
fn test_scan_unknown_blocks_filters_placeholders_and_detects_unknowns() {
    let mut storage = VoxelStorage::new();
    // Known / placeholder / air blocks
    storage.set_block(0, 0, 0, "minecraft:air", None);
    storage.set_block(1, 0, 0, "minecraft:bubble_column[drag=true]", None);
    storage.set_block(2, 0, 0, "minecraft:light[level=15,waterlogged=true]", None);
    storage.set_block(3, 0, 0, "minecraft:barrier", None);
    storage.set_block(4, 0, 0, "minecraft:water[level=0]", None);

    // Known block with mock model
    storage.set_block(5, 0, 0, "minecraft:stone", None);

    // Truly unknown blocks
    storage.set_block(6, 0, 0, "twilightforest:ironwood_block", None);
    storage.set_block(7, 0, 0, "custom_mod:magic_crystal[active=true]", None);
    storage.set_block(8, 0, 0, "twilightforest:ironwood_block", None); // duplicate

    let mut model_db = BakedModelDatabase::default();
    model_db
        .models
        .insert("minecraft:stone".to_string(), BakedModel::default());

    let mut address_map = AtlasAddressMap::default();
    let fallback_loc = AtlasSpriteLocation {
        chunk_id: 0,
        uv_bounds: [0.0, 0.0, 0.0625, 0.0625],
        frame_0_uv_bounds: [0.0, 0.0, 0.0625, 0.0625],
        ..Default::default()
    };
    address_map.sprites.insert(
        ResourceLocation::parse("mozi:fallback").unwrap(),
        fallback_loc,
    );

    let unknowns = storage.scan_unknown_blocks(Some(&model_db), Some(&address_map), None);

    assert_eq!(
        unknowns,
        vec![
            "custom_mod:magic_crystal".to_string(),
            "twilightforest:ironwood_block".to_string(),
        ],
        "Only custom/unknown blocks lacking models and textures should be reported"
    );

    // Also test VoxelPointCloud scanning
    let cloud = VoxelPointCloud::extract_from_storage(&storage, &MesherConfig::default());
    let cloud_unknowns = cloud.scan_unknown_blocks(Some(&model_db), Some(&address_map), None);
    assert_eq!(cloud_unknowns, unknowns);
}

#[test]
fn test_unknown_block_strategy_fallback_cube_vs_ignore_air() {
    let mut address_map = AtlasAddressMap::default();
    let fallback_loc = AtlasSpriteLocation {
        chunk_id: 0,
        uv_bounds: [0.0, 0.0, 0.0625, 0.0625],
        frame_0_uv_bounds: [0.0, 0.0, 0.0625, 0.0625],
        ..Default::default()
    };
    address_map.sprites.insert(
        ResourceLocation::parse("mozi:fallback").unwrap(),
        fallback_loc,
    );
    let atlas_arc = Arc::new(address_map);

    let culler = FaceCuller::default();

    // 1. FallbackCube: unknown block emits 6 faces
    let mut world_fallback = VoxelStorage::new();
    world_fallback.set_bounds(0, 0, 0, 16, 16, 16);
    world_fallback.set_block(5, 5, 5, "unknown_mod:alien_block", None);

    let padded_fb = world_fallback.get_section_padded_array(IVec3::new(0, 0, 0));
    let cfg_fb = MesherConfig {
        atlas_address_map: Some(atlas_arc.clone()),
        unknown_block_strategy: UnknownBlockStrategy::FallbackCube,
        ..Default::default()
    };

    let mesh_fb = SectionMesher::mesh_section(&padded_fb, &culler, |_| ModelSource::None, &cfg_fb);
    assert!(
        mesh_fb.quad_count() > 0,
        "FallbackCube strategy must emit geometry for unknown blocks"
    );
    assert_eq!(mesh_fb.quad_count(), 6, "Isolated cube has 6 quads");

    // 2. IgnoreAsAir: unknown block emits 0 faces
    let cfg_air = MesherConfig {
        atlas_address_map: Some(atlas_arc.clone()),
        unknown_block_strategy: UnknownBlockStrategy::IgnoreAsAir,
        ..Default::default()
    };

    let mesh_air =
        SectionMesher::mesh_section(&padded_fb, &culler, |_| ModelSource::None, &cfg_air);
    assert_eq!(
        mesh_air.quad_count(),
        0,
        "IgnoreAsAir strategy must skip geometry for unknown blocks"
    );
}

#[test]
fn test_placeholder_with_custom_model_is_meshed_when_provided() {
    let mut address_map = AtlasAddressMap::default();
    let loc = AtlasSpriteLocation {
        chunk_id: 0,
        uv_bounds: [0.0, 0.0, 0.0625, 0.0625],
        frame_0_uv_bounds: [0.0, 0.0, 0.0625, 0.0625],
        ..Default::default()
    };
    address_map.sprites.insert(
        ResourceLocation::parse("minecraft:block/barrier").unwrap(),
        loc,
    );
    let atlas_arc = Arc::new(address_map);
    let culler = FaceCuller::default();

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(5, 5, 5, "minecraft:barrier", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let cfg = MesherConfig {
        atlas_address_map: Some(atlas_arc.clone()),
        ..Default::default()
    };

    // Case A: ModelSource::None -> placeholder is skipped (0 quads)
    let mesh_empty = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &cfg);
    assert_eq!(
        mesh_empty.quad_count(),
        0,
        "Barrier without model must produce 0 quads"
    );

    // Case B: ModelSource::Single(model) -> resource pack provided model for barrier; must be meshed!
    use mtk_core::Direction;
    let mut custom_model = BakedModel {
        block_state: "minecraft:barrier".to_string(),
        ..Default::default()
    };
    let mut faces_map = std::collections::HashMap::new();
    for dir in Direction::ALL {
        faces_map.insert(
            dir,
            mtk_model::BakedFace {
                direction: dir,
                texture: "minecraft:block/barrier".to_string(),
                uv_rot: 0.0,
                uv_bounds: [0.0, 0.0, 1.0, 1.0],
                tint_index: -1,
                cullface: Some(dir),
                vertices: [
                    glam::Vec3::new(0.0, 0.0, 0.0),
                    glam::Vec3::new(1.0, 0.0, 0.0),
                    glam::Vec3::new(1.0, 1.0, 0.0),
                    glam::Vec3::new(0.0, 1.0, 0.0),
                ],
                uvs: [
                    glam::Vec2::new(0.0, 0.0),
                    glam::Vec2::new(1.0, 0.0),
                    glam::Vec2::new(1.0, 1.0),
                    glam::Vec2::new(0.0, 1.0),
                ],
                normal: glam::Vec3::new(0.0, 0.0, -1.0),
                atlas_uvs: Some([
                    glam::Vec2::new(0.0, 0.0),
                    glam::Vec2::new(0.0625, 0.0),
                    glam::Vec2::new(0.0625, 0.0625),
                    glam::Vec2::new(0.0, 0.0625),
                ]),
                atlas_chunk_id: Some(0),
                atlas_texture_id: Some(0),
            },
        );
    }
    custom_model.elements = vec![mtk_model::BakedElement {
        from_pos: [0.0, 0.0, 0.0],
        to_pos: [16.0, 16.0, 16.0],
        faces: faces_map,
    }];
    custom_model.rebuild_face_buckets();
    let mesh_with_model = SectionMesher::mesh_section(
        &padded,
        &culler,
        |s| {
            if s == "minecraft:barrier" {
                ModelSource::Single(&custom_model)
            } else {
                ModelSource::None
            }
        },
        &cfg,
    );
    assert_eq!(
        mesh_with_model.quad_count(),
        6,
        "Barrier with custom model from resource pack must be meshed"
    );
}
