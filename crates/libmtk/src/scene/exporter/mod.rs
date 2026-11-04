//! # Scene Interchange Exporter
//!
//! Provides the end-to-end export pipeline that tree-shakes block models, crops only
//! the referenced sprite textures from the active global atlas (including static, overlay,
//! animated strips, and dynamic entity skins/banners), stitches them into a compact,
//! self-contained Scene Mini-Atlas, remaps model UV coordinates, and bundles everything
//! into a self-contained `.mtkscene` package.

pub mod builder;
pub mod collector;
pub mod cropper;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;

pub use builder::{SceneExportResult, SceneExporterConfig};
pub use cropper::{
    AtlasLayerSource, BiomeAssetSource, CtmAssetSource, EntityAssetSource, SceneAtlasSource,
    SceneSourceCapabilities,
};
use mtk_model::BakedModelDatabase;
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use super::package::{write_scene_package, ScenePackageWriteOptions, SceneWriteRequest};
use crate::MtkError;

/// Explicit CTM rule resolution mode during scene interchange export.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CtmMode<'a> {
    /// Strictly no CTM rules exported or sub-tiles tree-shaken (default).
    #[default]
    Disabled,
    /// Automatically load CTM rules from the asset cache/source if present.
    Auto,
    /// Use an explicit slice of CTM rules (overriding the cache).
    Explicit(&'a [mtk_resource::CtmRule]),
}

/// Request structure encapsulating all parameters for building or exporting a scene interchange package.
pub struct SceneExportRequest<'a, S: SceneAtlasSource> {
    pub voxels: &'a SparseVoxelPayload,
    pub source_models: &'a BakedModelDatabase,
    pub source_atlas_map: &'a AtlasAddressMap,
    pub atlas_source: &'a mut S,
    pub config: &'a SceneExporterConfig,
    pub dynamic_entity_atlases: Option<&'a HashMap<u16, Vec<u8>>>,
    pub dynamic_entity_slots: Option<&'a HashMap<String, (u16, u16)>>,
    pub ctm_mode: CtmMode<'a>,
}

/// Builds all components of a Scene Interchange bundle in memory.
pub fn build_scene_interchange<S: SceneAtlasSource>(
    req: SceneExportRequest<'_, S>,
) -> Result<SceneExportResult, MtkError> {
    // 1. Resolve source CTM rules based on the explicit CtmMode
    let effective_ctm_rules: Option<Vec<mtk_resource::CtmRule>> = match req.ctm_mode {
        CtmMode::Disabled => None,
        CtmMode::Auto => match req.atlas_source.get_ctm_rules_json() {
            Ok(Some(json)) => {
                let rules: Vec<mtk_resource::CtmRule> =
                    serde_json::from_str(&json).map_err(|e| {
                        MtkError::Resource(mtk_resource::error::ResourceError::NotFound(format!(
                            "Corrupted CTM rules JSON in asset source: {e}"
                        )))
                    })?;
                Some(rules)
            }
            Ok(None) => None,
            Err(e) => {
                return Err(MtkError::Resource(
                    mtk_resource::error::ResourceError::NotFound(format!(
                        "Failed reading CTM rules from asset source: {e}"
                    )),
                ))
            }
        },
        CtmMode::Explicit(rules) => Some(rules.to_vec()),
    };

    // 2. Collect all unique texture and blockstate identifiers (plus tree-shaken CTM sub-tiles)
    let assets = collector::collect_needed_assets(
        req.voxels,
        req.source_models,
        effective_ctm_rules.as_deref(),
    );

    // 3. Crop referenced sprites into DecodedSprite instances
    let crops = cropper::crop_needed_sprites(
        &assets.needed_textures,
        req.source_atlas_map,
        req.atlas_source,
        req.dynamic_entity_atlases,
        req.dynamic_entity_slots,
    )?;

    // 4. Extract resident colormaps and biome_resolver from atlas_source
    let mut colormaps = Vec::new();
    for cm_name in &["grass", "foliage", "dry_foliage"] {
        match req.atlas_source.get_colormap(cm_name) {
            Ok(Some(bytes)) => {
                if bytes.len() >= 8
                    && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
                {
                    colormaps.push((cm_name.to_string(), bytes));
                } else {
                    return Err(MtkError::Resource(
                        mtk_resource::error::ResourceError::NotFound(format!(
                            "Corrupted colormap '{cm_name}': invalid PNG signature"
                        )),
                    ));
                }
            }
            Ok(None) => {}
            Err(e) => {
                return Err(MtkError::Resource(
                    mtk_resource::error::ResourceError::NotFound(format!(
                        "Failed reading colormap '{cm_name}' from asset source: {e}"
                    )),
                ));
            }
        }
    }
    let biome_resolver_json = match req.atlas_source.get_biome_resolver_json() {
        Ok(Some(json)) => {
            mtk_material::BiomeResolver::from_json(&json)?;
            Some(json)
        }
        Ok(None) => None,
        Err(e) => {
            return Err(MtkError::Resource(
                mtk_resource::error::ResourceError::NotFound(format!(
                    "Failed reading biome resolver from asset source: {e}"
                )),
            ));
        }
    };

    // 5. Delegate stitching, PBR decoding, and model tree-shaking to AtlasBuilder
    builder::build_scene_atlas_and_models(
        crops,
        req.source_models,
        &assets.needed_blockstates,
        req.voxels,
        req.config,
        colormaps,
        biome_resolver_json,
        assets.shaken_ctm_rules,
    )
}

/// End-to-end export function: builds the scene interchange and writes `.mtkscene` to disk.
pub fn export_scene_package<S: SceneAtlasSource>(
    path: impl AsRef<Path>,
    req: SceneExportRequest<'_, S>,
    write_options: Option<&ScenePackageWriteOptions>,
) -> Result<SceneExportResult, MtkError> {
    let voxels = req.voxels;
    let dynamic_entity_slots = req.dynamic_entity_slots.cloned();
    let result = build_scene_interchange(req)?;

    let mut write_req =
        SceneWriteRequest::new(&result.metadata, voxels, &result.mini_atlas_textures);
    write_req.models = Some(&result.tree_shaken_models);
    write_req.atlas_mapping = Some(&result.mini_atlas_map);
    write_req.colormaps = if !result.colormaps.is_empty() {
        Some(result.colormaps.as_slice())
    } else {
        None
    };
    write_req.biome_resolver_json = result.biome_resolver_json.as_deref();
    write_req.ctm_rules = if !result.ctm_rules.is_empty() {
        Some(result.ctm_rules.as_slice())
    } else {
        None
    };
    write_req.dynamic_entity_slots = dynamic_entity_slots.as_ref();
    write_req.options = write_options;

    write_scene_package(path, write_req)?;

    Ok(result)
}
