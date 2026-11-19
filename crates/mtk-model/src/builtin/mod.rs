//! Built-in geometry for block kinds whose vanilla renderer is not a plain block model.
//!
//! Two sources live here:
//! - Blockbench-compatible BlockModel JSON and blockstate fallbacks for blocks that ship no
//!   static JSON in the vanilla JAR (chests, beds, shulker boxes, signs, bells, skulls).
//! - `props`: procedural generators for block kinds whose geometry is a function of their
//!   blockstate alone (the copper golem statue). They stay in core on purpose: the statue is a
//!   block, so no continuous-entity pack is involved.

pub(crate) mod evaluator;
pub(crate) mod loader;
pub(crate) mod patch;
pub(crate) mod props;
pub(crate) mod vanilla;

use loader::MiExModelLoader;

use crate::baker::BakedModel;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::BlockModelJson;

/// Universal Builtin Model Registry driving dynamic models from raw upstream MiEx JSON definitions
/// and embedded vanilla fallback definitions.
pub struct BuiltinModelRegistry;

impl BuiltinModelRegistry {
    /// Attempts to provide an optimized pre-baked model for an entity block.
    pub fn get_baked_block_entity_model(
        blockstate: &BlockState,
        entity: Option<&crate::block_entity::BlockEntityData>,
    ) -> Option<BakedModel> {
        vanilla::get_builtin_baked_block_entity_model(blockstate, entity)
    }

    /// Attempts to provide a builtin fallback `BlockModelJson` using upstream MiEx JSON templates
    /// or procedural fallbacks, enriched with dynamic entity data.
    pub fn get_block_entity_model_json(
        blockstate: &BlockState,
        entity: Option<&crate::block_entity::BlockEntityData>,
    ) -> Option<BlockModelJson> {
        if let Some(model) = crate::builtin::props::chest::build_chest_model(blockstate) {
            return Some(model);
        }
        if let Some(model) = MiExModelLoader::load_for_block_entity(blockstate, entity) {
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
