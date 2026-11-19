//! Procedural geometry for block kinds whose client renderer draws them instead of a block model.
//!
//! Each family gets one module that returns a `BlockModelJson`, so the ordinary model baker handles
//! UVs, culling and atlas binding. Values are transcribed from the client's own model classes; the
//! conversion rules (model space, plane/v conventions) are documented in
//! `docs/review/MIEX_REMOVAL_PLAN.md`.

pub(crate) mod banner;
pub(crate) mod bell;
pub(crate) mod chest;
pub(crate) mod copper_statue;
pub(crate) mod cube;
pub(crate) mod decorated_pot;
pub(crate) mod end_portal;
pub(crate) mod helpers;
pub(crate) mod shulker;
pub(crate) mod skull;

use crate::parser::blockstate::BlockState;
use crate::BakedModel;

pub use decorated_pot::create_decorated_pot_baked_model;

/// Families that produce a ready `BakedModel` directly (their geometry bypasses the model format).
pub(crate) fn get_baked_block_entity_model(
    blockstate: &BlockState,
    entity: Option<&crate::block_entity::BlockEntityData>,
) -> Option<BakedModel> {
    let clean = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);

    if clean == "copper_golem_statue" || clean.ends_with("_copper_golem_statue") {
        return Some(copper_statue::create_copper_statue_baked_model(blockstate));
    }

    if clean == "decorated_pot" {
        if let Some(crate::block_entity::BlockEntityData::DecoratedPot { sherds }) = entity {
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
