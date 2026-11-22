//! Pre-scan loader for model JSONs, Wavefront `.obj`/`.mtl` assets, and non-`block/` textures.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use mtk_model::{BlockModelJson, ObjAssetStore, ParsedMtlLibrary, ParsedObjModel};
use mtk_resource::{ResourceLocation, ResourcePackStack};

use crate::MtkError;

pub(crate) struct PreloadedModelAssets {
    pub model_cache: HashMap<String, BlockModelJson>,
    pub item_model_locs: Vec<ResourceLocation>,
    pub discovery_failures: Vec<(String, &'static str)>,
    pub obj_assets: Arc<ObjAssetStore>,
    pub extra_atlas_textures: Vec<ResourceLocation>,
}

fn try_collect_extra_texture(
    raw_ref: &str,
    png_files: &HashSet<String>,
    out: &mut BTreeSet<ResourceLocation>,
) {
    let norm = BlockModelJson::normalize_texture(raw_ref);
    if norm.is_empty() || norm.starts_with('#') {
        return;
    }
    let Ok(loc) = ResourceLocation::parse(&norm) else {
        return;
    };
    if loc.path.starts_with("block/") {
        return;
    }
    let asset_path = loc.to_asset_path("textures", "png");
    if png_files.contains(&asset_path) {
        out.insert(loc);
    }
}

/// Scans `stack` once to preload model JSONs, referenced `.obj`/`.mtl` assets, and extra textures
/// needed by OBJ and composite models outside `textures/block/`.
pub(crate) fn preload_model_and_obj_assets(
    stack: &ResourcePackStack,
) -> Result<PreloadedModelAssets, MtkError> {
    let mut model_cache: HashMap<String, BlockModelJson> = HashMap::new();
    let mut item_model_locs: Vec<ResourceLocation> = Vec::new();
    let mut seen_items = HashSet::new();
    let mut discovery_failures: Vec<(String, &'static str)> = Vec::new();
    let mut png_files = HashSet::new();

    let all_files = stack.list_asset_files("assets/");
    for file in &all_files {
        if file.ends_with(".png") && file.contains("/textures/") {
            png_files.insert(file.clone());
            continue;
        }
        if file.ends_with(".json") && file.contains("/models/") {
            let Some(bytes) = stack.open_asset_raw(file)? else {
                continue;
            };
            let Some(loc) = ResourceLocation::from_asset_path(file, "models", "json") else {
                continue;
            };
            let canon = loc.as_string();
            // Zero-length model files exist as intentional placeholders in some packs
            // (the model is provided by a resource pack lower in the stack).
            if bytes.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            match serde_json::from_slice::<BlockModelJson>(&bytes) {
                Ok(model) => {
                    model_cache.insert(canon.clone(), model);
                    if loc.path.starts_with("item/") && seen_items.insert(canon) {
                        item_model_locs.push(loc);
                    }
                }
                Err(_) => discovery_failures.push((canon, "invalid_model_json")),
            }
        }
    }

    let mut obj_assets = ObjAssetStore::new();
    let mut loaded_objs: HashSet<String> = HashSet::new();
    let mut loaded_mtls: HashSet<String> = HashSet::new();
    let mut extra_textures = BTreeSet::new();

    for (model_id, model) in &model_cache {
        let fallback_ns = model_id
            .split_once(':')
            .map(|(ns, _)| ns)
            .unwrap_or("minecraft");
        let obj_refs = model.obj_model_references();
        let has_loader_ext = !obj_refs.is_empty()
            || model
                .loader
                .as_deref()
                .is_some_and(|l| l.ends_with(":composite"))
            || model
                .porting_lib_loader
                .as_deref()
                .is_some_and(|l| l.ends_with(":composite"));

        if has_loader_ext || !model_id.contains(":item/") {
            for tex_ref in model.texture_references() {
                try_collect_extra_texture(tex_ref, &png_files, &mut extra_textures);
            }
        }

        for (obj_ref, mtl_override) in obj_refs {
            let mut found_obj_path: Option<String> = None;
            let mut mtllib_list: Vec<String> = Vec::new();

            for cand in ObjAssetStore::model_ref_to_asset_candidates(obj_ref, fallback_ns) {
                if loaded_objs.contains(&cand) {
                    found_obj_path = Some(cand);
                    break;
                }
                if let Some(bytes) = stack.open_asset_raw(&cand)? {
                    let text = String::from_utf8_lossy(&bytes);
                    let parsed = ParsedObjModel::parse_str(&text);
                    mtllib_list = parsed.mtllibs.clone();
                    obj_assets.insert_obj(cand.clone(), parsed);
                    loaded_objs.insert(cand.clone());
                    found_obj_path = Some(cand);
                    break;
                }
            }

            let Some(obj_path) = found_obj_path else {
                continue;
            };

            let mut mtl_refs: Vec<&str> = mtllib_list.iter().map(String::as_str).collect();
            if let Some(ovr) = mtl_override {
                mtl_refs.push(ovr);
            }
            for mtl_ref in mtl_refs {
                for cand in ObjAssetStore::resolve_mtl_candidates(&obj_path, mtl_ref) {
                    if !loaded_mtls.insert(cand.clone()) {
                        continue;
                    }
                    if let Some(bytes) = stack.open_asset_raw(&cand)? {
                        let text = String::from_utf8_lossy(&bytes);
                        let lib = ParsedMtlLibrary::parse_str(&text);
                        for tex_ref in lib.iter_direct_texture_refs() {
                            try_collect_extra_texture(tex_ref, &png_files, &mut extra_textures);
                        }
                        obj_assets.insert_mtl(cand, lib);
                    }
                }
            }
        }
    }

    Ok(PreloadedModelAssets {
        model_cache,
        item_model_locs,
        discovery_failures,
        obj_assets: Arc::new(obj_assets),
        extra_atlas_textures: extra_textures.into_iter().collect(),
    })
}
