//! Wire-contract tests for visual entities emitted by the `yefira` server mod.
//!
//! The server builds these payloads by hand (`VisualEntitySerializer` /
//! `VisualBlockEntity`), so nothing type-checks them against `mtk-model`. These tests
//! pin the exact JSON shape the server produces and prove it still bakes correctly.

use mtk_core::Direction;
use mtk_model::{EntityInstance, EntityKindData};

/// Payload emitted by `VisualEntitySerializer.serializeItemFrame` for a decorated pot
/// (sherds are normalized to pattern stems by `VisualBlockEntity.serializeDecoratedPot`).
const DECORATED_POT_FRAME: &str = r#"{
  "uuid": "11111111-2222-3333-4444-555555555555",
  "id": "minecraft:item_frame",
  "position": [0.0, 64.0, 0.0],
  "facing": "north",
  "rotation": [0.0, 0.0, 0.0],
  "scale": [1.0, 1.0, 1.0],
  "kind": {
    "type": "minecraft:item_frame",
    "item_id": "minecraft:decorated_pot",
    "count": 1,
    "rotation": 0,
    "is_glowing": false,
    "is_invisible": false,
    "components": {
      "block_entity": {
        "id": "minecraft:decorated_pot",
        "sherds": ["angler", null, "blade", "brewer"]
      }
    }
  }
}"#;

/// Payload emitted for a banner held in a frame, including the map id field.
const BANNER_FRAME: &str = r#"{
  "uuid": "22222222-3333-4444-5555-666666666666",
  "id": "minecraft:glow_item_frame",
  "position": [2.0, 64.0, 0.0],
  "facing": "south",
  "kind": {
    "type": "minecraft:item_frame",
    "item_id": "minecraft:light_blue_banner",
    "count": 1,
    "rotation": 3,
    "is_glowing": true,
    "is_invisible": false,
    "map_id": 42,
    "components": {
      "block_entity": {
        "id": "minecraft:banner",
        "base_color": "light_blue",
        "patterns": [{"pattern": "stripe_top", "color": "red"}]
      }
    }
  }
}"#;

#[test]
fn server_item_frame_payload_deserializes_and_bakes_sherds() {
    let entity: EntityInstance =
        serde_json::from_str(DECORATED_POT_FRAME).expect("server payload must deserialize");

    let EntityKindData::ItemFrame(frame) = &entity.kind else {
        panic!("expected item frame payload");
    };
    let components = frame
        .components
        .as_ref()
        .expect("components must survive deserialization");
    assert!(
        components.block_entity.is_some(),
        "block_entity payload must be present"
    );

    let plan = mtk_model::build_entity_render_plan(
        std::slice::from_ref(&entity),
        mtk_model::EntityRenderConfig::default(),
        |texture| {
            texture
                .contains("pottery")
                .then_some(([0.0, 0.0, 0.25, 0.25], 0, 0))
        },
        |item_id| {
            // The frame's own item lookup is irrelevant here; the builtin pot model is used.
            let _ = item_id;
            None
        },
    );

    assert!(
        plan.warnings.is_empty(),
        "unexpected warnings: {:?}",
        plan.warnings
    );
    assert_eq!(plan.nodes.len(), 1);
    let routes: Vec<&str> = plan.nodes[0]
        .material_routes
        .iter()
        .map(|route| route.texture_key.as_str())
        .collect();
    for expected in [
        "minecraft:entity/decorated_pot/angler_pottery_pattern",
        "minecraft:entity/decorated_pot/blade_pottery_pattern",
        "minecraft:entity/decorated_pot/brewer_pottery_pattern",
    ] {
        assert!(
            routes.contains(&expected),
            "missing sherd route {expected}; got {routes:?}"
        );
    }
    // The left side was undecorated and must fall back to the plain pot side texture.
    assert!(
        routes.contains(&"minecraft:entity/decorated_pot/decorated_pot_side"),
        "undecorated side must keep the default pot texture; got {routes:?}"
    );
}

#[test]
fn server_banner_frame_payload_keeps_map_id_and_rotation() {
    let entity: EntityInstance =
        serde_json::from_str(BANNER_FRAME).expect("server payload must deserialize");

    let EntityKindData::ItemFrame(frame) = &entity.kind else {
        panic!("expected item frame payload");
    };
    assert_eq!(frame.map_id, Some(42));
    assert_eq!(frame.rotation, 3);
    assert!(frame.is_glowing);
    assert_eq!(entity.id, "minecraft:glow_item_frame");
    assert_eq!(entity.facing, Some(Direction::South));

    let components = frame
        .components
        .as_ref()
        .expect("components must survive deserialization");
    let block_entity = components
        .block_entity
        .as_ref()
        .expect("banner block entity must be present");
    assert!(
        matches!(block_entity, mtk_model::BlockEntityData::Banner { .. }),
        "expected Banner block entity, got {block_entity:?}"
    );

    let plan = mtk_model::build_entity_render_plan(
        std::slice::from_ref(&entity),
        mtk_model::EntityRenderConfig::default(),
        |texture| {
            texture
                .contains("banner")
                .then_some(([0.0, 0.0, 0.25, 0.25], 0, 0))
        },
        |_| None,
    );
    assert!(
        plan.warnings.is_empty(),
        "unexpected warnings: {:?}",
        plan.warnings
    );
    assert_eq!(plan.nodes.len(), 1);
}

#[test]
fn item_frame_without_components_still_bakes_frame_only() {
    let json = r#"{
      "id": "minecraft:item_frame",
      "position": [0.0, 64.0, 0.0],
      "facing": "north",
      "kind": {
        "type": "minecraft:item_frame",
        "item_id": "",
        "count": 1,
        "rotation": 0,
        "is_glowing": false,
        "is_invisible": false
      }
    }"#;
    let entity: EntityInstance = serde_json::from_str(json).expect("legacy payload shape works");
    let plan = mtk_model::build_entity_render_plan(
        std::slice::from_ref(&entity),
        mtk_model::EntityRenderConfig::default(),
        |_| Some(([0.0, 0.0, 0.25, 0.25], 0, 0)),
        |_| None,
    );
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    assert_eq!(plan.nodes.len(), 1);
    assert!(
        !plan.nodes[0].local_mesh.is_empty(),
        "an empty frame still has its own geometry"
    );
}
