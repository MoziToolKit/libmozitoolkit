//! Multi-threaded block model baking for asset precompilation.

use std::collections::HashMap;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use mtk_core::progress::{ProgressCallback, ProgressReport, ProgressThrottler};
use mtk_material::MaterialResolver;
use mtk_model::{BakedModelDatabase, BlockModelJson, BlockStateDefinition, ModelBaker};
use mtk_resource::{ResourceLocation, ResourcePackStack};
use mtk_texture::atlas::AtlasAddressMap;

use crate::MtkError;

#[inline]
fn is_dynamic_entity_texture(tex: &str) -> bool {
    tex.starts_with("minecraft:banner/")
        || tex.starts_with("banner/")
        || tex.starts_with("minecraft:skin/")
        || tex.starts_with("skin/")
        || tex.starts_with("minecraft:player/")
        || tex.starts_with("player/")
}

/// Prebakes all blockstates discovered across all active resource packs in the stack with physical progress reporting.
pub fn prebake_all_models_with_progress(
    stack: &ResourcePackStack,
    atlas_map: Option<&AtlasAddressMap>,
    progress_callback: Option<ProgressCallback>,
) -> Result<BakedModelDatabase, MtkError> {
    let blockstate_locs = stack.list_all_blockstate_locations();

    if let Some(cb) = progress_callback {
        cb(ProgressReport::new(
            "prebake_models",
            0,
            blockstate_locs.len().max(1),
            "Indexing model definitions...",
        ));
    }

    // 1. Preload all model JSONs across packs into a fast lookup map
    let mut model_cache: HashMap<String, BlockModelJson> = HashMap::new();
    for pack in stack.packs().iter().rev() {
        for file in pack.list_files("assets/") {
            if file.ends_with(".json") && file.contains("/models/") {
                if let Some(bytes) = pack.open(&file) {
                    if let Ok(model) = serde_json::from_slice::<BlockModelJson>(&bytes) {
                        if let Some(loc) =
                            ResourceLocation::from_asset_path(&file, "models", "json")
                        {
                            let canon = loc.as_string();
                            model_cache.insert(canon.clone(), model.clone());
                            if loc.namespace == "minecraft" {
                                model_cache.insert(loc.path.clone(), model.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    // Helper closure to look up models with unified candidate paths
    let get_model = |model_id: &str| -> Option<BlockModelJson> {
        if let Some(m) = model_cache.get(model_id) {
            return Some(m.clone());
        }
        for path in ResourceLocation::model_candidate_asset_paths(model_id) {
            if let Some(loc) = ResourceLocation::from_asset_path(&path, "models", "json") {
                if let Some(m) = model_cache
                    .get(&loc.as_string())
                    .or_else(|| model_cache.get(&loc.path))
                {
                    return Some(m.clone());
                }
            }
            if let Some(bytes) = stack.open_asset_raw(&path) {
                if let Ok(m) = serde_json::from_slice::<BlockModelJson>(&bytes) {
                    return Some(m);
                }
            }
        }
        None
    };

    // 2. Pre-resolve all discovered model JSON definitions into an immutable shared map
    #[cfg(feature = "parallel")]
    let resolved_map: HashMap<String, std::sync::Arc<mtk_model::ResolvedBlockModel>> = {
        model_cache
            .par_iter()
            .filter_map(|(id, model)| {
                model
                    .resolve_hierarchy(id, |parent_id| get_model(parent_id))
                    .ok()
                    .map(|r| (id.clone(), std::sync::Arc::new(r)))
            })
            .collect()
    };

    #[cfg(not(feature = "parallel"))]
    let resolved_map: HashMap<String, std::sync::Arc<mtk_model::ResolvedBlockModel>> = model_cache
        .iter()
        .filter_map(|(id, model)| {
            model
                .resolve_hierarchy(id, |parent_id| get_model(parent_id))
                .ok()
                .map(|r| (id.clone(), std::sync::Arc::new(r)))
        })
        .collect();

    let shared_resolved = std::sync::Arc::new(resolved_map);

    // 3. Pre-load BlockStateDefinitions for all discovered locations
    let mut blockstate_defs: Vec<(ResourceLocation, BlockStateDefinition)> = Vec::new();
    for loc in &blockstate_locs {
        let path = loc.to_asset_path("blockstates", "json");
        if let Some(bytes) = stack.open_asset_raw(&path) {
            if let Ok(def) = serde_json::from_slice::<BlockStateDefinition>(&bytes) {
                blockstate_defs.push((loc.clone(), def));
            }
        }
    }

    let total_defs = blockstate_defs.len();
    let throttler = ProgressThrottler::new("prebake_models", total_defs.max(1), progress_callback)
        .with_prefix("Baking block models");

    // 4. Bake all states concurrently
    #[cfg(feature = "parallel")]
    let baked_pairs: Vec<(String, mtk_model::BakedVariantGroup)> = blockstate_defs
        .par_iter()
        .flat_map(|(loc, def)| {
            let states = def.enumerate_all_states(&loc.as_string());
            let mut local_baker =
                ModelBaker::with_shared_resolved(std::sync::Arc::clone(&shared_resolved));
            let mut remap_cache: HashMap<String, Option<([f32; 4], u16, u32)>> = HashMap::new();
            let mut pairs = Vec::new();
            for state_str in states {
                if let Ok(mut group) =
                    local_baker.bake_blockstate_variants(&state_str, Some(def), |id| get_model(id))
                {
                    if let Some(atlas) = atlas_map {
                        group.remap_to_atlas_with(|tex| {
                            if is_dynamic_entity_texture(tex) {
                                return None;
                            }
                            if let Some(hit) = remap_cache.get(tex) {
                                *hit
                            } else {
                                let resolved =
                                    MaterialResolver::resolve(tex, None, atlas).map(|(_, sp)| {
                                        (sp.frame_0_uv_bounds, sp.chunk_id, sp.texture_id)
                                    });
                                remap_cache.insert(tex.to_string(), resolved);
                                resolved
                            }
                        });
                    }
                    group.deduplicate_faces();
                    pairs.push((state_str, group));
                }
            }
            throttler.inc();
            pairs
        })
        .collect();

    #[cfg(not(feature = "parallel"))]
    let baked_pairs: Vec<(String, mtk_model::BakedVariantGroup)> = blockstate_defs
        .iter()
        .flat_map(|(loc, def)| {
            let states = def.enumerate_all_states(&loc.as_string());
            let mut local_baker =
                ModelBaker::with_shared_resolved(std::sync::Arc::clone(&shared_resolved));
            let mut remap_cache: HashMap<String, Option<([f32; 4], u16, u32)>> = HashMap::new();
            let mut pairs = Vec::new();
            for state_str in states {
                if let Ok(mut group) =
                    local_baker.bake_blockstate_variants(&state_str, Some(def), |id| get_model(id))
                {
                    if let Some(atlas) = atlas_map {
                        group.remap_to_atlas_with(|tex| {
                            if is_dynamic_entity_texture(tex) {
                                return None;
                            }
                            if let Some(hit) = remap_cache.get(tex) {
                                *hit
                            } else {
                                let resolved =
                                    MaterialResolver::resolve(tex, None, atlas).map(|(_, sp)| {
                                        (sp.frame_0_uv_bounds, sp.chunk_id, sp.texture_id)
                                    });
                                remap_cache.insert(tex.to_string(), resolved);
                                resolved
                            }
                        });
                    }
                    group.deduplicate_faces();
                    pairs.push((state_str, group));
                }
            }
            throttler.inc();
            pairs
        })
        .collect();

    let mut db = BakedModelDatabase::new();
    for (state_str, group) in baked_pairs {
        db.insert_variant_group(state_str, group);
    }
    db.deduplicate_all();

    if let Some(cb) = progress_callback {
        cb(ProgressReport::new(
            "prebake_models",
            total_defs,
            total_defs.max(1),
            format!("Completed baking {} model variants", db.len()),
        ));
    }

    Ok(db)
}

/// Prebakes all blockstates discovered across all active resource packs in the stack.
pub fn prebake_all_models(
    stack: &ResourcePackStack,
    atlas_map: Option<&AtlasAddressMap>,
) -> Result<BakedModelDatabase, MtkError> {
    prebake_all_models_with_progress(stack, atlas_map, None)
}
