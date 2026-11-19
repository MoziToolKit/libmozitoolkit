//! Embedded Minecraft JSON for the two families whose geometry the jar ships as a `particle`-only
//! placeholder and which have no Rust generator: **conduit** and **end gateway**.
//!
//! These are ordinary block-model / blockstate JSON files (the same format the resource pack uses),
//! not a dialect: nothing here is interpreted at runtime beyond the normal baker. Geometry that is
//! computed in code lives in [`crate::builtin::generators`].

use crate::parser::blockstate::BlockStateDefinition;
use crate::parser::model_json::BlockModelJson;

// Embedded BlockStates (fallbacks for packs that ship only the models).
pub const DEF_DECORATED_POT: &str =
    include_str!("../../../assets/builtins/blockstates/decorated_pot.json");
pub const DEF_END_GATEWAY: &str =
    include_str!("../../../assets/builtins/blockstates/end_gateway.json");
pub const DEF_CONDUIT: &str = include_str!("../../../assets/builtins/blockstates/conduit.json");

// Embedded Models.
pub const MODEL_END_GATEWAY: &str =
    include_str!("../../../assets/builtins/models/end_gateway.json");
pub const MODEL_CONDUIT: &str = include_str!("../../../assets/builtins/models/conduit.json");

/// Retrieves an embedded builtin `BlockStateDefinition` by block name.
pub fn get_builtin_blockstate_def(name: &str) -> Option<BlockStateDefinition> {
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);

    let raw_json = match clean {
        "decorated_pot" => DEF_DECORATED_POT,
        "end_gateway" => DEF_END_GATEWAY,
        "conduit" => DEF_CONDUIT,
        _ => return None,
    };

    serde_json::from_str(raw_json).ok()
}

/// Retrieves an embedded `BlockModelJson` by model resource path / ID.
pub fn get_builtin_model_by_id(model_id: &str) -> Option<BlockModelJson> {
    let clean = model_id.strip_prefix("minecraft:").unwrap_or(model_id);
    let stem = clean.strip_prefix("block/").unwrap_or(clean);

    if stem.starts_with("bell_") {
        return crate::builtin::generators::bell::create_builtin_bell_model(stem);
    }
    if stem == "decorated_pot" {
        return crate::builtin::generators::decorated_pot::create_builtin_decorated_pot_model(None);
    }
    if stem == "end_portal" || stem == "end_gateway" || stem == "conduit" {
        let raw = if stem == "end_gateway" {
            MODEL_END_GATEWAY
        } else {
            MODEL_CONDUIT
        };
        return serde_json::from_str(raw).ok();
    }

    // Models whose geometry comes from a renderer: evaluate the canonical blockstate through the
    // same path the baker uses, so model-id lookups find the generators too.
    let canonical_state = match stem {
        "chest" => "minecraft:chest[facing=north,type=single]",
        "chest_left" => "minecraft:chest[facing=north,type=left]",
        "chest_right" => "minecraft:chest[facing=north,type=right]",
        "trapped_chest" => "minecraft:trapped_chest[facing=north,type=single]",
        "trapped_chest_left" => "minecraft:trapped_chest[facing=north,type=left]",
        "trapped_chest_right" => "minecraft:trapped_chest[facing=north,type=right]",
        "ender_chest" => "minecraft:ender_chest[facing=north]",
        "shulker_box" => "minecraft:shulker_box[facing=up]",
        "skull" => "minecraft:skeleton_skull[rotation=0]",
        "skull_wall" => "minecraft:skeleton_wall_skull[facing=north]",
        "dragon_head" => "minecraft:dragon_head[rotation=0]",
        "piglin_head" => "minecraft:piglin_head[rotation=0]",
        _ => return None,
    };

    let bs = crate::parser::blockstate::BlockState::parse(canonical_state).ok()?;
    crate::builtin::BuiltinModelRegistry::get_block_entity_model_json(&bs, None)
}
