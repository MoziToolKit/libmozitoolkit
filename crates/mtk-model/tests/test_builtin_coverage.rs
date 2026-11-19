//! Which block families still need a builtin model at all?
//!
//! In 26.2 the jar ships real block models for beds, signs and hanging signs, so those families must
//! resolve from the pack stack. Only the families whose client geometry is a `BlockEntityRenderer`
//! with a `particle`-only placeholder model need a generator or a definition.

use mtk_model::{BlockState, BuiltinModelRegistry};

fn builtin(state: &str) -> Option<mtk_model::BlockModelJson> {
    let parsed = BlockState::parse(state).expect("state parses");
    BuiltinModelRegistry::get_block_entity_model_json(&parsed, None)
}

/// Families the jar covers: a builtin would shadow the client's own model.
#[test]
fn jar_covered_families_have_no_builtin() {
    for state in [
        "minecraft:red_bed[facing=north,occupied=false,part=head]",
        "minecraft:red_bed[facing=north,occupied=false,part=foot]",
        "minecraft:oak_sign[rotation=0,waterlogged=false]",
        "minecraft:oak_wall_sign[facing=north,waterlogged=false]",
        "minecraft:oak_hanging_sign[attached=false,rotation=0,waterlogged=false]",
        "minecraft:oak_wall_hanging_sign[facing=east,waterlogged=false]",
    ] {
        assert!(
            builtin(state).is_none(),
            "{state} must resolve from the resource pack, not from a builtin"
        );
    }
}

/// Families that still need geometry: their jar model carries only a `particle` texture.
#[test]
fn renderer_only_families_still_have_geometry() {
    for state in [
        "minecraft:chest[facing=north,type=single,waterlogged=false]",
        "minecraft:trapped_chest[facing=south,type=single,waterlogged=false]",
        "minecraft:ender_chest[facing=west,type=left,waterlogged=false]",
        "minecraft:exposed_copper_chest[facing=north,type=right,waterlogged=false]",
        "minecraft:shulker_box[facing=up]",
        "minecraft:light_blue_shulker_box[facing=east]",
        "minecraft:skeleton_skull[rotation=0]",
        "minecraft:dragon_wall_head[facing=south]",
        "minecraft:end_portal",
    ] {
        let model = builtin(state).unwrap_or_else(|| panic!("{state} needs a builtin model"));
        assert!(
            model
                .elements
                .as_ref()
                .is_some_and(|elements| !elements.is_empty()),
            "{state} must produce geometry"
        );
    }
}
