mod common;

use glam::IVec3;
use mtk_cull::FaceCuller;
use mtk_voxel::mesher::shading::ModelSource;
use mtk_voxel::mesher::SectionMesher;
use mtk_voxel::storage::VoxelStorage;
use mtk_voxel::types::MesherConfig;

#[test]
fn test_solid_cube_culling_in_mesher() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // 2 adjacent solid cubes at (0,0,0) and (1,0,0)
    world.set_block(0, 0, 0, "minecraft:stone", None);
    world.set_block(1, 0, 0, "minecraft:stone", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    // Two touching cubes have 2 * 6 - 2 = 10 visible faces = 20 triangles
    assert_eq!(mesh.face_count(), 10);
    assert_eq!(mesh.triangle_count(), 20);
    assert_eq!(mesh.vertex_count(), 12); // Welded: 12 vertices

    let unwelded_config = MesherConfig {
        weld_vertices: false,
        ..Default::default()
    };
    let unwelded_mesh =
        SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &unwelded_config);
    assert_eq!(unwelded_mesh.vertex_count(), 40);
}

#[test]
fn test_fluid_and_solid_meshing() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    world.set_block(0, 0, 0, "minecraft:stone", None);
    world.set_block(0, 1, 0, "minecraft:water", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    // The water block on top of stone should generate visible fluid faces,
    // and the bottom of water against solid stone should be occluded!
    assert!(mesh.face_count() > 0);
}

#[cfg(feature = "parallel")]
#[test]
fn test_parallel_batch_mesher() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 32, 16, 16);

    world.set_block(0, 0, 0, "minecraft:oak_planks", None);
    world.set_block(16, 0, 0, "minecraft:stone", None);

    let p0 = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let p1 = world.get_section_padded_array(IVec3::new(1, 0, 0));

    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let results = SectionMesher::mesh_sections_parallel_with_source_and_progress(
        &[p0, p1],
        &culler,
        |_| mtk_voxel::mesher::shading::ModelSource::None,
        &config,
        Some(2),
        None,
    )
    .expect("Parallel meshing should succeed");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, IVec3::new(0, 0, 0));
    assert_eq!(results[1].0, IVec3::new(1, 0, 0));
    assert!(!results[0].1.is_empty());
    assert!(!results[1].1.is_empty());
}

#[test]
fn test_mesher_custom_attributes() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(0, 0, 0, "minecraft:stone", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);
    assert_eq!(mesh.face_count(), 6);

    let expected_attrs = [
        "mtk_source_textures",
        "mtk_source_texture_idx",
        "mtk_atlas_chunk_id",
        "mtk_uv_transform",
        "mtk_uv_rotation",
        "mtk_biome_tint_data",
        "mtk_biome_tint_color",
        "mtk_block_x",
        "mtk_block_y",
        "mtk_block_z",
    ];

    for attr_name in &expected_attrs {
        assert!(
            mesh.has_custom_attribute(attr_name),
            "Mesh should contain custom attribute: {}",
            attr_name
        );
    }

    let obsolete_attrs = [
        "mtk_material_slot",
        "mtk_atlas_texture_id",
        "mtk_uv_mode",
        "mtk_colormap_uv",
        "mtk_anim_total_frames",
    ];
    for attr_name in &obsolete_attrs {
        assert!(
            !mesh.has_custom_attribute(attr_name),
            "Mesh should NOT contain obsolete attribute: {}",
            attr_name
        );
    }

    // Regression gate: the mesher must never re-emit any attribute that the native
    // core declares retired (`mtk-core::attributes::RETIRED_MESH_ATTRIBUTES`).
    for retired in mtk_core::attributes::RETIRED_MESH_ATTRIBUTES {
        assert!(
            !mesh.has_custom_attribute(retired),
            "Mesher must never emit retired attribute: {}",
            retired
        );
    }
}

#[test]
fn test_grass_block_meshing_no_duplicate_overlay_faces() {
    use mtk_core::direction::Direction;
    use mtk_model::baker::{BakedElement, BakedFace, BakedModel};
    use std::collections::HashMap;
    use std::sync::Arc;

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(0, 0, 0, "minecraft:grass_block[snowy=false]", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();

    // Construct a simulated grass_block BakedModel with 2 elements:
    // Element 0: 6 canonical faces (down=dirt, up=grass_block_top, 4x grass_block_side)
    // Element 1: 4 overlay decal faces (4x grass_block_side_overlay)
    let mut el0_faces = HashMap::new();
    for dir in Direction::ALL {
        let tex = match dir {
            Direction::Down => "minecraft:block/dirt",
            Direction::Up => "minecraft:block/grass_block_top",
            _ => "minecraft:block/grass_block_side",
        };
        el0_faces.insert(
            dir,
            BakedFace {
                direction: dir,
                texture: tex.to_string(),
                tint_index: if dir == Direction::Up { 0 } else { -1 },
                cullface: Some(dir),
                vertices: [glam::Vec3::ZERO; 4],
                uvs: [glam::Vec2::ZERO; 4],
                ..Default::default()
            },
        );
    }

    let mut el1_faces = HashMap::new();
    for dir in [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ] {
        el1_faces.insert(
            dir,
            BakedFace {
                direction: dir,
                texture: "minecraft:block/grass_block_side_overlay".to_string(),
                tint_index: 0,
                cullface: Some(dir),
                vertices: [glam::Vec3::ZERO; 4],
                uvs: [glam::Vec2::ZERO; 4],
                ..Default::default()
            },
        );
    }

    let grass_model = Arc::new(BakedModel {
        block_state: "minecraft:grass_block[snowy=false]".to_string(),
        elements: vec![
            BakedElement {
                from_pos: [0.0, 0.0, 0.0],
                to_pos: [16.0, 16.0, 16.0],
                faces: el0_faces,
            },
            BakedElement {
                from_pos: [0.0, 0.0, 0.0],
                to_pos: [16.0, 16.0, 16.0],
                faces: el1_faces,
            },
        ],
        obj_faces: Vec::new(),
        faces: std::array::from_fn(|_| BakedFace::default()),
        is_cube: true,
        is_opaque: true,
        is_emissive: false,
        cull_meta: None,
        culled_faces: Default::default(),
        unculled_faces: Default::default(),
    });

    let config = MesherConfig {
        weld_vertices: false,
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |name| {
            if name.contains("grass_block") {
                ModelSource::from(Some(grass_model.clone()))
            } else {
                ModelSource::None
            }
        },
        &config,
    );

    // Exactly 6 cube faces should be emitted; the 4 overlay decal faces must be skipped!
    assert_eq!(
        mesh.face_count(),
        6,
        "Must emit exactly 6 faces, not 10 faces!"
    );
    assert_eq!(mesh.triangle_count(), 12);
    assert_eq!(mesh.vertex_count(), 24);

    // Verify tint data attributes
    let tint_data_attr = mesh
        .get_custom_attribute("mtk_biome_tint_data")
        .expect("Must have mtk_biome_tint_data");
    let tint_data = match &tint_data_attr.data {
        mtk_core::attributes::AttributeData::Float4(v) => v,
        _ => panic!("Expected Float4"),
    };
    assert_eq!(tint_data.len(), 6);

    // Face order is not part of the attribute contract; classify by tint payload.
    let up_faces = tint_data
        .iter()
        .filter(|td| td[0] == 1.0 && td[1] == 1.0 && td[2] == 1.0 && td[3] == 1.0)
        .count();
    let down_faces = tint_data.iter().filter(|td| td[2] == 0.0).count();
    let side_faces = tint_data
        .iter()
        .filter(|td| td[0] == 0.0 && td[1] == 1.0 && td[2] == 1.0 && td[3] == 1.0)
        .count();

    assert_eq!(up_faces, 1, "Exactly one grass top face must be tinted");
    assert_eq!(down_faces, 1, "Dirt bottom face must be untinted");
    assert_eq!(
        side_faces, 4,
        "All four side faces must tint only the overlay"
    );
}

#[test]
fn test_biome_transition_smoothing() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    // Left half (x in 0..8): plains, Right half (x in 8..16): desert
    for x in 0..16 {
        for z in 0..16 {
            let biome = if x < 8 {
                "minecraft:plains"
            } else {
                "minecraft:desert"
            };
            world.set_block(x, 0, z, "minecraft:grass_block", Some(biome));
        }
    }

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    assert!(
        padded.biome_data.is_some(),
        "Padded array must contain smoothed biome data"
    );

    let biome_cols = padded.biome_data.as_ref().unwrap();
    assert_eq!(biome_cols.len(), 256);

    let plains_pal = mtk_material::get_biome_palette("plains");
    let desert_pal = mtk_material::get_biome_palette("desert");

    // Deep in plains (x = 0, z = 8)
    let col_plains = &biome_cols[8];
    // Deep in desert (x = 15, z = 8)
    let col_desert = &biome_cols[15 * 16 + 8];
    // On the transition boundary (x = 7, z = 8 and x = 8, z = 8)
    let col_trans_7 = &biome_cols[7 * 16 + 8];
    let col_trans_8 = &biome_cols[8 * 16 + 8];

    // Colormap UVs must smoothly transition
    let _plains_uv = plains_pal.colormap_uv();
    let _desert_uv = desert_pal.colormap_uv();

    assert!(col_plains.colormap_uv[0] > col_desert.colormap_uv[0]);
    assert!(col_trans_7.colormap_uv[0] <= col_plains.colormap_uv[0] + 1e-4);
    assert!(col_trans_7.colormap_uv[0] >= col_trans_8.colormap_uv[0] - 1e-4);
    assert!(col_trans_8.colormap_uv[0] >= col_desert.colormap_uv[0] - 1e-4);

    // Mesh the section and verify mtk_biome_tint_color
    let culler = FaceCuller::default();
    let config = MesherConfig::default();
    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    assert!(
        mesh.get_custom_attribute("mtk_colormap_uv").is_none(),
        "mtk_colormap_uv must be retired"
    );

    let color_attr = mesh
        .get_custom_attribute("mtk_biome_tint_color")
        .expect("Must have mtk_biome_tint_color");
    let colors = match &color_attr.data {
        mtk_core::attributes::AttributeData::Float4(v) => v,
        _ => panic!("Expected Float4"),
    };
    assert!(!colors.is_empty());

    // Verify there are multiple distinct intermediate values in mtk_biome_tint_color across the transition!
    let mut distinct_col = std::collections::HashSet::new();
    for col in colors {
        distinct_col.insert((
            (col[0] * 100.0).round() as i32,
            (col[1] * 100.0).round() as i32,
        ));
    }
    assert!(
        distinct_col.len() > 2,
        "Transition zone must produce continuous blended gradient values, not a binary step!"
    );
}

#[test]
fn test_selection_boundary_no_color_bleeding() {
    let mut world = VoxelStorage::new();
    // Selection with non-zero world coordinates
    let min_x = 100;
    let min_y = 64;
    let min_z = 200;
    let size_x = 16;
    let size_y = 16;
    let size_z = 16;

    world.set_bounds(min_x, min_y, min_z, size_x, size_y, size_z);

    // Populate every block in the selection with desert biome
    for x in 0..16 {
        for z in 0..16 {
            world.set_block(
                min_x + x,
                min_y,
                min_z + z,
                "minecraft:grass_block",
                Some("minecraft:desert"),
            );
        }
    }

    let sec_coord = IVec3::new(min_x >> 4, min_y >> 4, min_z >> 4);
    let padded = world.get_section_padded_array(sec_coord);
    assert!(padded.biome_data.is_some());

    let biome_cols = padded.biome_data.as_ref().unwrap();
    let desert_pal = mtk_material::get_biome_palette("desert");
    let desert_uv = desert_pal.colormap_uv();
    let desert_grass = desert_pal.grass_linear();

    // Verify all 256 columns, especially boundary columns, have ZERO bleeding into default Plains
    for lx in 0..16 {
        for lz in 0..16 {
            let col = &biome_cols[lx * 16 + lz];
            assert!(
                (col.colormap_uv[0] - desert_uv[0]).abs() < 1e-4,
                "Col ({}, {}) colormap_uv[0] was {}, expected desert {}",
                lx,
                lz,
                col.colormap_uv[0],
                desert_uv[0]
            );
            assert!(
                (col.colormap_uv[1] - desert_uv[1]).abs() < 1e-4,
                "Col ({}, {}) colormap_uv[1] was {}, expected desert {}",
                lx,
                lz,
                col.colormap_uv[1],
                desert_uv[1]
            );
            assert!(
                (col.grass_color[0] - desert_grass[0]).abs() < 1e-4,
                "Col ({}, {}) grass_color was contaminated by bleeding",
                lx,
                lz
            );
        }
    }
}

#[test]
fn test_waterlogged_isolated_block_meshing() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // An isolated kelp block in air at (2, 2, 2) (canonical vanilla state without explicit waterlogged)
    world.set_block(2, 2, 2, "minecraft:kelp[age=0]", None);
    // An isolated waterlogged slab in air at (5, 5, 5)
    world.set_block(
        5,
        5,
        5,
        "minecraft:oak_slab[type=bottom,waterlogged=true]",
        None,
    );

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    // Both isolated kelp and isolated waterlogged slab MUST emit water geometry
    // into the mesh even without pre-baked element models!
    // Specifically, water top, bottom, and side faces should be emitted in open air.
    assert!(mesh.face_count() > 0);
    // 2 isolated waterlogged blocks in air with fallback unit cube:
    // - kelp: 6 fluid faces + 6 solid faces (its own occlusion shape is empty)
    // - bottom slab: 5 fluid faces (the fluid bottom is coplanar with the slab bottom and
    //   is self-occluded by vanilla `isFaceOccludedBySelf`) + 6 solid faces
    // Total: 11 + 12 = 23.
    assert_eq!(mesh.face_count(), 23);
}

#[test]
fn test_waterlogged_isolated_block_with_baked_model() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // An isolated waterlogged slab in air at (5, 5, 5)
    world.set_block(
        5,
        5,
        5,
        "minecraft:oak_slab[type=bottom,waterlogged=true]",
        None,
    );

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    // Create a mock baked model for the slab (without waterlogged in its block_state)
    let face = mtk_model::baker::BakedFace {
        direction: mtk_core::Direction::Up,
        vertices: [
            glam::Vec3::new(0.0, 0.5, 0.0),
            glam::Vec3::new(0.0, 0.5, 1.0),
            glam::Vec3::new(1.0, 0.5, 1.0),
            glam::Vec3::new(1.0, 0.5, 0.0),
        ],
        texture: "minecraft:block/oak_planks".to_string(),
        ..Default::default()
    };
    let mut faces_map = std::collections::HashMap::new();
    faces_map.insert(mtk_core::Direction::Up, face);
    let mut slab_model = mtk_model::baker::BakedModel {
        block_state: "minecraft:oak_slab[type=bottom]".to_string(),
        elements: vec![mtk_model::baker::BakedElement {
            from_pos: [0.0, 0.0, 0.0],
            to_pos: [16.0, 8.0, 16.0],
            faces: faces_map,
        }],
        obj_faces: Vec::new(),
        faces: std::array::from_fn(|_| mtk_model::baker::BakedFace::default()),
        is_cube: false,
        is_opaque: false,
        is_emissive: false,
        cull_meta: None,
        culled_faces: Default::default(),
        unculled_faces: Vec::new(),
    };
    slab_model.rebuild_face_buckets();
    let slab_arc = std::sync::Arc::new(slab_model);

    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |state| {
            if state.contains("oak_slab") {
                ModelSource::from(Some(slab_arc.clone()))
            } else {
                ModelSource::None
            }
        },
        &config,
    );

    // 1 solid model face + 5 fluid faces. The fluid bottom face is coplanar with the
    // waterlogged bottom slab and is self-occluded like vanilla `FluidRenderer`.
    assert_eq!(mesh.face_count(), 6);
}

#[test]
fn test_alternate_blocks_variant_sampling() {
    use mtk_model::baker::baked_model::{BakedElement, BakedFace, BakedModel, BakedVariantGroup};
    use mtk_voxel::mesher::ModelSource;
    use std::sync::Arc;

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // Place 16 dirt blocks along a row
    for x in 0..16 {
        world.set_block(x, 0, 0, "minecraft:dirt", None);
    }

    // Create 4 distinct variants of a unit cube model, distinguished by texture
    let mut variants = Vec::new();
    for i in 0..4 {
        let mut faces_map = std::collections::HashMap::new();
        faces_map.insert(
            mtk_core::Direction::Up,
            BakedFace {
                direction: mtk_core::Direction::Up,
                cullface: Some(mtk_core::Direction::Up),
                texture: format!("minecraft:block/dirt_var_{}", i),
                vertices: [
                    glam::Vec3::new(0.0, 1.0, 0.0),
                    glam::Vec3::new(0.0, 1.0, 1.0),
                    glam::Vec3::new(1.0, 1.0, 1.0),
                    glam::Vec3::new(1.0, 1.0, 0.0),
                ],
                uvs: [
                    glam::Vec2::new(0.0, 0.0),
                    glam::Vec2::new(0.0, 1.0),
                    glam::Vec2::new(1.0, 1.0),
                    glam::Vec2::new(1.0, 0.0),
                ],
                ..Default::default()
            },
        );
        let mut m = BakedModel {
            block_state: "minecraft:dirt".to_string(),
            elements: vec![BakedElement {
                from_pos: [0.0, 0.0, 0.0],
                to_pos: [16.0, 16.0, 16.0],
                faces: faces_map,
            }],
            obj_faces: Vec::new(),
            faces: std::array::from_fn(|_| BakedFace::default()),
            is_cube: true,
            is_opaque: true,
            is_emissive: false,
            cull_meta: None,
            culled_faces: Default::default(),
            unculled_faces: Vec::new(),
        };
        m.rebuild_face_buckets();
        variants.push(m);
    }

    let group = Arc::new(BakedVariantGroup::new(
        "minecraft:dirt".to_string(),
        variants,
        vec![1, 1, 1, 1],
    ));

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig {
        origin_centered: false,
        weld_vertices: false,
        enable_alternate_blocks: true,
        ..Default::default()
    };

    let mesh =
        SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::Variant(&group), &config);

    let sampled_textures: std::collections::HashSet<String> =
        common::face_texture_keys(&mesh).into_iter().collect();

    // Out of 16 blocks, multiple variants must have been sampled (not just 1 static variant!)
    assert!(
        sampled_textures.len() > 1,
        "Expected multiple variants to be sampled across 16 blocks, got {:?}",
        sampled_textures
    );

    // When enable_alternate_blocks is FALSE, only primary variant (dirt_var_0) must be sampled
    let disabled_config = MesherConfig {
        origin_centered: false,
        weld_vertices: false,
        enable_alternate_blocks: false,
        ..Default::default()
    };
    let mesh_disabled = SectionMesher::mesh_section(
        &padded,
        &culler,
        |_| ModelSource::Variant(&group),
        &disabled_config,
    );
    let disabled_textures: std::collections::HashSet<String> =
        common::face_texture_keys(&mesh_disabled)
            .into_iter()
            .collect();
    assert_eq!(disabled_textures.len(), 1);
    assert!(disabled_textures.contains("minecraft:block/dirt_var_0"));
}

#[test]
fn test_plant_position_offsets() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // Place a flower at (3, 5, 7) and a vertical duplicate at (3, 6, 7)
    world.set_block(3, 5, 7, "minecraft:poppy", None);
    world.set_block(3, 6, 7, "minecraft:poppy", None);
    world.set_block(8, 5, 2, "minecraft:poppy", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();

    // 1. With plant offsets enabled
    let config_enabled = MesherConfig {
        origin_centered: false,
        weld_vertices: false,
        enable_random_offsets: true,
        ..Default::default()
    };
    let mesh_enabled =
        SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config_enabled);

    // Poppy at (3, 5, 7) and (3, 6, 7):
    let offset_3_7 = mtk_core::random::get_block_offset(mtk_core::random::OffsetType::XZ, 3, 5, 7);
    assert!(offset_3_7.x.abs() > 0.001 || offset_3_7.z.abs() > 0.001);
    assert!(offset_3_7.x >= -0.25 && offset_3_7.x <= 0.25);
    assert!(offset_3_7.z >= -0.25 && offset_3_7.z <= 0.25);
    assert_eq!(offset_3_7.y, 0.0);

    // Check that vertices at (3, 5, 7) are shifted by offset_3_7
    let mut found_shifted = false;
    for pos in &mesh_enabled.positions {
        if (pos[1] - 5.0).abs() < 1.01 {
            let frac_x = pos[0] - 3.0;
            if (frac_x - (0.0 + offset_3_7.x)).abs() < 1e-4
                || (frac_x - (1.0 + offset_3_7.x)).abs() < 1e-4
            {
                found_shifted = true;
                break;
            }
        }
    }
    assert!(
        found_shifted,
        "Plant vertices should be shifted by offset_3_7"
    );

    // 2. With plant offsets disabled
    let config_disabled = MesherConfig {
        origin_centered: false,
        weld_vertices: false,
        enable_random_offsets: false,
        ..Default::default()
    };
    let mesh_disabled =
        SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config_disabled);

    // All vertex fractional coordinates must be exact integers (0.0 or 1.0 relative to block pos)
    for pos in &mesh_disabled.positions {
        let fx = (pos[0] - pos[0].round()).abs();
        let fz = (pos[2] - pos[2].round()).abs();
        assert!(
            fx < 1e-4,
            "Expected integer X coordinates without offset, got {}",
            pos[0]
        );
        assert!(
            fz < 1e-4,
            "Expected integer Z coordinates without offset, got {}",
            pos[2]
        );
    }
}

#[test]
fn test_submerged_seagrass_and_kelp_seamless_meshing() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // Seabed: dirt at (1, 0, 1)
    world.set_block(1, 0, 1, "minecraft:dirt", None);
    // Seagrass at (1, 1, 1) (vanilla state string without waterlogged=true)
    world.set_block(1, 1, 1, "minecraft:seagrass", None);
    // Water above seagrass at (1, 2, 1)
    world.set_block(1, 2, 1, "minecraft:water[level=0]", None);
    // Water adjacent to seagrass at (1, 1, 2)
    world.set_block(1, 1, 2, "minecraft:water[level=0]", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig {
        origin_centered: false,
        weld_vertices: false,
        mesh_fluids: true,
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    // Seagrass at (1, 1, 1) must emit water geometry merged with water above and adjacent.
    // The mutual face between seagrass (1, 1, 1) and water above (1, 2, 1) must be CULLED.
    // The mutual face between seagrass (1, 1, 1) and water adjacent (1, 1, 2) must be CULLED.
    assert!(mesh.face_count() > 0);

    // Verify there is no horizontal water face at y=2.0 between seagrass and water above
    for (i, norm) in mesh.normals.iter().enumerate().step_by(4) {
        let p0 = mesh.positions[mesh.indices[i / 4 * 6] as usize];
        // If normal is (0, 1, 0) or (0, -1, 0) and at x=1, z=1
        if (p0[0] - 1.0).abs() < 0.1 && (p0[2] - 1.0).abs() < 0.1 {
            // Internal horizontal boundary at y=2.0 between (1,1,1) and (1,2,1) MUST NOT exist
            assert!(
                !((p0[1] - 2.0).abs() < 1e-3 && norm[1].abs() > 0.9),
                "Internal dividing water face at y=2.0 between seagrass and water above must be culled!"
            );
        }
    }
}

#[test]
fn test_3d_caves_and_cliffs_biome_tinting() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    let mut sec = mtk_voxel::storage::SectionStorage::new(IVec3::new(0, 0, 0));
    let mut biomes = mtk_voxel::storage::SectionBiomes::uniform("minecraft:lush_caves");
    for qx in 0..4 {
        for qz in 0..4 {
            for qy in 2..4 {
                biomes.set_quart_biome(qx, qy, qz, "minecraft:dark_forest");
            }
        }
    }
    assert!(biomes.varies_vertically());
    sec.biomes = Some(biomes);

    sec.set_local(4, 1, 4, "minecraft:grass_block");
    sec.set_local(4, 12, 4, "minecraft:grass_block");
    world.sections.insert(IVec3::new(0, 0, 0), sec);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    assert!(padded.biome_data.is_some());
    let biome_cols = padded.biome_data.as_ref().unwrap();
    assert_eq!(
        biome_cols.len(),
        1024,
        "3D biome section must generate 1024 quart entries"
    );

    let bottom_idx = (4 << 4) | 4;
    let top_idx = (3 << 8) | (4 << 4) | 4;
    let bottom_col = &biome_cols[bottom_idx];
    let top_col = &biome_cols[top_idx];

    assert_ne!(bottom_col.grass_color, top_col.grass_color);

    let culler = FaceCuller::default();
    let config = MesherConfig::default();
    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);
    assert!(mesh.face_count() > 0);
}

#[test]
fn test_cache_driven_animated_texture_routing_block_models() {
    use mtk_core::direction::Direction;
    use mtk_model::baker::baked_model::{BakedFace, BakedModel};
    use mtk_texture::atlas::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation};
    use std::sync::Arc;

    // 1. Construct AtlasAddressMap with static chunk (0) and animated chunk (1)
    let mut address_map = AtlasAddressMap::new();
    address_map.chunks.push(AtlasChunkMeta {
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
    address_map.chunks.push(AtlasChunkMeta {
        chunk_id: 1,
        category: "blocks".to_string(),
        is_animated: true,
        category_chunk_index: 1,
        width: 1024,
        height: 4096,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    let anim_textures = [
        "minecraft:block/water_still",
        "minecraft:block/water_flow",
        "minecraft:block/kelp",
        "minecraft:block/nether_portal",
        "minecraft:block/magma",
        "minecraft:block/command_block_front",
        "minecraft:block/fire_0",
    ];

    for &tex in &anim_textures {
        let loc = mtk_resource::ResourceLocation::parse(tex).unwrap();
        // Insert static Frame 0 sprite into static map
        address_map.sprites.insert(
            loc.clone(),
            AtlasSpriteLocation {
                category: "blocks".to_string(),
                sprite_kind: mtk_texture::atlas::SpriteKind::StaticAtlas,
                chunk_id: 0,
                texture_id: 10,
                is_animated: false,
                frame_0_uv_bounds: [0.0, 0.0, 0.1, 0.1],
                local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                uv_bounds: [0.0, 0.0, 0.1, 0.1],
                frame_uv_step: [0.0, 0.0],
                pixel_rect: [0, 0, 16, 16],
                strip_pixel_rect: [0, 0, 16, 16],
                frame_size: [16, 16],
                frame_count: 1,
                animation: None,
                has_normal: false,
                has_specular: false,
                has_overlay: false,
                average_color: None,
            },
        );
        // Insert vertical strip sprite into animated cache
        address_map.anim_sprites.insert(
            loc.clone(),
            AtlasSpriteLocation {
                category: "blocks".to_string(),
                sprite_kind: mtk_texture::atlas::SpriteKind::AnimatedAtlas,
                chunk_id: 1, // Animated Chunk ID
                texture_id: 20,
                is_animated: true,
                frame_0_uv_bounds: [0.0, 0.0, 0.1, 0.025],
                local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                uv_bounds: [0.0, 0.0, 0.1, 0.1],
                frame_uv_step: [0.0, 0.025],
                pixel_rect: [0, 0, 16, 16],
                strip_pixel_rect: [0, 0, 16, 64],
                frame_size: [16, 16],
                frame_count: 4,
                animation: None,
                has_normal: false,
                has_specular: false,
                has_overlay: false,
                average_color: None,
            },
        );
    }

    // Static only sprite: stone
    let stone_loc = mtk_resource::ResourceLocation::parse("minecraft:block/stone").unwrap();
    address_map.sprites.insert(
        stone_loc.clone(),
        AtlasSpriteLocation {
            category: "blocks".to_string(),
            sprite_kind: mtk_texture::atlas::SpriteKind::StaticAtlas,
            chunk_id: 0,
            texture_id: 30,
            is_animated: false,
            frame_0_uv_bounds: [0.2, 0.2, 0.3, 0.3],
            local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            uv_bounds: [0.2, 0.2, 0.3, 0.3],
            frame_uv_step: [0.0, 0.0],
            pixel_rect: [32, 32, 16, 16],
            strip_pixel_rect: [32, 32, 16, 16],
            frame_size: [16, 16],
            frame_count: 1,
            animation: None,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            average_color: None,
        },
    );

    let atlas_arc = Arc::new(address_map);

    // 2. Build mock baked models with pre-baked static atlas_uvs
    let create_mock_model = |block_name: &str, tex_name: &str| -> Arc<BakedModel> {
        let mut model = BakedModel {
            block_state: block_name.to_string(),
            ..Default::default()
        };
        let face = BakedFace {
            direction: Direction::Up,
            texture: tex_name.to_string(),
            uv_rot: 0.0,
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            tint_index: -1,
            cullface: None,
            vertices: [
                glam::Vec3::new(0.0, 1.0, 0.0),
                glam::Vec3::new(1.0, 1.0, 0.0),
                glam::Vec3::new(1.0, 1.0, 1.0),
                glam::Vec3::new(0.0, 1.0, 1.0),
            ],
            uvs: [
                glam::Vec2::new(0.0, 0.0),
                glam::Vec2::new(1.0, 0.0),
                glam::Vec2::new(1.0, 1.0),
                glam::Vec2::new(0.0, 1.0),
            ],
            normal: glam::Vec3::new(0.0, 1.0, 0.0),
            // Pre-baked static UVs (pointing to chunk 0 in cache)
            atlas_uvs: Some([
                glam::Vec2::new(0.0, 0.0),
                glam::Vec2::new(0.1, 0.0),
                glam::Vec2::new(0.1, 0.1),
                glam::Vec2::new(0.0, 0.1),
            ]),
            atlas_chunk_id: Some(0),
            atlas_texture_id: Some(10),
        };
        let mut faces_map = std::collections::HashMap::new();
        faces_map.insert(Direction::Up, face);
        model.elements = vec![mtk_model::baker::BakedElement {
            from_pos: [0.0, 0.0, 0.0],
            to_pos: [16.0, 16.0, 16.0],
            faces: faces_map,
        }];
        model.rebuild_face_buckets();
        Arc::new(model)
    };

    let model_kelp = create_mock_model("minecraft:kelp", "minecraft:block/kelp");
    let model_portal =
        create_mock_model("minecraft:nether_portal", "minecraft:block/nether_portal");
    let model_magma = create_mock_model("minecraft:magma_block", "block/magma");
    let model_cmd = create_mock_model(
        "minecraft:command_block",
        "minecraft:block/command_block_front",
    );
    let model_fire = create_mock_model("minecraft:fire", "minecraft:block/fire_0");
    let model_stone = create_mock_model("minecraft:stone", "minecraft:block/stone");

    // 3. Populate VoxelStorage
    let mut storage = VoxelStorage::new();
    storage.set_bounds(0, 0, 0, 16, 16, 16);
    storage.set_block(1, 1, 1, "minecraft:kelp", None);
    storage.set_block(2, 1, 1, "minecraft:nether_portal", None);
    storage.set_block(3, 1, 1, "minecraft:magma_block", None);
    storage.set_block(4, 1, 1, "minecraft:command_block", None);
    storage.set_block(5, 1, 1, "minecraft:fire", None);
    storage.set_block(6, 1, 1, "minecraft:stone", None);

    let padded = storage.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();

    let model_resolver = |state: &str| -> ModelSource<'_> {
        if state.contains("kelp") {
            ModelSource::from(Some(model_kelp.clone()))
        } else if state.contains("nether_portal") {
            ModelSource::from(Some(model_portal.clone()))
        } else if state.contains("magma") {
            ModelSource::from(Some(model_magma.clone()))
        } else if state.contains("command_block") {
            ModelSource::from(Some(model_cmd.clone()))
        } else if state.contains("fire") {
            ModelSource::from(Some(model_fire.clone()))
        } else if state.contains("stone") {
            ModelSource::from(Some(model_stone.clone()))
        } else {
            ModelSource::None
        }
    };

    // --- TEST CASE A: enable_animated_textures = true ---
    let config_anim = MesherConfig {
        atlas_address_map: Some(atlas_arc.clone()),
        enable_animated_textures: true,
        ..Default::default()
    };
    let mesh_anim = SectionMesher::mesh_section(&padded, &culler, model_resolver, &config_anim);

    // Extract chunk_id for each emitted face from mesh_anim.custom_attributes
    let face_chunk_attr = mesh_anim
        .custom_attributes
        .get("mtk_atlas_chunk_id")
        .expect("mtk_atlas_chunk_id attr must exist");
    let chunk_ids = match &face_chunk_attr.data {
        mtk_core::attributes::AttributeData::Int32(ids) => ids,
        _ => panic!("Expected Int32 attribute data"),
    };

    let tex_keys = common::face_texture_keys(&mesh_anim);

    assert_eq!(chunk_ids.len(), tex_keys.len());
    assert!(!chunk_ids.is_empty(), "Faces should be emitted");

    for (tex, &cid) in tex_keys.iter().zip(chunk_ids.iter()) {
        if anim_textures
            .iter()
            .any(|&a| tex == a || tex.ends_with(a) || a.ends_with(tex.as_str()))
        {
            assert_eq!(cid, 1, "Texture {} must route to animated chunk 1", tex);
        } else if tex.contains("stone") {
            assert_eq!(cid, 0, "Stone texture {} must route to static chunk 0", tex);
        }
    }

    // --- TEST CASE B: enable_animated_textures = false (Manual Gate) ---
    let config_static = MesherConfig {
        atlas_address_map: Some(atlas_arc.clone()),
        enable_animated_textures: false,
        ..Default::default()
    };
    let mesh_static = SectionMesher::mesh_section(&padded, &culler, model_resolver, &config_static);
    let static_chunk_attr = mesh_static
        .custom_attributes
        .get("mtk_atlas_chunk_id")
        .expect("mtk_atlas_chunk_id attr must exist");
    let static_chunk_ids = match &static_chunk_attr.data {
        mtk_core::attributes::AttributeData::Int32(ids) => ids,
        _ => panic!("Expected Int32 attribute data"),
    };

    for (idx, &cid) in static_chunk_ids.iter().enumerate() {
        assert_eq!(
            cid, 0,
            "Face index {} must fallback to static chunk 0 when enable_animated_textures is false",
            idx
        );
    }
}

#[test]
fn test_dirt_path_mesh_ao_neighbor_shading() {
    use glam::{Vec2, Vec3};
    use mtk_core::direction::Direction;
    use mtk_model::baker::{BakedElement, BakedFace, BakedModel};
    use std::collections::HashMap;

    // Build a mock dirt_path model with Y = 15/16 height
    let mut faces = HashMap::new();
    let top_y = 15.0 / 16.0;
    faces.insert(
        Direction::Up,
        BakedFace {
            direction: Direction::Up,
            texture: "minecraft:block/dirt_path_top".to_string(),
            vertices: [
                Vec3::new(0.0, top_y, 0.0), // v0 (-X, -Z)
                Vec3::new(0.0, top_y, 1.0), // v1 (-X, +Z)
                Vec3::new(1.0, top_y, 1.0), // v2 (+X, +Z)
                Vec3::new(1.0, top_y, 0.0), // v3 (+X, -Z)
            ],
            uvs: [
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 0.0),
            ],
            normal: Vec3::Y,
            ..Default::default()
        },
    );

    let mut model_dirt_path = BakedModel {
        block_state: "minecraft:dirt_path".to_string(),
        elements: vec![BakedElement {
            from_pos: [0.0, 0.0, 0.0],
            to_pos: [16.0, 15.0, 16.0],
            faces,
        }],
        culled_faces: Default::default(),
        unculled_faces: Vec::new(),
        ..Default::default()
    };
    model_dirt_path.rebuild_face_buckets();

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // Place dirt_path at (2, 2, 2), and solid stone adjacent to it on the East side (3, 2, 2)
    world.set_block(2, 2, 2, "minecraft:dirt_path", None);
    world.set_block(3, 2, 2, "minecraft:stone", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig {
        enable_ao: true,
        weld_vertices: false,
        ..Default::default()
    };

    let model_dirt_path_arc = std::sync::Arc::new(model_dirt_path);
    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |st| {
            if st.contains("dirt_path") {
                ModelSource::from(Some(model_dirt_path_arc.clone()))
            } else {
                ModelSource::None
            }
        },
        &config,
    );

    let ao_attr = mesh
        .get_custom_attribute(mtk_core::attributes::constants::ATTR_AO)
        .expect("mtk_ao must be emitted when enable_ao is true");
    let ao_values = match &ao_attr.data {
        mtk_core::attributes::AttributeData::Float(v) => v,
        other => panic!("Expected Float for mtk_ao, got {:?}", other),
    };
    assert!(!mesh.positions.is_empty());

    // Find the dirt_path top face at Y = 2.0 + 15/16 = 2.9375
    let mut found_top_face = false;
    for quad_idx in 0..mesh.positions.len() / 4 {
        let base = quad_idx * 4;
        let v0 = mesh.positions[base];
        let v1 = mesh.positions[base + 1];
        let v2 = mesh.positions[base + 2];
        let v3 = mesh.positions[base + 3];

        // Check if this is the dirt path top face at Y ~ 2.9375
        if (v0[1] - (2.0 + top_y)).abs() < 1e-3
            && (v1[1] - (2.0 + top_y)).abs() < 1e-3
            && (v2[1] - (2.0 + top_y)).abs() < 1e-3
            && (v3[1] - (2.0 + top_y)).abs() < 1e-3
        {
            found_top_face = true;
            let c0 = ao_values[base];
            let c1 = ao_values[base + 1];
            let c2 = ao_values[base + 2];
            let c3 = ao_values[base + 3];

            // v0 (-X, -Z) and v1 (-X, +Z) are away from East neighbor -> brightness 1.0
            let exp_full = 1.0;
            // v2 (+X, +Z) and v3 (+X, -Z) are adjacent to East solid stone -> brightness 0.8 (Minecraft BlockModelLighter)
            let exp_shaded = 0.8;
            assert!(
                (c0 - exp_full).abs() < 1e-3,
                "v0 should have full brightness {}, got {}",
                exp_full,
                c0
            );
            assert!(
                (c1 - exp_full).abs() < 1e-3,
                "v1 should have full brightness {}, got {}",
                exp_full,
                c1
            );

            // v2 (+X, +Z) and v3 (+X, -Z) are adjacent to East solid stone -> brightness 0.8
            assert!(
                (c2 - exp_shaded).abs() < 1e-3,
                "v2 should have shaded brightness {}, got {}",
                exp_shaded,
                c2
            );
            assert!(
                (c3 - exp_shaded).abs() < 1e-3,
                "v3 should have shaded brightness {}, got {}",
                exp_shaded,
                c3
            );
        }
    }

    assert!(found_top_face, "Dirt path top face must be emitted");
}

#[test]
fn test_dirt_path_adjacent_no_occlusion() {
    use glam::{Vec2, Vec3};
    use mtk_core::direction::Direction;
    use mtk_model::baker::{BakedElement, BakedFace, BakedModel};
    use std::collections::HashMap;

    let mut faces = HashMap::new();
    let top_y = 15.0 / 16.0;
    faces.insert(
        Direction::Up,
        BakedFace {
            direction: Direction::Up,
            texture: "minecraft:block/dirt_path_top".to_string(),
            vertices: [
                Vec3::new(0.0, top_y, 0.0),
                Vec3::new(0.0, top_y, 1.0),
                Vec3::new(1.0, top_y, 1.0),
                Vec3::new(1.0, top_y, 0.0),
            ],
            uvs: [
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 0.0),
            ],
            normal: Vec3::Y,
            ..Default::default()
        },
    );

    let mut model_dirt_path = BakedModel {
        block_state: "minecraft:dirt_path".to_string(),
        elements: vec![BakedElement {
            from_pos: [0.0, 0.0, 0.0],
            to_pos: [16.0, 15.0, 16.0],
            faces,
        }],
        culled_faces: Default::default(),
        unculled_faces: Vec::new(),
        ..Default::default()
    };
    model_dirt_path.rebuild_face_buckets();

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // Place 2 adjacent dirt_path blocks at (2, 2, 2) and (3, 2, 2)
    world.set_block(2, 2, 2, "minecraft:dirt_path", None);
    world.set_block(3, 2, 2, "minecraft:dirt_path", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig {
        enable_ao: true,
        weld_vertices: false,
        ..Default::default()
    };

    let model_dirt_path_arc = std::sync::Arc::new(model_dirt_path);
    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |st| {
            if st.contains("dirt_path") {
                ModelSource::from(Some(model_dirt_path_arc.clone()))
            } else {
                ModelSource::None
            }
        },
        &config,
    );

    let ao_attr = mesh
        .get_custom_attribute(mtk_core::attributes::constants::ATTR_AO)
        .expect("mtk_ao must be emitted when enable_ao is true");
    let ao_values = match &ao_attr.data {
        mtk_core::attributes::AttributeData::Float(v) => v,
        other => panic!("Expected Float for mtk_ao, got {:?}", other),
    };

    // Every emitted vertex on dirt_path top faces must be 1.0 (unoccluded by its path neighbor!)
    for (quad_idx, &ao_val) in ao_values.iter().enumerate() {
        assert!(
            (ao_val - 1.0).abs() < 1e-3,
            "Adjacent dirt_path vertex {} must have 1.0 brightness without black border, got {}",
            quad_idx,
            ao_val
        );
    }
}

#[test]
fn test_decorated_pot_block_entity_meshing() {
    let mut sec = mtk_voxel::storage::SectionStorage::new(IVec3::new(0, 0, 0));
    let local_idx = mtk_voxel::storage::block_index(2, 2, 2) as u16;

    // Register decorated pot in palette
    let pal_idx = sec.palette.len() as u16;
    sec.palette
        .push("minecraft:decorated_pot[facing=north]".to_string());
    sec.voxels[local_idx as usize] = pal_idx;
    sec.non_air_count = 1;

    // Attach BlockEntityData
    let pot_entity = mtk_voxel::BlockEntityData::DecoratedPot {
        sherds: [
            Some("minecraft:angler_pottery_sherd".to_string()),
            None,
            Some("miner".to_string()),
            Some("blade_pottery_sherd".to_string()),
        ],
    };
    sec.set_block_entity(local_idx, Some(pot_entity));

    let padded = sec.build_padded_array(|_, _, _| "minecraft:air");
    assert!(padded.block_entities.contains_key(&local_idx));

    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    assert!(
        mesh.face_count() >= 16,
        "Decorated pot must have all faces emitted (got {})",
        mesh.face_count()
    );

    // Verify positions are centered around (2, 2, 2)
    for pos in &mesh.positions {
        assert!(pos[0] >= 2.0 && pos[0] <= 3.0);
        assert!(pos[1] >= 2.0 && pos[1] <= 3.3); // Rim goes slightly above Y=3.0 (up to 20/16 = 1.25)
        assert!(pos[2] >= 2.0 && pos[2] <= 3.0);
    }
}

#[test]
fn test_generic_block_entity_meshing_skull_and_pot() {
    let mut sec = mtk_voxel::storage::SectionStorage::new(IVec3::new(0, 0, 0));

    // 1. Decorated Pot at (2, 2, 2)
    let pot_idx = mtk_voxel::storage::block_index(2, 2, 2) as u16;
    let pot_pal = sec.palette.len() as u16;
    sec.palette
        .push("minecraft:decorated_pot[facing=north]".to_string());
    sec.voxels[pot_idx as usize] = pot_pal;

    let pot_entity = mtk_voxel::BlockEntityData::new_decorated_pot([
        Some("minecraft:angler_pottery_sherd"),
        None,
        Some("miner"),
        Some("blade"),
    ]);
    sec.set_block_entity(pot_idx, Some(pot_entity));

    // 2. Player Skull at (5, 5, 5)
    let skull_idx = mtk_voxel::storage::block_index(5, 5, 5) as u16;
    let skull_pal = sec.palette.len() as u16;
    sec.palette
        .push("minecraft:player_head[rotation=4]".to_string());
    sec.voxels[skull_idx as usize] = skull_pal;

    let skull_entity = mtk_model::BlockEntityData::new_skull(
        mtk_model::entity::skull::SkullKind::Player,
        Some(4),
        Some(mtk_model::entity::skull::PlayerProfileData::new(
            Some("Steve".to_string()),
            None,
            Some("http://textures.minecraft.net/texture/cust123".to_string()),
            None,
        )),
    );
    sec.set_block_entity(skull_idx, Some(skull_entity));

    sec.non_air_count = 2;

    let padded = sec.build_padded_array(|_, _, _| "minecraft:air");
    assert_eq!(padded.block_entities.len(), 2);

    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    // Ensure both block entities emitted mesh faces
    assert!(
        mesh.face_count() >= 22,
        "Section must contain faces from both pot and skull (got {})",
        mesh.face_count()
    );
    assert_eq!(mesh.face_materials.len(), mesh.face_count());
}

#[test]
fn test_player_skull_dynamic_atlas_meshing_and_uv_remapping() {
    let mut world = mtk_voxel::world::VoxelWorld::new(None, None, None);

    let hash = "5813943ca53225ee0ee814321cba1e420ea21f7c55f2998f9306cfad6655f69e";
    let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([128, 64, 32, 255]));
    let mut png_buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png_buf, image::ImageFormat::Png)
        .expect("PNG encoding should succeed");
    let png_bytes = png_buf.into_inner();

    world.skin_resolver.register_memory_skin(hash, png_bytes);

    let skull_entity = mtk_model::BlockEntityData::new_skull(
        mtk_model::entity::skull::SkullKind::Player,
        Some(0),
        Some(mtk_model::entity::skull::PlayerProfileData::new(
            Some("Steve".to_string()),
            Some("00000000-0000-0000-0000-000000000000".to_string()),
            Some(format!("http://textures.minecraft.net/texture/{}", hash)),
            None,
        )),
    );

    world.set_block(2, 2, 2, "minecraft:player_head[rotation=0]", None);
    let sec_coord = IVec3::new(0, 0, 0);
    let sec = world.storage.sections.get_mut(&sec_coord).unwrap();
    let local_idx = mtk_voxel::storage::block_index(2, 2, 2) as u16;
    sec.set_block_entity(local_idx, Some(skull_entity));

    let mesh = world.rebuild_all().expect("Rebuild should succeed").clone();
    assert!(mesh.face_count() > 0);

    // Verify dynamic skin atlas allocated slot and blitted pixels
    // 16 pinned slots for vanilla banners + 1 player skull
    assert_eq!(world.dynamic_entities.allocated_count(), 17);
    let atlas_rgba = world
        .dynamic_entity_atlas_rgba()
        .expect("Atlas RGBA must exist");
    assert_eq!(atlas_rgba.len(), 1024 * 1024 * 4);

    // Verify skull face material slot is 0xFFF0
    assert!(
        mesh.face_materials
            .contains(&mtk_texture::atlas::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID),
        "Mesh must contain dynamic entity material slot 0xFFF0"
    );

    // Verify skull UVs are remapped into allocated slot bounds (slot 16)
    let dynamic_slot = mtk_texture::atlas::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
    let (_, slot_idx) = world
        .dynamic_entities
        .get_location(hash)
        .expect("Skull slot");
    assert_eq!(slot_idx, 16);
    let expected = mtk_texture::atlas::DynamicAtlasChunk::slot_info(slot_idx).uv_bounds;

    let mut dynamic_face_found = false;
    for (f_idx, &mat) in mesh.face_materials.iter().enumerate() {
        if mat == dynamic_slot {
            dynamic_face_found = true;
            for tri in 0..2 {
                let v_start = (f_idx * 2 + tri) * 3;
                for i in 0..3 {
                    let idx = mesh.indices[v_start + i] as usize;
                    let uv = mesh.uvs[idx];
                    assert!(
                        uv[0] >= expected[0] - 1e-4 && uv[0] <= expected[2] + 1e-4,
                        "U must be within slot {} bounds (got {})",
                        slot_idx,
                        uv[0]
                    );
                    assert!(
                        uv[1] >= expected[1] - 1e-4 && uv[1] <= expected[3] + 1e-4,
                        "V must be within slot {} bounds (got {})",
                        slot_idx,
                        uv[1]
                    );
                }
            }
        }
    }
    assert!(dynamic_face_found, "Must have checked dynamic face UVs");
}

#[test]
fn test_non_player_skull_textures_not_clobbered() {
    let mut baker = mtk_model::baker::ModelBaker::new();

    // 1. Zombie head without block entity profile (typical modern 1.13+ save where NBT lacks SkullType)
    let dummy_be = mtk_model::BlockEntityData::new_skull(
        mtk_model::entity::skull::SkullKind::Skeleton, // Default fallback from NBT without SkullType
        None,
        None,
    );
    let zombie_model = baker
        .bake_blockstate_with_entity("minecraft:zombie_head", Some(&dummy_be), None, |_| None)
        .expect("Zombie head baking must succeed");

    let tex_names: Vec<String> = zombie_model
        .unculled_faces
        .iter()
        .map(|f| f.texture.clone())
        .collect();
    assert!(
        tex_names.iter().any(|t| t.contains("zombie")),
        "Zombie head faces must use zombie texture, got: {:?}",
        tex_names
    );
    assert!(
        !tex_names.iter().any(|t| t.contains("skeleton/skeleton")),
        "Zombie head faces must NOT be clobbered by skeleton texture"
    );

    // 2. Creeper head without profile
    let creeper_model = baker
        .bake_blockstate_with_entity("minecraft:creeper_head", Some(&dummy_be), None, |_| None)
        .expect("Creeper head baking must succeed");
    let creeper_tex: Vec<String> = creeper_model
        .unculled_faces
        .iter()
        .map(|f| f.texture.clone())
        .collect();
    assert!(
        creeper_tex.iter().any(|t| t.contains("creeper")),
        "Creeper head faces must use creeper texture, got: {:?}",
        creeper_tex
    );

    // 3. Pre-1.20.5 ground skull rotation from block entity (Rot = 4 => 270 deg)
    let rot_be = mtk_model::BlockEntityData::new_skull(
        mtk_model::entity::skull::SkullKind::Skeleton,
        Some(4),
        None,
    );
    let rot_model = baker
        .bake_blockstate_with_entity("minecraft:skeleton_skull", Some(&rot_be), None, |_| None)
        .expect("Skeleton skull baking must succeed");
    assert!(!rot_model.unculled_faces.is_empty());
}

#[test]
fn test_mesher_average_color_and_tint_injection() {
    use mtk_material::biome::hex_to_linear_rgba;
    use mtk_texture::atlas::{AtlasAddressMap, AtlasSpriteLocation, SpriteKind};

    let mut address_map = AtlasAddressMap::new();

    // 1. Stone with average_color "#787878"
    let stone_loc = mtk_resource::ResourceLocation::parse("minecraft:block/stone").unwrap();
    address_map.sprites.insert(
        stone_loc,
        AtlasSpriteLocation {
            category: "blocks".to_string(),
            sprite_kind: SpriteKind::StaticAtlas,
            chunk_id: 0,
            texture_id: 1,
            frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
            average_color: Some("#787878".to_string()),
            ..Default::default()
        },
    );

    // 2. Cobblestone without average_color (legacy cache fallback to white)
    let cobble_loc = mtk_resource::ResourceLocation::parse("minecraft:block/cobblestone").unwrap();
    address_map.sprites.insert(
        cobble_loc,
        AtlasSpriteLocation {
            category: "blocks".to_string(),
            sprite_kind: SpriteKind::StaticAtlas,
            chunk_id: 0,
            texture_id: 2,
            frame_0_uv_bounds: [0.5, 0.0, 1.0, 0.5],
            average_color: None,
            ..Default::default()
        },
    );

    // 3. Grass block top with average_color "#808080" (tinted)
    let grass_top_loc =
        mtk_resource::ResourceLocation::parse("minecraft:block/grass_block_top").unwrap();
    address_map.sprites.insert(
        grass_top_loc,
        AtlasSpriteLocation {
            category: "blocks".to_string(),
            sprite_kind: SpriteKind::StaticAtlas,
            chunk_id: 0,
            texture_id: 3,
            frame_0_uv_bounds: [0.0, 0.5, 0.5, 1.0],
            average_color: Some("#808080".to_string()),
            ..Default::default()
        },
    );

    let mut sec = mtk_voxel::storage::SectionStorage::new(glam::IVec3::new(0, 0, 0));
    // Put stone at (1, 1, 1)
    let pal_stone = sec.palette.len() as u16;
    sec.palette.push("minecraft:stone".to_string());
    sec.voxels[mtk_voxel::storage::block_index(1, 1, 1) as usize] = pal_stone;

    // Put cobblestone at (3, 1, 1)
    let pal_cobble = sec.palette.len() as u16;
    sec.palette.push("minecraft:cobblestone".to_string());
    sec.voxels[mtk_voxel::storage::block_index(3, 1, 1) as usize] = pal_cobble;

    // Put grass_block at (5, 1, 1)
    let pal_grass = sec.palette.len() as u16;
    sec.palette.push("minecraft:grass_block".to_string());
    sec.voxels[mtk_voxel::storage::block_index(5, 1, 1) as usize] = pal_grass;

    sec.non_air_count = 3;

    let padded = sec.build_padded_array(|_, _, _| "minecraft:air");
    let culler = FaceCuller::default();
    let config = MesherConfig {
        atlas_address_map: Some(std::sync::Arc::new(address_map)),
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    assert!(mesh.colors.is_some(), "mesh.colors must be populated");
    let colors = mesh.colors.as_ref().unwrap();
    // 3 cubes * 6 faces * 4 corners = 72 colors
    assert_eq!(colors.len(), 72);

    // Verify stone faces have linear "#787878"
    let expected_stone = hex_to_linear_rgba("#787878");
    for i in 0..24 {
        for c in 0..3 {
            assert!(
                (colors[i][c] - expected_stone[c]).abs() < 1e-3,
                "Stone corner {} channel {} expected {}, got {}",
                i,
                c,
                expected_stone[c],
                colors[i][c]
            );
        }
        assert_eq!(colors[i][3], 1.0);
    }

    // Verify cobblestone faces without average_color fallback to pure white [1, 1, 1, 1]
    for i in 24..48 {
        assert_eq!(
            colors[i],
            [1.0, 1.0, 1.0, 1.0],
            "Legacy chunk without average_color must fallback to pure white"
        );
    }

    // Verify grass block has tinted top face (greenish, where G > R and G > B)
    let mut found_tinted_green = false;
    for i in 48..72 {
        let col = colors[i];
        if col[1] > col[0] && col[1] > col[2] {
            found_tinted_green = true;
            break;
        }
    }
    assert!(
        found_tinted_green,
        "Grass block top face must be tinted green"
    );
}

#[test]
fn test_mesher_baked_model_atlas_uvs_average_color() {
    use mtk_core::Direction;
    use mtk_material::biome::hex_to_linear_rgba;
    use mtk_model::{BakedFace, BakedModel};
    use mtk_texture::atlas::{AtlasAddressMap, AtlasSpriteLocation, SpriteKind};
    use std::sync::Arc;

    let mut address_map = AtlasAddressMap::new();

    // Dirt with average_color "#866043"
    let dirt_loc = mtk_resource::ResourceLocation::parse("minecraft:block/dirt").unwrap();
    address_map.sprites.insert(
        dirt_loc,
        AtlasSpriteLocation {
            category: "blocks".to_string(),
            sprite_kind: SpriteKind::StaticAtlas,
            chunk_id: 14,
            texture_id: 3972,
            frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
            average_color: Some("#866043".to_string()),
            ..Default::default()
        },
    );

    let mut model = BakedModel {
        block_state: "minecraft:dirt".to_string(),
        ..Default::default()
    };
    let face = BakedFace {
        direction: Direction::Up,
        texture: "minecraft:block/dirt".to_string(),
        uv_rot: 0.0,
        uv_bounds: [0.0, 0.0, 1.0, 1.0],
        tint_index: -1,
        cullface: None,
        vertices: [
            glam::Vec3::new(0.0, 1.0, 0.0),
            glam::Vec3::new(1.0, 1.0, 0.0),
            glam::Vec3::new(1.0, 1.0, 1.0),
            glam::Vec3::new(0.0, 1.0, 1.0),
        ],
        uvs: [
            glam::Vec2::new(0.0, 0.0),
            glam::Vec2::new(1.0, 0.0),
            glam::Vec2::new(1.0, 1.0),
            glam::Vec2::new(0.0, 1.0),
        ],
        normal: glam::Vec3::new(0.0, 1.0, 0.0),
        atlas_uvs: Some([
            glam::Vec2::new(0.0, 0.0),
            glam::Vec2::new(0.1, 0.0),
            glam::Vec2::new(0.1, 0.1),
            glam::Vec2::new(0.0, 0.1),
        ]),
        atlas_chunk_id: Some(14),
        atlas_texture_id: Some(3972),
    };
    model.culled_faces[Direction::Up.to_index()].push(face);
    let model_arc = Arc::new(model);

    let mut sec = mtk_voxel::storage::SectionStorage::new(glam::IVec3::new(0, 0, 0));
    let pal_dirt = sec.palette.len() as u16;
    sec.palette.push("minecraft:dirt".to_string());
    sec.voxels[mtk_voxel::storage::block_index(1, 1, 1) as usize] = pal_dirt;
    sec.non_air_count = 1;

    let padded = sec.build_padded_array(|_, _, _| "minecraft:air");
    let culler = FaceCuller::default();
    let config = MesherConfig {
        atlas_address_map: Some(std::sync::Arc::new(address_map)),
        ..Default::default()
    };

    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |state| {
            if state == "minecraft:dirt" {
                ModelSource::from(Some(model_arc.clone()))
            } else {
                ModelSource::None
            }
        },
        &config,
    );

    assert!(mesh.colors.is_some(), "mesh.colors must be present");
    let colors = mesh.colors.as_ref().unwrap();
    assert_eq!(colors.len(), 4, "Must have 4 vertex colors for 1 face");

    let expected_dirt = hex_to_linear_rgba("#866043");
    for i in 0..4 {
        for c in 0..3 {
            assert!(
                (colors[i][c] - expected_dirt[c]).abs() < 1e-3,
                "Dirt corner {} channel {} expected {}, got {}",
                i,
                c,
                expected_dirt[c],
                colors[i][c]
            );
        }
        assert_eq!(colors[i][3], 1.0);
    }
}

#[test]
fn test_banner_dynamic_atlas_meshing_and_baking() {
    let mut world = mtk_voxel::world::VoxelWorld::new(None, None, None);

    // Place standing red banner (plain, no patterns)
    world.set_block(2, 2, 2, "minecraft:red_banner[rotation=0]", None);

    // Place wall banner with patterns
    let banner_entity = mtk_model::BlockEntityData::new_banner(
        "white",
        vec![("stripe_top", "blue"), ("flower", "red")],
    );
    world.set_block(2, 2, 3, "minecraft:white_wall_banner[facing=north]", None);
    let sec_coord = IVec3::new(0, 0, 0);
    let sec = world.storage.sections.get_mut(&sec_coord).unwrap();
    let local_idx = mtk_voxel::storage::block_index(2, 2, 3) as u16;
    sec.set_block_entity(local_idx, Some(banner_entity.clone()));

    let mesh = world.rebuild_all().expect("Rebuild should succeed").clone();
    assert!(mesh.face_count() > 0);

    // 16 pinned slots for vanilla banners + 1 custom patterned banner = 17
    assert_eq!(world.dynamic_entities.allocated_count(), 17);

    // Red banner slot is 14
    let red_slot = 14;
    let expected_red = mtk_texture::atlas::DynamicAtlasChunk::slot_info(red_slot).uv_bounds;

    // Patterned banner slot is 16
    let pat_key = "minecraft:banner/white+stripe_top#blue+flower#red";
    let (cid, pat_slot) = world
        .dynamic_entities
        .get_location(pat_key)
        .expect("Patterned banner slot");
    assert_eq!(cid, 0xFFF0);
    assert_eq!(pat_slot, 16);
    let expected_pat = mtk_texture::atlas::DynamicAtlasChunk::slot_info(pat_slot).uv_bounds;

    let dynamic_chunk_id = mtk_texture::atlas::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
    let mut red_cloth_found = false;
    let mut pat_cloth_found = false;

    for f_idx in 0..mesh.face_count() {
        let mat = mesh.face_materials[f_idx];
        if mat == dynamic_chunk_id {
            let mut is_red = true;
            let mut is_pat = true;
            for i in 0..4 {
                let uv = mesh.uvs[f_idx * 4 + i];
                if uv[0] < expected_red[0] - 1e-4
                    || uv[0] > expected_red[2] + 1e-4
                    || uv[1] < expected_red[1] - 1e-4
                    || uv[1] > expected_red[3] + 1e-4
                {
                    is_red = false;
                }
                if uv[0] < expected_pat[0] - 1e-4
                    || uv[0] > expected_pat[2] + 1e-4
                    || uv[1] < expected_pat[1] - 1e-4
                    || uv[1] > expected_pat[3] + 1e-4
                {
                    is_pat = false;
                }
            }
            if is_red {
                red_cloth_found = true;
            }
            if is_pat {
                pat_cloth_found = true;
            }
        }
    }

    assert!(
        red_cloth_found,
        "Must have found red banner cloth in dynamic slot 14"
    );
    assert!(
        pat_cloth_found,
        "Must have found patterned banner cloth in dynamic slot 16"
    );
}

#[test]
fn test_plain_banner_without_block_entity_meshing() {
    let mut addr_map = mtk_texture::atlas::AtlasAddressMap::new();
    let banner_base_id =
        mtk_resource::ResourceLocation::parse("minecraft:entity/banner/banner_base").unwrap();
    let banner_base_loc = mtk_texture::atlas::AtlasSpriteLocation {
        chunk_id: 0,
        category: "banner_patterns".to_string(),
        is_animated: false,
        sprite_kind: mtk_texture::atlas::SpriteKind::StaticAtlas,
        texture_id: 0,
        uv_bounds: [0.0, 0.0, 0.5, 0.5],
        frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
        local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
        frame_uv_step: [0.0, 0.0],
        pixel_rect: [0, 0, 64, 64],
        strip_pixel_rect: [0, 0, 64, 64],
        frame_size: [64, 64],
        frame_count: 1,
        animation: None,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        average_color: None,
    };
    addr_map.sprites.insert(banner_base_id, banner_base_loc);

    let config = mtk_voxel::MesherConfig {
        atlas_address_map: Some(std::sync::Arc::new(addr_map)),
        ..Default::default()
    };
    let mut world = mtk_voxel::world::VoxelWorld::new(Some(config), None, None);

    // Verify 16 prewarmed banners exist in world.config.atlas_address_map immediately
    let map = world.config.atlas_address_map.as_ref().unwrap();
    assert!(map.lookup_str("minecraft:banner/red").is_some());
    assert!(map.lookup_str("minecraft:banner/blue").is_some());

    // Place a plain red banner and blue wall banner WITHOUT any block entities
    world.set_block(1, 1, 1, "minecraft:red_banner[rotation=4]", None);
    world.set_block(3, 1, 3, "minecraft:blue_wall_banner[facing=north]", None);

    let mesh = world.rebuild_all().expect("Rebuild should succeed").clone();
    assert!(mesh.face_count() > 0);

    let dynamic_chunk_id = mtk_texture::atlas::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
    let mut red_cloth_found = false;
    let mut blue_cloth_found = false;
    let expected_red = mtk_texture::atlas::DynamicAtlasChunk::slot_info(14).uv_bounds;
    let expected_blue = mtk_texture::atlas::DynamicAtlasChunk::slot_info(11).uv_bounds;

    for f_idx in 0..mesh.face_count() {
        let mat = mesh.face_materials[f_idx];
        if mat == dynamic_chunk_id {
            let mut is_red = true;
            let mut is_blue = true;
            for i in 0..4 {
                let uv = mesh.uvs[f_idx * 4 + i];
                if uv[0] < expected_red[0] - 1e-4
                    || uv[0] > expected_red[2] + 1e-4
                    || uv[1] < expected_red[1] - 1e-4
                    || uv[1] > expected_red[3] + 1e-4
                {
                    is_red = false;
                }
                if uv[0] < expected_blue[0] - 1e-4
                    || uv[0] > expected_blue[2] + 1e-4
                    || uv[1] < expected_blue[1] - 1e-4
                    || uv[1] > expected_blue[3] + 1e-4
                {
                    is_blue = false;
                }
            }
            if is_red {
                red_cloth_found = true;
            }
            if is_blue {
                blue_cloth_found = true;
            }
        }
    }

    assert!(
        red_cloth_found,
        "Plain standing red banner cloth must map to slot 14 on dynamic atlas"
    );
    assert!(
        blue_cloth_found,
        "Plain wall blue banner cloth must map to slot 11 on dynamic atlas"
    );
}
