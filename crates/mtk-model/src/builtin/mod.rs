//! Built-in fallback BlockStates and Models for standard vanilla entity blocks.
//!
//! Provides clean, Blockbench-compatible standard Minecraft BlockModel JSONs and
//! BlockState definitions for blocks lacking static models in vanilla Java JARs
//! (Chests, Beds, Shulker Boxes, Signs, Bells, Skulls, etc.).

pub(crate) mod evaluator;
pub(crate) mod loader;
pub(crate) mod patch;
pub(crate) mod vanilla;

use loader::MiExModelLoader;

use crate::baker::BakedModel;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::BlockModelJson;

/// Universal Builtin Model Registry driving dynamic models from raw upstream MiEx JSON definitions
/// and embedded vanilla fallback definitions.
pub struct BuiltinModelRegistry;

impl BuiltinModelRegistry {
    /// Attempts to provide a builtin fallback `BlockModelJson` using upstream MiEx JSON templates.
    pub fn get_builtin_model(blockstate: &BlockState) -> Option<BlockModelJson> {
        Self::get_builtin_model_with_entity(blockstate, None)
    }

    /// Attempts to provide an optimized pre-baked model for an entity block.
    pub fn get_builtin_baked_model_with_entity(
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
    ) -> Option<BakedModel> {
        vanilla::get_builtin_baked_model_for_state_and_entity(blockstate, entity)
    }

    /// Attempts to provide a builtin fallback `BlockModelJson` using upstream MiEx JSON templates
    /// or procedural fallbacks, enriched with dynamic entity data.
    pub fn get_builtin_model_with_entity(
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
    ) -> Option<BlockModelJson> {
        if let Some(model) = MiExModelLoader::load_for_blockstate_with_entity(blockstate, entity) {
            return Some(model);
        }
        vanilla::get_builtin_model_for_state_and_entity(blockstate, entity)
    }

    /// Universal block entity baker: synthesizes or bakes a dynamic block entity model.
    pub fn bake_block_entity(
        state_str: &str,
        entity: &crate::entity::BlockEntityData,
    ) -> Option<BakedModel> {
        let blockstate = BlockState::parse(state_str).ok()?;
        if let Some(baked) = Self::get_builtin_baked_model_with_entity(&blockstate, Some(entity)) {
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
