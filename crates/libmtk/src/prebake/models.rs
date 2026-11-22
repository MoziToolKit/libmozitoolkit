//! Multi-threaded block model baking for asset precompilation.

use std::collections::HashMap;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use mtk_core::{ProgressCallback, ProgressReport, ProgressThrottler};
use mtk_material::MaterialResolver;
use mtk_model::{bake_item_from_png_bytes, ItemMeshOptions};
use mtk_model::{BakedModelDatabase, BlockModelJson, BlockStateDefinition, ModelBaker};
use mtk_resource::{ResourceLocation, ResourcePackStack};
use mtk_texture::AtlasAddressMap;

use crate::prebake::preload::{preload_model_and_obj_assets, PreloadedModelAssets};
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

/// Maps an item model location (`minecraft:item/chest`) to the block model that carries
/// its geometry (`minecraft:block/chest`).
///
/// `builtin/entity` items declare display transforms but no elements; vanilla renders them
/// from block geometry through a block-entity renderer.
fn block_model_id_for_item(loc: &ResourceLocation) -> Option<String> {
    let name = loc.path.strip_prefix("item/")?;
    Some(format!("{}:block/{}", loc.namespace, name))
}

#[inline]
fn open_item_texture_png(
    stack: &ResourcePackStack,
    tex: &str,
) -> Result<Option<Vec<u8>>, mtk_resource::ResourceError> {
    let loc = ResourceLocation::parse(tex).unwrap_or_else(|_| ResourceLocation::vanilla(tex));
    let path = loc.to_asset_path("textures", "png");
    if let Some(bytes) = stack.open_asset_raw(&path)? {
        return Ok(Some(bytes));
    }
    if !loc.path.contains('/') {
        let item_path = format!("assets/{}/textures/item/{}.png", loc.namespace, loc.path);
        if let Some(bytes) = stack.open_asset_raw(&item_path)? {
            return Ok(Some(bytes));
        }
        let block_path = format!("assets/{}/textures/block/{}.png", loc.namespace, loc.path);
        if let Some(bytes) = stack.open_asset_raw(&block_path)? {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

/// Why one item model produced no geometry.
///
/// A read failure must abort the bake (its own variant); the rest are per-model diagnostics.
enum ItemBakeError {
    Read(mtk_resource::ResourceError),
    Diagnostic { key: String, reason: &'static str },
}

/// Prebakes all blockstates discovered across all active resource packs in the stack with physical progress reporting.
pub(crate) fn prebake_all_models_with_progress(
    stack: &ResourcePackStack,
    atlas_map: Option<&AtlasAddressMap>,
    progress_callback: Option<ProgressCallback>,
) -> Result<BakedModelDatabase, MtkError> {
    let preloaded = preload_model_and_obj_assets(stack)?;
    prebake_all_models_with_preloaded(stack, preloaded, atlas_map, progress_callback)
}

/// Prebakes all blockstates and item models reusing an already-scanned `PreloadedModelAssets`.
pub(crate) fn prebake_all_models_with_preloaded(
    stack: &ResourcePackStack,
    preloaded: PreloadedModelAssets,
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

    // 1. Reuse preloaded model JSONs and OBJ/MTL asset store
    let model_cache = preloaded.model_cache;
    let mut item_model_locs = preloaded.item_model_locs;
    let mut discovery_failures = preloaded.discovery_failures;
    let obj_assets = preloaded.obj_assets;

    // `get_model` feeds `resolve_hierarchy`, whose callback cannot return an error, so a read
    // failure is parked here and surfaced once the parallel phases finish.
    let read_error: std::sync::Mutex<Option<mtk_resource::ResourceError>> =
        std::sync::Mutex::new(None);

    // Helper closure to look up models with unified candidate paths
    let get_model = |model_id: &str| -> Option<BlockModelJson> {
        if let Some(m) = model_cache.get(model_id) {
            return Some(m.clone());
        }
        for path in ResourceLocation::model_candidate_asset_paths(model_id) {
            if let Some(loc) = ResourceLocation::from_asset_path(&path, "models", "json") {
                if let Some(m) = model_cache.get(&loc.as_string()) {
                    return Some(m.clone());
                }
            }
            match stack.open_asset_raw(&path) {
                Ok(None) => {}
                Ok(Some(bytes)) => {
                    if let Ok(m) = serde_json::from_slice::<BlockModelJson>(&bytes) {
                        return Some(m);
                    }
                }
                Err(err) => {
                    if let Ok(mut slot) = read_error.lock() {
                        slot.get_or_insert(err);
                    }
                    return None;
                }
            }
        }
        None
    };

    // 2. Pre-resolve all discovered model JSON definitions into an immutable shared map
    type ResolveOutcome =
        Result<(String, std::sync::Arc<mtk_model::ResolvedBlockModel>), (String, &'static str)>;

    let resolve_one = |id: &String, model: &BlockModelJson| -> ResolveOutcome {
        model
            .resolve_hierarchy(id, |parent_id| get_model(parent_id))
            .map(|resolved| (id.clone(), std::sync::Arc::new(resolved)))
            .map_err(|error| {
                // Templates reference variables their children define, so resolving them
                // standalone always fails; only a genuinely broken hierarchy is worth reporting.
                let reason = if matches!(error, mtk_model::ModelError::CircularParentHierarchy(_)) {
                    "circular_parent_hierarchy"
                } else {
                    "unresolved_template"
                };
                (id.clone(), reason)
            })
    };

    #[cfg(feature = "parallel")]
    let resolve_results: Vec<ResolveOutcome> = mtk_core::threading::install(|| {
        model_cache
            .par_iter()
            .map(|(id, model)| resolve_one(id, model))
            .collect()
    });

    #[cfg(not(feature = "parallel"))]
    let resolve_results: Vec<ResolveOutcome> = model_cache
        .iter()
        .map(|(id, model)| resolve_one(id, model))
        .collect();

    let mut resolved_map: HashMap<String, std::sync::Arc<mtk_model::ResolvedBlockModel>> =
        HashMap::with_capacity(resolve_results.len());
    for outcome in resolve_results {
        match outcome {
            Ok(pair) => {
                resolved_map.insert(pair.0, pair.1);
            }
            // Only a circular hierarchy is a real defect; template resolution failures are
            // expected (see `resolve_one`). A blockstate that cannot bake is reported below.
            Err((id, "circular_parent_hierarchy")) if id.contains(':') => {
                discovery_failures.push((id, "circular_parent_hierarchy"))
            }
            Err(_) => {}
        }
    }

    if let Some(err) = read_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        return Err(err.into());
    }

    let shared_resolved = std::sync::Arc::new(resolved_map);

    // 3. Pre-load BlockStateDefinitions for all discovered locations
    let mut blockstate_defs: Vec<(ResourceLocation, BlockStateDefinition)> = Vec::new();
    for loc in &blockstate_locs {
        let path = loc.to_asset_path("blockstates", "json");
        let Some(bytes) = stack.open_asset_raw(&path)? else {
            continue;
        };
        match serde_json::from_slice::<BlockStateDefinition>(&bytes) {
            Ok(def) => blockstate_defs.push((loc.clone(), def)),
            Err(_) => discovery_failures.push((loc.as_string(), "invalid_blockstate_json")),
        }
    }

    let total_defs = blockstate_defs.len();
    let throttler = ProgressThrottler::new("prebake_models", total_defs.max(1), progress_callback)
        .with_prefix("Baking block models");

    // 4. Bake all states concurrently
    #[cfg(feature = "parallel")]
    let baked_results: Vec<(
        Vec<(String, mtk_model::BakedVariantGroup)>,
        Vec<(String, &'static str)>,
    )> = mtk_core::threading::install(|| {
        blockstate_defs
            .par_iter()
            .map(|(loc, def)| {
                let states = def.enumerate_all_states(&loc.as_string());
                let mut local_baker =
                    ModelBaker::with_shared_resolved(std::sync::Arc::clone(&shared_resolved))
                        .with_obj_assets(std::sync::Arc::clone(&obj_assets));
                let mut remap_cache: HashMap<String, Option<([f32; 4], u16, u32)>> = HashMap::new();
                let mut pairs = Vec::new();
                let mut failures: Vec<(String, &'static str)> = Vec::new();
                for state_str in states {
                    let baked = local_baker
                        .bake_blockstate_variants(&state_str, Some(def), |id| get_model(id));
                    let Ok(mut group) = baked else {
                        failures.push((state_str, "bake_failed"));
                        continue;
                    };
                    {
                        if let Some(atlas) = atlas_map {
                            group.remap_to_atlas_with(|tex| {
                                if is_dynamic_entity_texture(tex) {
                                    return None;
                                }
                                if let Some(hit) = remap_cache.get(tex) {
                                    *hit
                                } else {
                                    let resolved = MaterialResolver::resolve(tex, None, atlas).map(
                                        |(_, sp)| {
                                            (sp.frame_0_uv_bounds, sp.chunk_id, sp.texture_id)
                                        },
                                    );
                                    remap_cache.insert(tex.to_string(), resolved);
                                    resolved
                                }
                            });
                            group.update_cull_meta_with_atlas(|tex| {
                                if is_dynamic_entity_texture(tex) {
                                    return None;
                                }
                                atlas.resolve_texture_key(tex).map(|sp| sp.is_opaque)
                            });
                        }
                        group.deduplicate_faces();
                        pairs.push((state_str, group));
                    }
                }
                throttler.inc();
                (pairs, failures)
            })
            .collect()
    });

    #[cfg(not(feature = "parallel"))]
    let baked_results: Vec<(
        Vec<(String, mtk_model::BakedVariantGroup)>,
        Vec<(String, &'static str)>,
    )> = blockstate_defs
        .iter()
        .map(|(loc, def)| {
            let states = def.enumerate_all_states(&loc.as_string());
            let mut local_baker =
                ModelBaker::with_shared_resolved(std::sync::Arc::clone(&shared_resolved))
                    .with_obj_assets(std::sync::Arc::clone(&obj_assets));
            let mut remap_cache: HashMap<String, Option<([f32; 4], u16, u32)>> = HashMap::new();
            let mut pairs = Vec::new();
            let mut failures: Vec<(String, &'static str)> = Vec::new();
            for state_str in states {
                let baked =
                    local_baker.bake_blockstate_variants(&state_str, Some(def), |id| get_model(id));
                let Ok(mut group) = baked else {
                    failures.push((state_str, "bake_failed"));
                    continue;
                };
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
                        group.update_cull_meta_with_atlas(|tex| {
                            if is_dynamic_entity_texture(tex) {
                                return None;
                            }
                            atlas.resolve_texture_key(tex).map(|sp| sp.is_opaque)
                        });
                    }
                    group.deduplicate_faces();
                    pairs.push((state_str, group));
                }
            }
            throttler.inc();
            (pairs, failures)
        })
        .collect();

    let mut baked_pairs: Vec<(String, mtk_model::BakedVariantGroup)> = Vec::new();
    for (pairs, failures) in baked_results {
        baked_pairs.extend(pairs);
        discovery_failures.extend(failures);
    }

    // Abstract item models are not renderable items: inheritance-only templates and `air`.
    // Being referenced as a parent is NOT a reliable signal — real items such as
    // `item/bow` are parents of their own pulling variants — so only explicit names are
    // excluded. Everything else that cannot bake is reported through `bake_failures()`.
    const ABSTRACT_ITEM_MODELS: &[&str] = &[
        "air",
        "generated",
        "handheld",
        "handheld_mace",
        "handheld_rod",
        "spear_in_hand",
    ];
    item_model_locs.retain(|loc| {
        let stem = loc.path.strip_prefix("item/").unwrap_or(&loc.path);
        !(loc.path.starts_with("item/template_")
            || (loc.namespace == "minecraft" && ABSTRACT_ITEM_MODELS.contains(&stem)))
    });

    // 5. Bake all item models
    let total_items = item_model_locs.len();
    let item_throttler =
        ProgressThrottler::new("prebake_items", total_items.max(1), progress_callback)
            .with_prefix("Baking item models");

    let item_mesh_opts = ItemMeshOptions::default();

    let bake_single_item =
        |loc: &ResourceLocation| -> Result<(Vec<String>, mtk_model::BakedModel), ItemBakeError> {
            let canon = loc.as_string();
            let resolved =
                shared_resolved
                    .get(&canon)
                    .ok_or_else(|| ItemBakeError::Diagnostic {
                        key: canon.clone(),
                        reason: "unresolved_model",
                    })?;

            let baked: Option<mtk_model::BakedModel> = if resolved.has_geometry() {
                let baker = ModelBaker::new().with_obj_assets(std::sync::Arc::clone(&obj_assets));
                Some(baker.bake_resolved_model(&canon, resolved))
            } else {
                // 2D sprite items: extrude the layer textures into a flat model.
                let mut combined_model: Option<mtk_model::BakedModel> = None;
                for layer_idx in 0..8 {
                    let layer_key = format!("layer{}", layer_idx);
                    if let Some(tex) = resolved.textures.get(&layer_key) {
                        if let Some(png_bytes) =
                            open_item_texture_png(stack, tex).map_err(ItemBakeError::Read)?
                        {
                            if let Ok(layer_model) =
                                bake_item_from_png_bytes(&png_bytes, tex, &item_mesh_opts)
                            {
                                if let Some(ref mut base) = combined_model {
                                    base.elements.extend(layer_model.elements);
                                } else {
                                    combined_model = Some(layer_model);
                                }
                            }
                        }
                    }
                }

                // `builtin/entity` items (chests, shulker boxes, decorated pots, conduits, ...)
                // declare display transforms but no geometry: vanilla renders them through a
                // block-entity renderer. Bake that geometry and keep the item's own display data,
                // otherwise the item renders with guessed scale and orientation.
                if combined_model.is_none() {
                    if let Some(block_name) = loc.path.strip_prefix("item/") {
                        let state = format!("{}:{}", loc.namespace, block_name);
                        let mut baker =
                            ModelBaker::new().with_obj_assets(std::sync::Arc::clone(&obj_assets));
                        let builtin = baker
                            .bake_blockstate_with_entity(&state, None, None, |id| get_model(id))
                            .ok()
                            .filter(|model| {
                                !model.elements.is_empty() || !model.obj_faces.is_empty()
                            });
                        if let Some(mut model) = builtin {
                            model.transforms = resolved.transforms.clone();
                            model.gui_light = resolved.gui_light.or(model.gui_light);
                            model.block_state = canon.clone();
                            combined_model = Some(model);
                        }

                        if combined_model.is_none() {
                            if let Some(block_id) = block_model_id_for_item(loc) {
                                if let Some(block_resolved) = shared_resolved.get(&block_id) {
                                    if block_resolved.has_geometry() {
                                        let mut model = ModelBaker::new()
                                            .with_obj_assets(std::sync::Arc::clone(&obj_assets))
                                            .bake_resolved_model(&canon, block_resolved);
                                        model.transforms = resolved.transforms.clone();
                                        model.gui_light =
                                            resolved.gui_light.or(block_resolved.gui_light);
                                        combined_model = Some(model);
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(ref mut model) = combined_model {
                    if resolved.transforms != mtk_model::ItemTransforms::default() {
                        model.transforms = resolved.transforms.clone();
                    }
                    if resolved.gui_light.is_some() {
                        model.gui_light = resolved.gui_light;
                    }
                }
                combined_model
            };

            let mut baked = baked.ok_or_else(|| ItemBakeError::Diagnostic {
                key: canon.clone(),
                reason: if resolved.textures.is_empty() {
                    "no_geometry"
                } else {
                    "no_texture_layers"
                },
            })?;

            if let Some(atlas) = atlas_map {
                baked.remap_to_atlas_with(|tex| {
                    if is_dynamic_entity_texture(tex) {
                        return None;
                    }
                    MaterialResolver::resolve(tex, None, atlas)
                        .map(|(_, sp)| (sp.frame_0_uv_bounds, sp.chunk_id, sp.texture_id))
                });
            }
            baked.rebuild_face_buckets();
            baked.deduplicate_faces();

            // Only canonical item-model keys. Inserting the bare item name (e.g.
            // `minecraft:chest`) would overwrite the blockstate entry with item geometry
            // and item display data, corrupting block meshing for every block item.
            let keys = vec![loc.as_string(), loc.path.clone()];

            item_throttler.inc();
            Ok((keys, baked))
        };

    #[cfg(feature = "parallel")]
    let item_results: Vec<Result<(Vec<String>, mtk_model::BakedModel), ItemBakeError>> =
        mtk_core::threading::install(|| item_model_locs.par_iter().map(bake_single_item).collect());

    #[cfg(not(feature = "parallel"))]
    let item_results: Vec<Result<(Vec<String>, mtk_model::BakedModel), ItemBakeError>> =
        item_model_locs.iter().map(bake_single_item).collect();

    let mut baked_item_pairs: Vec<(Vec<String>, mtk_model::BakedModel)> = Vec::new();
    let mut item_failures: Vec<(String, &'static str)> = Vec::new();
    for result in item_results {
        match result {
            Ok(pair) => baked_item_pairs.push(pair),
            Err(ItemBakeError::Read(err)) => return Err(err.into()),
            Err(ItemBakeError::Diagnostic { key, reason }) => item_failures.push((key, reason)),
        }
    }

    let mut db = BakedModelDatabase::new();
    for (state_str, group) in baked_pairs {
        db.insert_variant_group(state_str, group);
    }
    for (keys, model) in baked_item_pairs {
        for key in keys {
            db.insert(key, model.clone());
        }
    }
    for (key, reason) in discovery_failures.iter().chain(item_failures.iter()) {
        db.push_bake_failure(key.clone(), *reason);
    }
    db.deduplicate_all();

    if let Some(cb) = progress_callback {
        cb(ProgressReport::new(
            "prebake_models",
            total_defs + total_items,
            (total_defs + total_items).max(1),
            format!("Completed baking {} models and items", db.len()),
        ));
        let failures = db.bake_failures();
        if !failures.is_empty() {
            let sample: Vec<&str> = failures
                .iter()
                .take(5)
                .map(|failure| failure.key.as_str())
                .collect();
            cb(ProgressReport::new(
                "prebake_models",
                total_defs + total_items,
                (total_defs + total_items).max(1),
                format!(
                    "{} models could not be baked ({}): {}",
                    failures.len(),
                    if failures.len() > sample.len() {
                        "showing first 5"
                    } else {
                        "complete list"
                    },
                    sample.join(", ")
                ),
            ));
        }
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
