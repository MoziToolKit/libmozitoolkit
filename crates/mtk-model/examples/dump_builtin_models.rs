//! Dumps the builtin block-model JSON for a curated set of blockstates.
//!
//! Used to snapshot the current output (MiEx interpreter) before replacing it with native Rust
//! generators, so the conversion can be verified instead of judged by eye.

use mtk_model::{BlockState, BuiltinModelRegistry};

fn main() {
    let states = [
        "minecraft:chest[facing=north,type=single,waterlogged=false]",
        "minecraft:chest[facing=east,type=left,waterlogged=false]",
        "minecraft:trapped_chest[facing=south,type=single,waterlogged=false]",
        "minecraft:ender_chest[facing=west,type=single,waterlogged=false]",
        "minecraft:red_bed[facing=north,occupied=false,part=head]",
        "minecraft:red_bed[facing=north,occupied=false,part=foot]",
        "minecraft:light_blue_bed[facing=west,occupied=true,part=head]",
        "minecraft:shulker_box[facing=up]",
        "minecraft:red_shulker_box[facing=north]",
        "minecraft:oak_sign[rotation=0,waterlogged=false]",
        "minecraft:oak_sign[rotation=9,waterlogged=false]",
        "minecraft:oak_wall_sign[facing=north,waterlogged=false]",
        "minecraft:oak_hanging_sign[attached=false,rotation=0,waterlogged=false]",
        "minecraft:oak_wall_hanging_sign[facing=east,waterlogged=false]",
        "minecraft:skeleton_skull[rotation=0]",
        "minecraft:skeleton_wall_skull[facing=north]",
        "minecraft:player_head[rotation=7]",
        "minecraft:dragon_wall_head[facing=south]",
        "minecraft:creeper_head[rotation=3]",
        "minecraft:end_portal",
    ];

    let mut out = serde_json::Map::new();
    for state in states {
        let parsed = BlockState::parse(state).expect("state must parse");
        let model = BuiltinModelRegistry::get_block_entity_model_json(&parsed, None);
        let value = match model {
            Some(model) => serde_json::to_value(&model).expect("model serializes"),
            None => serde_json::Value::Null,
        };
        out.insert(state.to_string(), value);
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Object(out)).expect("dump serializes")
    );
}
