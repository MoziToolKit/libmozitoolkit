//! Snapshots the builtin block-model output for the block families still served by the MiEx
//! definition interpreter.
//!
//! The plan is to replace that interpreter with native generators (see
//! `docs/review/MIEX_REMOVAL_PLAN.md`). Each family's states are listed in `MIGRATED` once its
//! native generator lands; the snapshot then guards the remaining families against silent drift
//! while the migration is in flight.

use mtk_model::{BlockModelJson, BlockState, BuiltinModelRegistry};

/// States already served by a native Rust generator (no longer compared to the MiEx snapshot).
const MIGRATED: &[&str] = &[
    // P1: chest now comes from builtin::props::chest, derived from ChestModel/ChestRenderer.
    "minecraft:chest[facing=north,type=single,waterlogged=false]",
    "minecraft:chest[facing=east,type=left,waterlogged=false]",
    "minecraft:trapped_chest[facing=south,type=single,waterlogged=false]",
    "minecraft:ender_chest[facing=west,type=single,waterlogged=false]",
];

#[test]
fn builtin_models_match_the_miex_snapshot() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/builtin_models_golden.json"
    );
    let golden: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture).expect("golden fixture"))
            .expect("golden parses");

    let mut mismatches = Vec::new();
    for (state, expected) in golden.as_object().expect("object") {
        if MIGRATED.contains(&state.as_str()) {
            continue;
        }
        // Compare the model structs, not the JSON text: the fields are `f32`, so both sides agree
        // exactly, while a text round-trip can disagree in the last printed digit.
        let expected: Option<BlockModelJson> =
            (!expected.is_null()).then(|| serde_json::from_value(expected.clone()).expect("model"));
        let parsed = BlockState::parse(state).expect("state parses");
        let actual = BuiltinModelRegistry::get_block_entity_model_json(&parsed, None);
        if actual != expected {
            mismatches.push(state.clone());
        }
    }

    assert!(
        mismatches.is_empty(),
        "builtin model output drifted for {mismatches:?}; update the fixture only when the change \
         is intentional"
    );
}
