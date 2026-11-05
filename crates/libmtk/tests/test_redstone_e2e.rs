use mtk_core::Direction;
use mtk_material::BiomeResolver;
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
fn test_end_to_end_redstone_wire_baking_and_material_addressing() {
    let mut baker = ModelBaker::new();
    let def = get_test_redstone_def();

    // 1. Off state (Power 0)
    let baked_off = baker
        .bake_blockstate(
            "minecraft:redstone_wire[north=side,power=0,south=side]",
            Some(&def),
            get_test_redstone_model,
        )
        .expect("Must bake redstone wire off state");

    assert!(!baked_off.is_emissive);
    let (mesh_off, _) = baked_off.to_mesh_with_textures(false);

    // Emission is baked-model metadata; retired face attributes must not be emitted.
    for name in mtk_core::RETIRED_MESH_ATTRIBUTES {
        assert!(
            mesh_off.get_custom_attribute(name).is_none(),
            "retired attribute {name} must not be emitted"
        );
    }

    // 2. On state (Power 15) with vertical wall wire (north=up, south=side)
    let baked_on = baker
        .bake_blockstate(
            "minecraft:redstone_wire[north=up,power=15,south=side]",
            Some(&def),
            get_test_redstone_model,
        )
        .expect("Must bake redstone wire on state");

    assert!(baked_on.is_emissive);
    let (mesh_on, _) = baked_on.to_mesh_with_textures(false);
    for name in mtk_core::RETIRED_MESH_ATTRIBUTES {
        assert!(
            mesh_on.get_custom_attribute(name).is_none(),
            "retired attribute {name} must not be emitted"
        );
    }

    // 3. Verify vertical wall wire faces into the room (South)
    let south_faces: Vec<_> = baked_on
        .elements
        .iter()
        .flat_map(|el| el.faces.iter())
        .filter(|(&dir, _)| dir == Direction::South)
        .collect();
    assert!(
        !south_faces.is_empty(),
        "Vertical wall wire must retain room-facing South face"
    );

    // 4. Verify BiomeResolver address lookup matches
    let resolver = BiomeResolver::new();
    let tint_info_off = resolver.get_tint_info(
        "redstone_dust_line0",
        Some("minecraft:redstone_wire[power=0]"),
        Some(0),
    );
    assert_eq!(tint_info_off.hardcoded_hex.as_deref(), Some("#4B0000"));

    let tint_info_on = resolver.get_tint_info(
        "redstone_dust_line0",
        Some("minecraft:redstone_wire[power=15]"),
        Some(0),
    );
    assert_eq!(tint_info_on.hardcoded_hex.as_deref(), Some("#FF2600"));

    // 5. Verify BakedModelDatabase Tier 2 non-geometric property stripping
    let mut db = BakedModelDatabase::new();
    let geometric = baker
        .bake_blockstate(
            "minecraft:redstone_wire[east=side,west=side]",
            Some(&def),
            get_test_redstone_model,
        )
        .unwrap();
    db.insert(
        "minecraft:redstone_wire[east=side,west=side]".to_string(),
        geometric,
    );

    let match_with_power = db.get("minecraft:redstone_wire[east=side,power=15,west=side]");
    assert!(
        match_with_power.is_some(),
        "Database must match geometry when power is specified"
    );
}
