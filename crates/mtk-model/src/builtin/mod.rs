//! Built-in geometry for block kinds whose client renderer draws them instead of a block model.
//!
//! - `props`: procedural generators, one per family (chest, shulker box, skull, end portal, copper
//!   golem statue). A generator produces the same `BlockModelJson` the resource pack would, so the
//!   ordinary model baker handles it.
//! - `vanilla`: plain BlockModel/blockstate JSON for blocks the jar still ships without geometry
//!   (bells, conduits, decorated pots, end gateways).

pub(crate) mod props;
pub(crate) mod vanilla;

use crate::baker::BakedModel;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::BlockModelJson;

/// Builtin geometry registry consulted only when the resource pack has no model for a blockstate.
pub struct BuiltinModelRegistry;

impl BuiltinModelRegistry {
    /// Attempts to provide an optimized pre-baked model for an entity block.
    pub fn get_baked_block_entity_model(
        blockstate: &BlockState,
        entity: Option<&crate::block_entity::BlockEntityData>,
    ) -> Option<BakedModel> {
        vanilla::get_builtin_baked_block_entity_model(blockstate, entity)
    }

    /// Attempts to provide builtin geometry for a block that has no model in the resource pack.
    pub fn get_block_entity_model_json(
        blockstate: &BlockState,
        entity: Option<&crate::block_entity::BlockEntityData>,
    ) -> Option<BlockModelJson> {
        if let Some(model) = crate::builtin::props::chest::build_chest_model(blockstate) {
            return Some(model);
        }
        if let Some(model) = crate::builtin::props::shulker::build_shulker_model(blockstate) {
            return Some(model);
        }
        if let Some(model) = crate::builtin::props::skull::build_skull_model(blockstate, entity) {
            return Some(model);
        }
        if let Some(model) = crate::builtin::props::end_portal::build_end_portal_model(blockstate) {
            return Some(model);
        }
        vanilla::get_builtin_block_entity_model_json(blockstate, entity)
    }

    /// Universal block entity baker: synthesizes or bakes a dynamic block entity model.
    pub fn bake_block_entity(
        state_str: &str,
        entity: &crate::block_entity::BlockEntityData,
    ) -> Option<BakedModel> {
        let blockstate = BlockState::parse(state_str).ok()?;
        if let Some(baked) = Self::get_baked_block_entity_model(&blockstate, Some(entity)) {
            return Some(baked);
        }
        let mut baker = crate::baker::ModelBaker::new();
        baker
            .bake_blockstate_with_entity(state_str, Some(entity), None, |_| None)
            .ok()
    }

    /// Attempts to provide a builtin fallback `BlockStateDefinition` by block name.
    pub fn get_builtin_blockstate_def(name: &str) -> Option<BlockStateDefinition> {
        vanilla::get_builtin_blockstate_def(name)
    }

    /// Attempts to provide a builtin fallback `BlockModelJson` by model resource path / ID.
    pub fn get_builtin_model_by_id(model_id: &str) -> Option<BlockModelJson> {
        vanilla::get_builtin_model_by_id(model_id)
    }
}
