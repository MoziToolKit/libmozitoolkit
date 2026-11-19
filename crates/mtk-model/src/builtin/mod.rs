//! Builtin geometry for blocks whose client renderer draws them instead of a block model.
//!
//! Two modules, split by *how* the geometry is produced:
//!
//! - [`generators`]: Rust code that computes a `BlockModelJson` from the blockstate (and, for block
//!   entities, from their data). This is where the client's `BlockEntityRenderer` geometry lives.
//! - [`json`]: embedded Minecraft block-model/blockstate JSON for the two families that have no
//!   generator yet (conduit, end gateway). Same format as a resource pack, nothing interpreted.
//!
//! The registry below is consulted **only** when the configured pack has no model for a blockstate;
//! families the jar covers (beds, signs, hanging signs, bells, fences, ...) never reach it.

pub(crate) mod generators;
pub(crate) mod json;

use crate::baker::BakedModel;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::BlockModelJson;

/// Builtin geometry registry.
pub struct BuiltinModelRegistry;

impl BuiltinModelRegistry {
    /// Pre-baked geometry for blocks whose mesh is a pure function of state + block entity data.
    pub fn get_baked_block_entity_model(
        blockstate: &BlockState,
        entity: Option<&crate::block_entity::BlockEntityData>,
    ) -> Option<BakedModel> {
        generators::get_baked_block_entity_model(blockstate, entity)
    }

    /// Bakes the geometry of a block entity's host block.
    ///
    /// Used by the section mesher when a section contains block entity data: the mesh has to
    /// reflect that data (sherd patterns, banner layers, skull profiles), which no static model in
    /// the pack can express.
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

    /// Synthesized `BlockModelJson` for a blockstate (and block entity, when relevant).
    pub fn get_block_entity_model_json(
        blockstate: &BlockState,
        entity: Option<&crate::block_entity::BlockEntityData>,
    ) -> Option<BlockModelJson> {
        if let Some(model) = generators::chest::build_chest_model(blockstate) {
            return Some(model);
        }
        if let Some(model) = generators::shulker::build_shulker_model(blockstate) {
            return Some(model);
        }
        if let Some(model) = generators::skull::build_skull_model(blockstate, entity) {
            return Some(model);
        }
        if let Some(model) = generators::end_portal::build_end_portal_model(blockstate) {
            return Some(model);
        }

        let clean = blockstate
            .name
            .strip_prefix("minecraft:")
            .unwrap_or(&blockstate.name);
        if clean == "decorated_pot" {
            return generators::decorated_pot::create_builtin_decorated_pot_model(entity);
        }
        if clean.ends_with("_banner") {
            return generators::banner::create_builtin_banner_model(blockstate, entity);
        }
        if clean == "bell" {
            let attachment = blockstate
                .properties
                .get("attachment")
                .map(String::as_str)
                .unwrap_or("floor");
            let model_id = match attachment {
                "ceiling" => "bell_ceiling",
                "single_wall" | "wall" => "bell_wall",
                "double_wall" => "bell_between_walls",
                _ => "bell_floor",
            };
            return generators::bell::create_builtin_bell_model(model_id);
        }
        if clean == "end_gateway" || clean == "conduit" {
            return json::get_builtin_model_by_id(clean);
        }
        None
    }

    /// Embedded blockstate definition for packs that ship the models without the blockstate.
    pub fn get_builtin_blockstate_def(name: &str) -> Option<BlockStateDefinition> {
        json::get_builtin_blockstate_def(name)
    }

    /// Embedded model lookup by resource id, used when a parent model is missing from the pack.
    pub fn get_builtin_model_by_id(model_id: &str) -> Option<BlockModelJson> {
        json::get_builtin_model_by_id(model_id)
    }
}
