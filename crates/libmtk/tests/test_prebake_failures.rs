//! Prebake must report every model it could not use, for blocks as well as items.
//!
//! Before this, `if let Ok(..)` chains dropped unparsable blockstate/model JSON and failed
//! block bakes silently, so a host had no way to tell "this block is missing" from "this
//! block has no model".

use mtk_resource::{MemoryPack, ResourcePackStack};

fn stack_with_broken_inputs() -> ResourcePackStack {
    let mut pack = MemoryPack::new("BrokenInputs");

    // 1. Model JSON that parses as JSON but not as a block model.
    pack.insert(
        "assets/minecraft/models/block/broken_json.json",
        br#"{ "elements": "definitely-not-an-array" }"#.to_vec(),
    );

    // 2. Circular parent hierarchy: parses fine, cannot be resolved.
    pack.insert(
        "assets/minecraft/models/block/circular_a.json",
        br#"{ "parent": "minecraft:block/circular_b" }"#.to_vec(),
    );
    pack.insert(
        "assets/minecraft/models/block/circular_b.json",
        br#"{ "parent": "minecraft:block/circular_a" }"#.to_vec(),
    );

    // 3. Blockstate JSON that is not valid JSON at all.
    pack.insert(
        "assets/minecraft/blockstates/broken_blockstate.json",
        b"{ this is not json".to_vec(),
    );

    // 4. A healthy block so the run still produces something.
    pack.insert(
        "assets/minecraft/models/block/fine.json",
        br#"{ "elements": [] }"#.to_vec(),
    );
    pack.insert(
        "assets/minecraft/blockstates/fine.json",
        br#"{ "variants": { "": { "model": "minecraft:block/fine" } } }"#.to_vec(),
    );

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));
    stack
}

#[test]
fn prebake_reports_unparsable_and_failed_blocks() {
    let db = libmtk::prebake_all_models(&stack_with_broken_inputs(), None).expect("prebake");
    let failures = db.bake_failures();
    let lookup = |key: &str| {
        failures
            .iter()
            .find(|failure| failure.key == key)
            .map(|failure| failure.reason.as_str())
    };

    assert_eq!(
        lookup("minecraft:block/broken_json"),
        Some("invalid_model_json"),
        "unparsable model JSON must be reported, got {failures:?}"
    );
    assert_eq!(
        lookup("minecraft:broken_blockstate"),
        Some("invalid_blockstate_json"),
        "unparsable blockstate JSON must be reported, got {failures:?}"
    );
    assert_eq!(
        lookup("minecraft:block/circular_a"),
        Some("circular_parent_hierarchy"),
        "a circular parent chain must be reported, got {failures:?}"
    );

    // The healthy block must still be baked and must not be reported as a failure.
    assert!(
        db.get("minecraft:fine").is_some(),
        "healthy blockstate must still bake"
    );
    assert!(
        !failures
            .iter()
            .any(|failure| failure.key.contains("minecraft:fine")),
        "healthy block must not be reported, got {failures:?}"
    );
}

#[test]
fn duplicate_failure_reports_keep_the_first_reason() {
    use mtk_model::BakedModelDatabase;

    let mut db = BakedModelDatabase::new();
    db.push_bake_failure("minecraft:block/x", "invalid_model_json");
    db.push_bake_failure("minecraft:block/x", "bake_failed");

    assert_eq!(db.bake_failures().len(), 1);
    assert_eq!(db.bake_failures()[0].reason, "invalid_model_json");
}
