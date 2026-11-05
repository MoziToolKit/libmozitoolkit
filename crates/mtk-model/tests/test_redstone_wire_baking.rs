use mtk_core::Direction;
use mtk_model::{BakedModelDatabase, BlockModelJson, BlockStateDefinition, ModelBaker};

fn get_test_redstone_def() -> BlockStateDefinition {
    serde_json::from_str(include_str!(
        "fixtures/redstone/blockstates/redstone_wire.json"
    ))
    .unwrap()
}

fn get_test_redstone_model(id: &str) -> Option<BlockModelJson> {
    let clean = id.strip_prefix("minecraft:").unwrap_or(id);
    let stem = clean.strip_prefix("block/").unwrap_or(clean);
    let raw = match stem {
        "redstone_dust_dot" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_dot.json")
        }
        "redstone_dust_side0" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_side0.json")
        }
        "redstone_dust_side1" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_side1.json")
        }
        "redstone_dust_side_alt0" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_side_alt0.json")
        }
        "redstone_dust_side_alt1" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_side_alt1.json")
        }
        "redstone_dust_side" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_side.json")
        }
        "redstone_dust_side_alt" => {
            include_str!("fixtures/redstone/models/block/redstone_dust_side_alt.json")
        }
        "redstone_dust_up" => include_str!("fixtures/redstone/models/block/redstone_dust_up.json"),
        _ => return None,
    };
    serde_json::from_str(raw).ok()
}

#[test]
fn test_bake_redstone_wire_vanilla_resolution() {
    let mut baker = ModelBaker::new();
    let def = get_test_redstone_def();

    let baked = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=none,north=none,power=15,south=none,west=none]",
            Some(&def),
            get_test_redstone_model,
        )
        .expect("Baking must succeed for redstone_wire with vanilla definition");

    assert!(!baked.is_cube);
    assert!(baked.is_emissive);

    let (mesh, textures) = baked.to_mesh_with_textures(false);
    assert!(!mesh.positions.is_empty());
    assert!(textures.iter().any(|t| t.contains("redstone_dust_dot")));

    // Emission is baked-model metadata; retired face attributes must not be emitted.
    use mtk_core::RETIRED_MESH_ATTRIBUTES;
    for name in RETIRED_MESH_ATTRIBUTES {
        assert!(
            mesh.get_custom_attribute(name).is_none(),
            "retired attribute {name} must not be emitted"
        );
    }
}

#[test]
fn test_bake_redstone_wire_straight_line_z() {
    let mut baker = ModelBaker::new();
    let def = get_test_redstone_def();

    let baked_z = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=none,north=side,power=0,south=side,west=none]",
            Some(&def),
            get_test_redstone_model,
        )
        .unwrap();

    assert!(!baked_z.is_emissive);

    let (_mesh, textures) = baked_z.to_mesh_with_textures(false);
    assert!(textures.iter().any(|t| t.contains("redstone_dust_line0")));
    assert!(baked_z.elements.iter().any(|el| el
        .faces
        .values()
        .any(|f| f.texture.contains("redstone_dust_overlay"))));
    // Straight line in vanilla does not include dot
    assert!(!textures.iter().any(|t| t.contains("redstone_dust_dot")));
    assert_eq!(baked_z.elements.len(), 4); // 2 elements (line0 + overlay) * 2 parts
}

#[test]
fn test_bake_redstone_wire_corner_and_cross() {
    let mut baker = ModelBaker::new();
    let def = get_test_redstone_def();

    // 1. Corner (angled) includes dot + side elements
    let mut baked_corner = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=side,north=side,south=none,west=none]",
            Some(&def),
            get_test_redstone_model,
        )
        .unwrap();
    let (_, textures_corner) = baked_corner.to_mesh_with_textures(false);
    assert!(textures_corner
        .iter()
        .any(|t| t.contains("redstone_dust_dot")));
    assert!(textures_corner
        .iter()
        .any(|t| t.contains("redstone_dust_line0")));
    assert!(textures_corner
        .iter()
        .any(|t| t.contains("redstone_dust_line1")));

    let removed_corner = baked_corner.deduplicate_faces();
    assert_eq!(
        removed_corner, 0,
        "deduplicate_faces must not remove 2D planar arms in corner wire"
    );
    let (mesh_corner, _) = baked_corner.to_mesh_with_textures(true);
    assert_eq!(
        mesh_corner.face_count(),
        3,
        "Corner wire must have 3 faces (dot + 2 arms)"
    );

    // 2. Four-way cross includes dot + all 4 sides
    let mut baked_cross = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=side,north=side,south=side,west=side]",
            Some(&def),
            get_test_redstone_model,
        )
        .unwrap();
    let (_, textures_cross) = baked_cross.to_mesh_with_textures(false);
    assert!(textures_cross
        .iter()
        .any(|t| t.contains("redstone_dust_dot")));
    assert!(textures_cross
        .iter()
        .any(|t| t.contains("redstone_dust_line0")));
    assert!(textures_cross
        .iter()
        .any(|t| t.contains("redstone_dust_line1")));
    assert_eq!(baked_cross.elements.len(), 10); // 2 elements * 5 multipart matches

    let removed_cross = baked_cross.deduplicate_faces();
    assert_eq!(
        removed_cross, 0,
        "deduplicate_faces must not remove 2D planar arms in cross wire"
    );
    let (mesh_cross, _) = baked_cross.to_mesh_with_textures(true);
    assert_eq!(
        mesh_cross.face_count(),
        5,
        "Cross wire must have 5 faces (dot + 4 arms)"
    );
}

#[test]
fn test_bake_redstone_wire_vertical_ascending_wall() {
    let mut baker = ModelBaker::new();
    let def = get_test_redstone_def();

    // Vertical ascending wire on north wall: north=up, south=side
    let baked = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=none,north=up,south=side,west=none]",
            Some(&def),
            get_test_redstone_model,
        )
        .unwrap();

    let (mesh, textures) = baked.to_mesh_with_textures(false);
    assert!(textures.iter().any(|t| t.contains("redstone_dust_line1")));

    // Check that vertical ascending element on North wall exists (from_pos[2] == 0.25, to_pos[1] == 16.0)
    let has_vertical_element = baked
        .elements
        .iter()
        .any(|el| (el.from_pos[2] - 0.25).abs() < 1e-4 && (el.to_pos[1] - 16.0).abs() < 1e-4);
    assert!(
        has_vertical_element,
        "Baked model must contain the vertical wall element extending to y=16.0"
    );

    // CRITICAL: Ensure the vertical quad faces SOUTH (into the room) and NOT NORTH (into the wall)
    let south_faces: Vec<_> = baked
        .elements
        .iter()
        .flat_map(|el| el.faces.iter())
        .filter(|(&dir, _)| dir == Direction::South)
        .collect();

    assert!(
        !south_faces.is_empty(),
        "Vertical wall wire MUST keep the South face pointing into the room!"
    );

    // Verify face normals on mesh
    let has_south_normal = mesh.normals.iter().any(|n| (n[2] - 1.0).abs() < 1e-4);
    assert!(
        has_south_normal,
        "Mesh must contain South-facing normals for the vertical wire on North wall"
    );
}

#[test]
fn test_baked_model_database_redstone_power_stripping() {
    let mut db = BakedModelDatabase::new();
    let mut baker = ModelBaker::new();
    let def = get_test_redstone_def();

    // Bake pure geometric model without power: redstone_wire[east=side,west=side]
    let baked = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=side,west=side]",
            Some(&def),
            get_test_redstone_model,
        )
        .unwrap();

    db.insert(
        "minecraft:redstone_wire[east=side,west=side]".to_string(),
        baked,
    );

    // Querying with power=15 should match the prebaked model via Tier 2 non-geometric property stripping!
    let found = db.get("minecraft:redstone_wire[east=side,power=15,west=side]");
    assert!(
        found.is_some(),
        "BakedModelDatabase must match redstone_wire[east=side,west=side] when querying with power=15"
    );
}
