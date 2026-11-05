use mtk_voxel::{MesherConfig, ModelSource, SectionMesher, VoxelStorage};

mod common;

#[test]
fn test_fluid_side_and_top_texture_keys_with_atlas() {
    use glam::IVec3;
    use mtk_cull::FaceCuller;
    use mtk_resource::ResourceLocation;
    use mtk_texture::{AtlasAddressMap, AtlasSpriteLocation};

    let mut address_map = AtlasAddressMap::new();
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_still"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 201,
            uv_bounds: [0.0, 0.0, 0.5, 0.5],
            frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_flow"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 202,
            uv_bounds: [0.5, 0.0, 1.0, 1.0],
            frame_0_uv_bounds: [0.5, 0.0, 1.0, 1.0],
            ..Default::default()
        },
    );

    let config = MesherConfig {
        atlas_address_map: Some(std::sync::Arc::new(address_map)),
        enable_ao: false,
        ..Default::default()
    };

    let culler = FaceCuller::default();

    // 1. Still water isolated block:
    // Top face should be water_still (id 201).
    // Bottom face should be water_still (id 201).
    // North, South, West, East side faces should ALL be water_flow (id 202)!
    let mut still_world = VoxelStorage::new();
    still_world.set_bounds(0, 0, 0, 16, 16, 16);
    still_world.set_block(4, 4, 4, "minecraft:water[level=0]", None);

    let padded_still = still_world.get_section_padded_array(IVec3::new(0, 0, 0));
    let still_mesh =
        SectionMesher::mesh_section(&padded_still, &culler, |_| ModelSource::None, &config);

    assert_eq!(still_mesh.face_count(), 6);

    let keys = common::face_texture_keys(&still_mesh);
    // Emission order is stable: Up, Down, North, South, West, East.
    for (i, key) in keys.iter().enumerate() {
        if i <= 1 {
            assert_eq!(
                key, "minecraft:block/water_still",
                "Up/Down face of still water must use water_still"
            );
        } else {
            assert_eq!(
                key, "minecraft:block/water_flow",
                "Side face of fluid must ALWAYS use water_flow"
            );
        }
    }

    // 2. Flowing water block:
    // Top face should also be water_flow!
    let mut flow_world = VoxelStorage::new();
    flow_world.set_bounds(0, 0, 0, 16, 16, 16);
    flow_world.set_block(4, 4, 4, "minecraft:water[level=1]", None);

    let padded_flow = flow_world.get_section_padded_array(IVec3::new(0, 0, 0));
    let flow_mesh =
        SectionMesher::mesh_section(&padded_flow, &culler, |_| ModelSource::None, &config);

    assert_eq!(flow_mesh.face_count(), 6);

    let flow_keys = common::face_texture_keys(&flow_mesh);
    for (i, key) in flow_keys.iter().enumerate() {
        if i == 1 {
            assert_eq!(
                key, "minecraft:block/water_still",
                "Down face of fluid uses water_still"
            );
        } else {
            assert_eq!(
                key, "minecraft:block/water_flow",
                "Top and side faces of flowing water must use water_flow"
            );
        }
    }
}

#[test]
fn test_animated_texture_routing_dual_channel() {
    use glam::IVec3;
    use mtk_cull::FaceCuller;
    use mtk_resource::ResourceLocation;
    use mtk_texture::{AtlasAddressMap, AtlasSpriteLocation};

    let mut address_map = AtlasAddressMap::new();
    // Static chunk (chunk 0): Frame 0 fallback baseline
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_still"),
        AtlasSpriteLocation {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            texture_id: 101,
            uv_bounds: [0.0, 0.0, 0.25, 0.25],
            frame_0_uv_bounds: [0.0, 0.0, 0.25, 0.25],
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_flow"),
        AtlasSpriteLocation {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            texture_id: 102,
            uv_bounds: [0.25, 0.0, 0.5, 0.25],
            frame_0_uv_bounds: [0.25, 0.0, 0.5, 0.25],
            ..Default::default()
        },
    );

    // Dedicated animated chunk (chunk 1): Animated strip
    address_map.anim_sprites.insert(
        ResourceLocation::new("minecraft", "block/water_still"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            is_animated: true,
            texture_id: 201,
            uv_bounds: [0.0, 0.0, 0.1, 1.0],
            frame_0_uv_bounds: [0.0, 0.9, 0.1, 1.0],
            ..Default::default()
        },
    );
    address_map.anim_sprites.insert(
        ResourceLocation::new("minecraft", "block/water_flow"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            is_animated: true,
            texture_id: 202,
            uv_bounds: [0.1, 0.0, 0.2, 1.0],
            frame_0_uv_bounds: [0.1, 0.9, 0.2, 1.0],
            ..Default::default()
        },
    );

    let atlas_arc = std::sync::Arc::new(address_map);
    let culler = FaceCuller::default();

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(4, 4, 4, "minecraft:water[level=0]", None);
    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));

    // 1. With enable_animated_textures = true -> should route to anim chunk (chunk 1)
    let config_anim = MesherConfig {
        atlas_address_map: Some(atlas_arc.clone()),
        enable_ao: false,
        enable_animated_textures: true,
        ..Default::default()
    };
    let mesh_anim =
        SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config_anim);
    let chunk_attr_anim = mesh_anim
        .get_custom_attribute("mtk_atlas_chunk_id")
        .expect("Must have atlas chunk IDs");
    let cids_anim = match &chunk_attr_anim.data {
        mtk_core::AttributeData::Int32(c) => c,
        _ => panic!("Expected Int32 attribute"),
    };
    for &cid in cids_anim {
        assert_eq!(
            cid, 1,
            "Animated texture must route to anim chunk 1 when enabled"
        );
    }

    // 2. With enable_animated_textures = false -> should route to static chunk (chunk 0)
    let config_static = MesherConfig {
        atlas_address_map: Some(atlas_arc),
        enable_ao: false,
        enable_animated_textures: false,
        ..Default::default()
    };
    let mesh_static =
        SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config_static);
    let chunk_attr_static = mesh_static
        .get_custom_attribute("mtk_atlas_chunk_id")
        .expect("Must have atlas chunk IDs");
    let cids_static = match &chunk_attr_static.data {
        mtk_core::AttributeData::Int32(c) => c,
        _ => panic!("Expected Int32 attribute"),
    };
    for &cid in cids_static {
        assert_eq!(
            cid, 0,
            "Animated texture must route to static chunk 0 when disabled"
        );
    }
}

#[test]
fn test_fluid_top_face_under_ceiling_is_not_culled() {
    use glam::IVec3;
    use mtk_cull::FaceCuller;

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(4, 4, 4, "minecraft:water[level=0]", None);
    world.set_block(4, 5, 4, "minecraft:stone", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |_| ModelSource::None,
        &MesherConfig::default(),
    );

    // 6 water quads (including the top surface at 8/9) + 6 stone quads.
    // The top water face is 1/9 below the ceiling and must remain visible.
    assert_eq!(mesh.face_count(), 12);
}

#[test]
fn test_flowing_fluid_top_face_under_ceiling_is_not_culled() {
    use glam::IVec3;
    use mtk_cull::FaceCuller;

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    // A sloped flow: a source feeding a lower flowing cell, with a block above the flow.
    world.set_block(4, 4, 4, "minecraft:water[level=0]", None);
    world.set_block(5, 4, 4, "minecraft:flowing_water[level=1]", None);
    world.set_block(5, 5, 4, "minecraft:stone", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |_| ModelSource::None,
        &MesherConfig::default(),
    );

    // Find the top surface of the flowing cell (x=5). All four corners are above the
    // block floor and below the ceiling, and every vertex stays inside x = [5, 6].
    let top_face_exists = mesh
        .quad_indices
        .as_ref()
        .expect("quad indices")
        .as_chunks::<4>()
        .0
        .iter()
        .any(|quad| {
            quad.iter().all(|&idx| {
                let p = mesh.positions[idx as usize];
                p[1] > 4.0 && p[1] < 5.0 && p[0] >= 5.0
            })
        });
    assert!(
        top_face_exists,
        "sloped fluid top face beneath a solid ceiling must not be culled"
    );
}

#[test]
fn test_bubble_column_and_waterlogged_continuous_fluid_mesh_no_hollow_wall() {
    use glam::IVec3;
    use mtk_cull::FaceCuller;

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // Row of 3 blocks submerged in water:
    // (4, 4, 4) = water
    // (5, 4, 4) = bubble_column
    // (6, 4, 4) = light[waterlogged=true]
    // (7, 4, 4) = water
    world.set_block(4, 4, 4, "minecraft:water[level=0]", None);
    world.set_block(5, 4, 4, "minecraft:bubble_column[drag=true]", None);
    world.set_block(6, 4, 4, "minecraft:light[level=15,waterlogged=true]", None);
    world.set_block(7, 4, 4, "minecraft:water[level=0]", None);

    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let mesh = SectionMesher::mesh_section(
        &padded,
        &culler,
        |_| ModelSource::None,
        &MesherConfig::default(),
    );

    // Verify there are NO internal vertical faces along X = 5.0, 6.0, or 7.0 between the fluid blocks.
    let quads = mesh
        .quad_indices
        .as_ref()
        .expect("quad indices")
        .as_chunks::<4>()
        .0;

    for quad in quads {
        let p0 = mesh.positions[quad[0] as usize];
        let p1 = mesh.positions[quad[1] as usize];
        let p2 = mesh.positions[quad[2] as usize];
        let p3 = mesh.positions[quad[3] as usize];

        // An internal X-wall would have all 4 vertices at the exact same X coordinate (e.g. 5.0, 6.0, or 7.0)
        let is_planar_x = (p0[0] - p1[0]).abs() < 1e-4
            && (p1[0] - p2[0]).abs() < 1e-4
            && (p2[0] - p3[0]).abs() < 1e-4;

        if is_planar_x {
            let x_val = p0[0];
            assert!(
                (x_val - 5.0).abs() > 1e-3 && (x_val - 6.0).abs() > 1e-3 && (x_val - 7.0).abs() > 1e-3,
                "Found unexpected internal fluid wall at X = {x_val} between water and bubble_column/waterlogged light!"
            );
        }
    }

    // All 4 blocks must produce top fluid surfaces (around y ~ 4.888)
    let top_quads_count = quads
        .iter()
        .filter(|quad| {
            let p0 = mesh.positions[quad[0] as usize];
            let p1 = mesh.positions[quad[1] as usize];
            let p2 = mesh.positions[quad[2] as usize];
            let p3 = mesh.positions[quad[3] as usize];
            let is_horizontal_y = (p0[1] - p1[1]).abs() < 1e-4
                && (p1[1] - p2[1]).abs() < 1e-4
                && (p2[1] - p3[1]).abs() < 1e-4;
            is_horizontal_y && p0[1] > 4.5
        })
        .count();

    assert_eq!(
        top_quads_count, 4,
        "All 4 adjacent fluid blocks (water, bubble_column, waterlogged light, water) must emit top fluid surfaces"
    );
}
