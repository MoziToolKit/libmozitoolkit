use glam::IVec3;
use mtk_cull::FaceCuller;
use mtk_resource::{CtmRule, CtmSolver, ResourceLocation};
use mtk_texture::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, SpriteKind};
use mtk_voxel::{MesherConfig, ModelSource, SectionMesher, VoxelStorage};
use std::sync::Arc;

mod common;

#[test]
fn test_ctm_horizontal_meshing_with_atlas() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    // Place 3 bookshelves in a horizontal line along X: (1, 1, 1), (2, 1, 1), (3, 1, 1)
    world.set_block(1, 1, 1, "minecraft:bookshelf", None);
    world.set_block(2, 1, 1, "minecraft:bookshelf", None);
    world.set_block(3, 1, 1, "minecraft:bookshelf", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();

    // Create CTM rule for horizontal connection
    let ctm_props = r#"
matchBlocks=minecraft:bookshelf
method=horizontal
tiles=0-3
"#;
    let rule =
        CtmRule::parse_properties("optifine/ctm/bookshelf.properties", "minecraft", ctm_props)
            .unwrap();
    let solver = Arc::new(CtmSolver::new(vec![rule]));

    // Create AtlasAddressMap with 4 tiles
    let mut atlas = AtlasAddressMap::new();
    atlas.chunks.push(AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 1024,
        height: 1024,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    for i in 0..4 {
        let loc = ResourceLocation::new("minecraft", format!("optifine/ctm/{}", i));
        let uv = [0.25 * i as f32, 0.0, 0.25 * (i + 1) as f32, 1.0];
        let rect = [256 * i, 0, 256, 256];
        atlas.sprites.insert(
            loc,
            AtlasSpriteLocation {
                chunk_id: 0,
                category: "blocks".to_string(),
                is_animated: false,
                sprite_kind: SpriteKind::StaticAtlas,
                texture_id: i,
                uv_bounds: uv,
                frame_0_uv_bounds: uv,
                local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                frame_uv_step: [0.0, 0.0],
                pixel_rect: rect,
                strip_pixel_rect: rect,
                frame_size: [256, 256],
                frame_count: 1,
                animation: None,
                has_normal: false,
                has_specular: false,
                has_overlay: false,
                average_color: None,
                slot_index: None,
                ..Default::default()
            },
        );
    }

    let config = MesherConfig {
        ctm_solver: Some(solver),
        atlas_address_map: Some(Arc::new(atlas)),
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    assert!(!mesh.positions.is_empty());
    assert_eq!(mesh.positions.len(), 16); // 2 connected cubes welded vertices
    assert_eq!(mesh.uvs.len(), 56); // 14 quads * 4 loop corner UVs
                                    // All face materials should be chunk_id = 0
    assert!(mesh.face_materials.iter().all(|&mat| mat == 0));
}

#[test]
fn test_ctm_full_47_tile_resolution() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    // Place a 3x3 plane of glass in XZ at Y=1
    for x in 1..=3 {
        for z in 1..=3 {
            world.set_block(x, 1, z, "minecraft:glass", None);
        }
    }

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();

    let ctm_props = r#"
matchBlocks=minecraft:glass
method=ctm
tiles=0-46
"#;
    let rule =
        CtmRule::parse_properties("optifine/ctm/glass.properties", "minecraft", ctm_props).unwrap();
    let solver = Arc::new(CtmSolver::new(vec![rule]));

    let config = MesherConfig {
        ctm_solver: Some(solver),
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);
    assert!(!mesh.positions.is_empty());
}

#[test]
fn test_ctm_standalone_meshing() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    // Place 3 bookshelves in a line along X: (1, 1, 1), (2, 1, 1), (3, 1, 1)
    world.set_block(1, 1, 1, "minecraft:bookshelf", None);
    world.set_block(2, 1, 1, "minecraft:bookshelf", None);
    world.set_block(3, 1, 1, "minecraft:bookshelf", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();

    let ctm_props = r#"
matchBlocks=minecraft:bookshelf
method=horizontal
tiles=0-3
"#;
    let rule =
        CtmRule::parse_properties("optifine/ctm/bookshelf.properties", "minecraft", ctm_props)
            .unwrap();
    let solver = Arc::new(CtmSolver::new(vec![rule]));

    // Standalone mode: atlas_address_map is None!
    let config = MesherConfig {
        ctm_solver: Some(solver),
        atlas_address_map: None,
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);
    assert!(!mesh.positions.is_empty());

    // Verify the canonical palette + indices pair resolves to optifine/ctm sub-tiles
    let keys = common::face_texture_keys(&mesh);
    assert!(!keys.is_empty());
    let has_ctm = keys.iter().any(|k| k.contains("optifine/ctm/"));
    assert!(
        has_ctm,
        "At least some faces must resolve to CTM sub-tiles, found: {:?}",
        keys
    );
}
