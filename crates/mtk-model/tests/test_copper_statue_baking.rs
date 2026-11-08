use mtk_model::copper_statue::{CopperOxidation, CopperStatuePose};
use mtk_model::{BlockStateDefinition, ModelBaker};

#[test]
fn test_copper_statue_oxidation_and_textures() {
    assert_eq!(
        CopperStatuePose::parse("standing"),
        CopperStatuePose::Standing
    );
    assert_eq!(
        CopperStatuePose::parse("sitting"),
        CopperStatuePose::Sitting
    );
    assert_eq!(
        CopperStatuePose::parse("running"),
        CopperStatuePose::Running
    );
    assert_eq!(CopperStatuePose::parse("star"), CopperStatuePose::Star);
    assert_eq!(
        CopperStatuePose::parse("unknown"),
        CopperStatuePose::Standing
    );

    let cases = [
        (
            "minecraft:copper_golem_statue",
            CopperOxidation::Unaffected,
            "minecraft:entity/copper_golem/copper_golem",
        ),
        (
            "minecraft:waxed_copper_golem_statue",
            CopperOxidation::Unaffected,
            "minecraft:entity/copper_golem/copper_golem",
        ),
        (
            "minecraft:exposed_copper_golem_statue",
            CopperOxidation::Exposed,
            "minecraft:entity/copper_golem/copper_golem_exposed",
        ),
        (
            "minecraft:waxed_exposed_copper_golem_statue",
            CopperOxidation::Exposed,
            "minecraft:entity/copper_golem/copper_golem_exposed",
        ),
        (
            "minecraft:weathered_copper_golem_statue",
            CopperOxidation::Weathered,
            "minecraft:entity/copper_golem/copper_golem_weathered",
        ),
        (
            "minecraft:waxed_weathered_copper_golem_statue",
            CopperOxidation::Weathered,
            "minecraft:entity/copper_golem/copper_golem_weathered",
        ),
        (
            "minecraft:oxidized_copper_golem_statue",
            CopperOxidation::Oxidized,
            "minecraft:entity/copper_golem/copper_golem_oxidized",
        ),
        (
            "minecraft:waxed_oxidized_copper_golem_statue",
            CopperOxidation::Oxidized,
            "minecraft:entity/copper_golem/copper_golem_oxidized",
        ),
    ];

    for (name, expected_ox, expected_tex) in cases {
        let ox = CopperOxidation::from_block_name(name);
        assert_eq!(ox, expected_ox, "Mismatch oxidation for {}", name);
        assert_eq!(
            ox.texture_path(),
            expected_tex,
            "Mismatch texture for {}",
            name
        );
    }
}

#[test]
fn test_copper_statue_state_expansion() {
    let dummy_def: BlockStateDefinition = serde_json::from_str(
        r#"{"variants":{"":{"model":"minecraft:block/copper_golem_statue"}}}"#,
    )
    .expect("Parse dummy def");

    let states = dummy_def.enumerate_all_states("minecraft:copper_golem_statue");
    // 4 poses * 4 facings = 16 states + base_id = 17 states
    assert_eq!(states.len(), 17);
    assert!(states.contains(
        &"minecraft:copper_golem_statue[copper_golem_pose=standing,facing=north]".to_string()
    ));
    assert!(states.contains(
        &"minecraft:copper_golem_statue[copper_golem_pose=sitting,facing=east]".to_string()
    ));
    assert!(states.contains(
        &"minecraft:copper_golem_statue[copper_golem_pose=running,facing=south]".to_string()
    ));
    assert!(states.contains(
        &"minecraft:copper_golem_statue[copper_golem_pose=star,facing=west]".to_string()
    ));
    assert!(states.contains(&"minecraft:copper_golem_statue".to_string()));
}

#[test]
fn test_copper_statue_all_poses_baking() {
    let mut baker = ModelBaker::new();

    for pose in ["standing", "sitting", "running", "star"] {
        let state = format!(
            "minecraft:weathered_copper_golem_statue[copper_golem_pose={},facing=north]",
            pose
        );
        let baked = baker
            .bake_blockstate(&state, None, |_| None)
            .expect("Must bake statue pose");

        assert!(
            baked.elements.len() >= 9,
            "Pose {} must have elements",
            pose
        );
        for (elem_idx, elem) in baked.elements.iter().enumerate() {
            assert_eq!(
                elem.faces.len(),
                6,
                "Pose {} element {} must have exactly 6 faces",
                pose,
                elem_idx
            );
        }
        assert!(
            !baked.unculled_faces.is_empty(),
            "Pose {} must have unculled faces",
            pose
        );

        // Check texture assignment
        for face in &baked.unculled_faces {
            assert_eq!(
                face.texture, "minecraft:entity/copper_golem/copper_golem_weathered",
                "Face texture in pose {} must match weathered texture",
                pose
            );
            // Y vertex positions should be non-negative (ground at 0.0 or slightly above)
            for v in &face.vertices {
                assert!(
                    v.y >= -0.05 && v.y <= 1.6,
                    "Pose {} vertex y={} out of expected bounds",
                    pose,
                    v.y
                );
            }
        }

        // Verify DCC mesh generation works
        let mesh = baked.to_mesh(false);
        assert!(
            mesh.vertex_count() > 0,
            "Mesh for pose {} must have vertices",
            pose
        );
        assert!(
            mesh.face_count() > 0,
            "Mesh for pose {} must have faces",
            pose
        );
    }
}

#[test]
fn test_copper_statue_facing_rotation() {
    let mut baker = ModelBaker::new();

    let baked_north = baker
        .bake_blockstate(
            "minecraft:copper_golem_statue[copper_golem_pose=standing,facing=north]",
            None,
            |_| None,
        )
        .expect("Bake north");

    let baked_south = baker
        .bake_blockstate(
            "minecraft:copper_golem_statue[copper_golem_pose=standing,facing=south]",
            None,
            |_| None,
        )
        .expect("Bake south");

    let baked_east = baker
        .bake_blockstate(
            "minecraft:copper_golem_statue[copper_golem_pose=standing,facing=east]",
            None,
            |_| None,
        )
        .expect("Bake east");

    // Face counts must match across rotations
    assert_eq!(
        baked_north.unculled_faces.len(),
        baked_south.unculled_faces.len()
    );
    assert_eq!(
        baked_north.unculled_faces.len(),
        baked_east.unculled_faces.len()
    );

    // South rotation should invert Z relative to center 0.5:
    // If a vertex has Z in north, its south counterpart should have ~ (1.0 - Z)
    let north_z_avg: f32 = baked_north
        .unculled_faces
        .iter()
        .flat_map(|f| f.vertices.iter())
        .map(|v| v.z)
        .sum::<f32>()
        / (baked_north.unculled_faces.len() * 4) as f32;

    let south_z_avg: f32 = baked_south
        .unculled_faces
        .iter()
        .flat_map(|f| f.vertices.iter())
        .map(|v| v.z)
        .sum::<f32>()
        / (baked_south.unculled_faces.len() * 4) as f32;

    // Both averages should be symmetric around 0.5
    assert!(
        (north_z_avg + south_z_avg - 1.0).abs() < 0.05,
        "North Z avg {} and South Z avg {} should be symmetric around 0.5",
        north_z_avg,
        south_z_avg
    );
}

#[test]
fn test_copper_statue_bake_variants_group() {
    let mut baker = ModelBaker::new();
    let state =
        "minecraft:waxed_oxidized_copper_golem_statue[copper_golem_pose=running,facing=west]";

    let group = baker
        .bake_blockstate_variants(state, None, |_| None)
        .expect("Bake variant group");

    assert_eq!(group.base_state, state);
    let primary = group.select_primary();
    assert!(!primary.unculled_faces.is_empty());
    assert_eq!(
        primary.unculled_faces[0].texture,
        "minecraft:entity/copper_golem/copper_golem_oxidized"
    );

    // Verify cull_meta is NonOccluding and not full cube
    let meta = primary
        .cull_meta
        .as_ref()
        .expect("Copper statue must have cull_meta");
    assert_eq!(meta.category, mtk_cull::CullCategory::NonOccluding);
    assert!(!meta.is_full_cube, "Copper statue must not be full cube");
    assert!(!meta.is_opaque, "Copper statue must not be opaque");

    // Verify 6-face summary contains real geometric vertices and uvs for each direction
    use mtk_core::Direction;
    for dir in Direction::ALL {
        let f = primary.get_face(dir);
        assert_eq!(f.direction, dir);
        assert_ne!(
            f.vertices,
            [glam::Vec3::ZERO; 4],
            "{:?} face summary must have non-zero vertices",
            dir
        );
        assert_ne!(
            f.uvs,
            [glam::Vec2::ZERO; 4],
            "{:?} face summary must have non-zero UVs",
            dir
        );
    }
}

#[test]
fn test_copper_statue_arm_and_leg_uv_bounds_strictly_fit_opaque_texture() {
    let mut baker = ModelBaker::new();
    let state = "minecraft:copper_golem_statue[copper_golem_pose=standing,facing=north]";
    let baked = baker
        .bake_blockstate(state, None, |_| None)
        .expect("Bake statue");

    // Right Arm: box at (-3, -1, -2) size (3, 10, 4) with tex_offset (36, 16)
    // Left Arm: box at (0, -1, -2) size (3, 10, 4) with tex_offset (50, 16)
    // Find all faces originating from Direction::Down (the bottom of arm/foot in entity coords):
    let mut right_arm_bottom: Option<[f32; 4]> = None;
    let mut left_arm_bottom: Option<[f32; 4]> = None;

    for elem in &baked.elements {
        if let Some(down_face) = elem.faces.get(&mtk_core::Direction::Down) {
            let u_span = (down_face.uv_bounds[2] - down_face.uv_bounds[0]) * 64.0;
            let v_span = (down_face.uv_bounds[3] - down_face.uv_bounds[1]) * 64.0;
            let u_min_px = (down_face.uv_bounds[0] * 64.0).round() as i32;
            let v_min_px = (down_face.uv_bounds[1] * 64.0).round() as i32;

            if (u_min_px - 43).abs() <= 1 && (v_min_px - 16).abs() <= 1 {
                right_arm_bottom = Some([
                    down_face.uv_bounds[0] * 64.0,
                    down_face.uv_bounds[1] * 64.0,
                    down_face.uv_bounds[2] * 64.0,
                    down_face.uv_bounds[3] * 64.0,
                ]);
                assert!(
                    (u_span - 3.0).abs() < 1e-4,
                    "Right arm bottom U span must be exactly 3px, got {}px (must not sample transparent 4th pixel!)",
                    u_span
                );
                assert!(
                    (v_span - 4.0).abs() < 1e-4,
                    "Right arm bottom V span must be exactly 4px, got {}px",
                    v_span
                );
            } else if (u_min_px - 57).abs() <= 1 && (v_min_px - 16).abs() <= 1 {
                left_arm_bottom = Some([
                    down_face.uv_bounds[0] * 64.0,
                    down_face.uv_bounds[1] * 64.0,
                    down_face.uv_bounds[2] * 64.0,
                    down_face.uv_bounds[3] * 64.0,
                ]);
                assert!(
                    (u_span - 3.0).abs() < 1e-4,
                    "Left arm bottom U span must be exactly 3px, got {}px (must not sample transparent 4th pixel!)",
                    u_span
                );
                assert!(
                    (v_span - 4.0).abs() < 1e-4,
                    "Left arm bottom V span must be exactly 4px, got {}px",
                    v_span
                );
            }
        }
    }

    assert!(
        right_arm_bottom.is_some(),
        "Must find right arm bottom face"
    );
    assert!(left_arm_bottom.is_some(), "Must find left arm bottom face");
}
