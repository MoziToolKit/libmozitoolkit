//! Embedded vanilla fallback blockstates and JSON models strictly for blocks lacking
//! static JSON models in vanilla (bell, decorated pot, end portal) or bridging
//! model ID lookups to dynamic MiEx AST evaluations.

pub mod banner;
pub mod bell;
pub mod helpers;
pub mod pot;

use crate::builtin::loader::MiExModelLoader;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::BlockModelJson;
use crate::BakedModel;

pub use banner::create_builtin_banner_model;
pub use bell::create_builtin_bell_model;
pub use pot::{create_builtin_decorated_pot_model, create_decorated_pot_baked_model};

// Embedded BlockStates
pub const DEF_DECORATED_POT: &str =
    include_str!("../../../assets/builtins/blockstates/decorated_pot.json");
pub const DEF_END_GATEWAY: &str =
    include_str!("../../../assets/builtins/blockstates/end_gateway.json");
pub const DEF_CONDUIT: &str = include_str!("../../../assets/builtins/blockstates/conduit.json");
pub const DEF_BELL: &str = include_str!("../../../assets/builtins/blockstates/bell.json");

// Embedded Models
pub const MODEL_END_PORTAL: &str = include_str!("../../../assets/builtins/models/end_portal.json");
pub const MODEL_END_GATEWAY: &str =
    include_str!("../../../assets/builtins/models/end_gateway.json");
pub const MODEL_CONDUIT: &str = include_str!("../../../assets/builtins/models/conduit.json");

/// Retrieves an embedded builtin `BlockStateDefinition` by block name.
pub fn get_builtin_blockstate_def(name: &str) -> Option<BlockStateDefinition> {
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);

    let raw_json = match clean {
        "bell" => DEF_BELL,
        "decorated_pot" => DEF_DECORATED_POT,
        "end_gateway" => DEF_END_GATEWAY,
        "conduit" => DEF_CONDUIT,
        _ => return None,
    };

    serde_json::from_str(raw_json).ok()
}

/// Retrieves an embedded builtin `BlockModelJson` by model resource path / ID.
pub fn get_builtin_model_by_id(model_id: &str) -> Option<BlockModelJson> {
    let clean = model_id.strip_prefix("minecraft:").unwrap_or(model_id);
    let stem = clean.strip_prefix("block/").unwrap_or(clean);

    if stem.starts_with("bell_") {
        return create_builtin_bell_model(stem);
    }

    if stem == "decorated_pot" {
        return create_builtin_decorated_pot_model(None);
    }
    if stem == "end_portal" {
        return serde_json::from_str(MODEL_END_PORTAL).ok();
    }
    if stem == "end_gateway" {
        return serde_json::from_str(MODEL_END_GATEWAY).ok();
    }
    if stem == "conduit" {
        return serde_json::from_str(MODEL_CONDUIT).ok();
    }

    // Map entity block model IDs to canonical BlockStates evaluated by MiExModelLoader
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

    let bs = BlockState::parse(canonical_state).ok()?;
    MiExModelLoader::load_for_blockstate(&bs)
}

/// Fallback model for a blockstate and optional block entity data.
pub fn get_builtin_model_for_state_and_entity(
    blockstate: &BlockState,
    entity: Option<&crate::entity::BlockEntityData>,
) -> Option<BlockModelJson> {
    let clean = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);

    if clean == "decorated_pot" {
        return create_builtin_decorated_pot_model(entity);
    }
    if clean == "end_gateway" {
        return get_builtin_model_by_id("end_gateway");
    }
    if clean == "conduit" {
        return get_builtin_model_by_id("conduit");
    }
    if clean == "bell" {
        let att = blockstate
            .properties
            .get("attachment")
            .map(|s| s.as_str())
            .unwrap_or("floor");
        let model_id = match att {
            "ceiling" => "bell_ceiling",
            "single_wall" | "wall" => "bell_wall",
            "double_wall" => "bell_between_walls",
            _ => "bell_floor",
        };
        return get_builtin_model_by_id(model_id);
    }
    if clean.ends_with("_banner") {
        return create_builtin_banner_model(blockstate, entity);
    }
    None
}

/// Pre-baked direct model synthesizer for entity blocks (e.g. Copper Golem Statue, Decorated Pot).
pub fn get_builtin_baked_model_for_state_and_entity(
    blockstate: &BlockState,
    entity: Option<&crate::entity::BlockEntityData>,
) -> Option<BakedModel> {
    let clean = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);

    if clean == "copper_golem_statue" || clean.ends_with("_copper_golem_statue") {
        return Some(crate::entity::copper_statue::create_copper_statue_baked_model(blockstate));
    }

    if clean == "decorated_pot" {
        if let Some(crate::entity::BlockEntityData::DecoratedPot { sherds }) = entity {
            let facing = blockstate
                .properties
                .get("facing")
                .and_then(|f| mtk_core::Direction::parse_loose(f))
                .unwrap_or(mtk_core::Direction::North);

            return Some(create_decorated_pot_baked_model(facing, sherds));
        }
    }
    None
}
