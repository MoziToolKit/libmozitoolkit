//! Snapshots the builtin block-model output for the block families still served by the MiEx
//! definition interpreter.
//!
//! The plan is to replace that interpreter with native generators (see
//! `docs/review/MIEX_REMOVAL_PLAN.md`). The snapshot is the visual reference: for every family that
//! has not been migrated yet it records the interpreter's output verbatim, and a native generator
//! must reproduce it exactly.
//!
//! Chest entries were updated once, on purpose: the interpreter handed the baker the client's
//! top/bottom rectangles in the client's own v order, which renders those faces upside down (the
//! chest lid needed a manual Y flip in the UV editor). The native generator flips v on up/down, so
//! the recorded chest states are the corrected output; the interpreter's version is in git history.

use mtk_model::{BlockModelJson, BlockState, BuiltinModelRegistry};

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
