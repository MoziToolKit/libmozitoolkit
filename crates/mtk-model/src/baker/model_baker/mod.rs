pub mod assembler;
pub mod resolver;

#[cfg(test)]
use mtk_core::Direction;

#[cfg(test)]
#[path = "../model_baker_tests.rs"]
mod tests;

use std::collections::HashMap;

use crate::baker::baked_model::{BakedModel, BakedVariantGroup};
use crate::builtin::BuiltinModelRegistry;
use crate::error::ModelError;
use crate::parser::blockstate::{
    BlockState, BlockStateDefinition, BlockStateResolver, VariantMatch,
};
use crate::parser::model_json::{BlockModelJson, ResolvedBlockModel};

/// Universal, headless Minecraft Model Baker.
///
/// Bakes arbitrary BlockStates and models (both native Minecraft JSON models and Mod Wavefront OBJ meshes)
/// into fully realized geometry completely independent of Blender or any external 3D runtime.
#[derive(Default, Clone)]
pub struct ModelBaker {
    /// In-memory cache for baked models keyed by canonical BlockState string.
    bake_cache: HashMap<String, BakedModel>,
    /// In-memory cache for baked variant groups keyed by canonical BlockState string.
    variant_cache: HashMap<String, BakedVariantGroup>,
    /// Thread-local cache for dynamically resolved model hierarchies.
    resolved_cache: HashMap<String, std::sync::Arc<ResolvedBlockModel>>,
    /// Shared pre-resolved model database (zero-copy read across threads).
    shared_resolved: Option<std::sync::Arc<HashMap<String, std::sync::Arc<ResolvedBlockModel>>>>,
}

impl ModelBaker {
    /// Creates a new `ModelBaker` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `ModelBaker` sharing a pre-resolved model database.
    pub fn with_shared_resolved(
        shared: std::sync::Arc<HashMap<String, std::sync::Arc<ResolvedBlockModel>>>,
    ) -> Self {
        Self {
            bake_cache: HashMap::new(),
            variant_cache: HashMap::new(),
            resolved_cache: HashMap::new(),
            shared_resolved: Some(shared),
        }
    }

    /// Clears the internal bake cache and resolved model cache.
    pub fn clear_cache(&mut self) {
        self.bake_cache.clear();
        self.variant_cache.clear();
        self.resolved_cache.clear();
    }

    /// Bakes a BlockState into a `BakedModel` using supplied JSON definitions and model loaders.
    ///
    /// - `state_str`: E.g. `"minecraft:oak_stairs[facing=east,half=bottom,shape=straight]"`
    /// - `blockstate_def`: Optional `BlockStateDefinition` (variants or multipart). If None, uses heuristic.
    /// - `model_loader`: Callback retrieving raw `BlockModelJson` given a model ID path.
    pub fn bake_blockstate<F>(
        &mut self,
        state_str: &str,
        blockstate_def: Option<&BlockStateDefinition>,
        model_loader: F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        self.bake_blockstate_with_entity(state_str, None, blockstate_def, model_loader)
    }

    /// Bakes a BlockState with optional `BlockEntityData` into a `BakedModel`.
    pub fn bake_blockstate_with_entity<F>(
        &mut self,
        state_str: &str,
        entity: Option<&crate::entity::BlockEntityData>,
        blockstate_def: Option<&BlockStateDefinition>,
        mut model_loader: F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let blockstate = BlockState::parse(state_str)?;
        let canonical_str = blockstate.to_canonical_string();

        let cache_key = match entity {
            Some(e) => format!("{}:{:?}", canonical_str, e),
            None => canonical_str.clone(),
        };

        if let Some(cached) = self.bake_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        if entity.is_none() {
            if let Some(group) = self.variant_cache.get(&canonical_str) {
                let primary = group.select_primary().clone();
                self.bake_cache.insert(cache_key, primary.clone());
                return Ok(primary);
            }
        }

        if let Some(baked) =
            BuiltinModelRegistry::get_builtin_baked_model_with_entity(&blockstate, entity)
        {
            self.bake_cache.insert(cache_key, baked.clone());
            return Ok(baked);
        }

        let variant_matches = if let Some(def) = blockstate_def {
            BlockStateResolver::resolve(def, &blockstate)
        } else if let Some(builtin_def) =
            BuiltinModelRegistry::get_builtin_blockstate_def(&blockstate.name)
        {
            BlockStateResolver::resolve(&builtin_def, &blockstate)
        } else {
            vec![BlockStateResolver::heuristic_match(&blockstate)]
        };

        let baked_model = self.bake_matches_with_entity(
            &canonical_str,
            &blockstate,
            entity,
            &variant_matches,
            &mut model_loader,
        )?;
        self.bake_cache.insert(cache_key, baked_model.clone());
        Ok(baked_model)
    }

    /// Bakes all discrete variants of a BlockState into a `BakedVariantGroup`.
    pub fn bake_blockstate_variants<F>(
        &mut self,
        state_str: &str,
        blockstate_def: Option<&BlockStateDefinition>,
        mut model_loader: F,
    ) -> Result<BakedVariantGroup, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let blockstate = BlockState::parse(state_str)?;
        let canonical_str = blockstate.to_canonical_string();

        if let Some(cached) = self.variant_cache.get(&canonical_str) {
            return Ok(cached.clone());
        }

        if let Some(baked) =
            BuiltinModelRegistry::get_builtin_baked_model_with_entity(&blockstate, None)
        {
            let group = BakedVariantGroup::new(canonical_str.clone(), vec![baked.clone()], vec![1]);
            self.variant_cache
                .insert(canonical_str.clone(), group.clone());
            self.bake_cache.insert(canonical_str, baked);
            return Ok(group);
        }

        let variant_specs = if let Some(def) = blockstate_def {
            BlockStateResolver::resolve_variants(def, &blockstate)
        } else if let Some(builtin_def) =
            BuiltinModelRegistry::get_builtin_blockstate_def(&blockstate.name)
        {
            BlockStateResolver::resolve_variants(&builtin_def, &blockstate)
        } else {
            vec![(vec![BlockStateResolver::heuristic_match(&blockstate)], 1)]
        };

        let mut models = Vec::with_capacity(variant_specs.len());
        let mut weights = Vec::with_capacity(variant_specs.len());

        for (matches, weight) in variant_specs {
            let model =
                self.bake_matches(&canonical_str, &blockstate, &matches, &mut model_loader)?;
            models.push(model);
            weights.push(weight);
        }

        let group = BakedVariantGroup::new(canonical_str.clone(), models, weights);
        self.variant_cache.insert(canonical_str, group.clone());
        Ok(group)
    }

    pub(crate) fn bake_matches<F>(
        &mut self,
        canonical_str: &str,
        blockstate: &BlockState,
        variant_matches: &[VariantMatch],
        model_loader: &mut F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        self.bake_matches_with_entity(
            canonical_str,
            blockstate,
            None,
            variant_matches,
            model_loader,
        )
    }
}
