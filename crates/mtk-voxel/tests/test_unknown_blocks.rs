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
