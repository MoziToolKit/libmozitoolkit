use super::ModelBaker;
use crate::builtin::BuiltinModelRegistry;
use crate::error::ModelError;
use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{BlockModelJson, ResolvedBlockModel};

impl ModelBaker {
    pub(crate) fn resolve_variant_model_with_entity<F>(
        &mut self,
        model_id: &str,
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
        allow_blockstate_builtin: bool,
        model_loader: &mut F,
    ) -> Result<std::sync::Arc<ResolvedBlockModel>, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let clean_block = blockstate
            .name
            .strip_prefix("minecraft:")
            .unwrap_or(&blockstate.name);
        let is_bell = clean_block == "bell";
        let has_entity = entity.is_some();

        // Fast path: check shared pre-resolved cache and local cache (only when no entity data)
        if !is_bell && !has_entity {
            if let Some(ref shared) = self.shared_resolved {
                if let Some(r) = shared.get(model_id) {
                    if !r.elements.is_empty() {
                        return Ok(std::sync::Arc::clone(r));
                    }
                }
            }
            if let Some(r) = self.resolved_cache.get(model_id) {
                if !r.elements.is_empty() {
                    return Ok(std::sync::Arc::clone(r));
                }
            }
        }

        // Full resolution path
        let mut can_cache = false;
        let r = if let Some(external) = model_loader(model_id) {
            let res = external.resolve_hierarchy(model_id, |id| {
                model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
            })?;
            let is_missing_bell_body = is_bell && !res.textures.contains_key("bell_body");
            if res.elements.is_empty() || is_missing_bell_body {
                if allow_blockstate_builtin {
                    if let Some(builtin) =
                        BuiltinModelRegistry::get_builtin_model_with_entity(blockstate, entity)
                    {
                        builtin.resolve_hierarchy(model_id, |id| {
                            model_loader(id)
                                .or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                        })?
                    } else if let Some(builtin) =
                        BuiltinModelRegistry::get_builtin_model_by_id(model_id)
                    {
                        builtin.resolve_hierarchy(model_id, |id| {
                            model_loader(id)
                                .or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                        })?
                    } else {
                        res
                    }
                } else if let Some(builtin) =
                    BuiltinModelRegistry::get_builtin_model_by_id(model_id)
                {
                    builtin.resolve_hierarchy(model_id, |id| {
                        model_loader(id)
                            .or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                    })?
                } else {
                    res
                }
            } else {
                can_cache = true;
                res
            }
        } else if allow_blockstate_builtin {
            if let Some(builtin) =
                BuiltinModelRegistry::get_builtin_model_with_entity(blockstate, entity)
            {
                builtin.resolve_hierarchy(model_id, |id| {
                    model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                })?
            } else if let Some(builtin) = BuiltinModelRegistry::get_builtin_model_by_id(model_id) {
                builtin.resolve_hierarchy(model_id, |id| {
                    model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                })?
            } else {
                ResolvedBlockModel::default()
            }
        } else if let Some(builtin) = BuiltinModelRegistry::get_builtin_model_by_id(model_id) {
            builtin.resolve_hierarchy(model_id, |id| {
                model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
            })?
        } else {
            ResolvedBlockModel::default()
        };

        let arc_r = std::sync::Arc::new(r);
        if can_cache && !is_bell && !arc_r.elements.is_empty() {
            self.resolved_cache
                .insert(model_id.to_string(), std::sync::Arc::clone(&arc_r));
        }
        Ok(arc_r)
    }
}
