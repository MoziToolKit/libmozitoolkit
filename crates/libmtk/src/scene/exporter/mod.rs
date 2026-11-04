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
pub use cropper::SceneAtlasSource;
use mtk_model::BakedModelDatabase;
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use super::package::{write_scene_package, ScenePackageWriteOptions};
use crate::MtkError;

/// Builds all components of a Scene Interchange bundle in memory.
pub fn build_scene_interchange(
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    config: &SceneExporterConfig,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
) -> Result<SceneExportResult, MtkError> {
    build_scene_interchange_with_ctm(
        voxels,
        source_models,
        source_atlas_map,
        atlas_source,
        config,
        dynamic_entity_atlases,
        None,
    )
}

/// Builds all components of a Scene Interchange bundle in memory with explicit or auto-loaded CTM rules.
pub fn build_scene_interchange_with_ctm(
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    config: &SceneExporterConfig,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
    source_ctm_rules: Option<&[mtk_resource::CtmRule]>,
) -> Result<SceneExportResult, MtkError> {
    // 1. Resolve source CTM rules: either explicit rules or loaded from atlas_source
    let loaded_ctm_rules: Option<Vec<mtk_resource::CtmRule>> = if source_ctm_rules.is_some() {
        None
    } else {
        atlas_source
            .get_ctm_rules_json()
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
    };
    let effective_ctm_rules = source_ctm_rules.or(loaded_ctm_rules.as_deref());

    // 2. Collect all unique texture and blockstate identifiers (plus tree-shaken CTM sub-tiles)
    let assets = collector::collect_needed_assets(voxels, source_models, effective_ctm_rules);

    // 3. Crop referenced sprites into DecodedSprite instances
    let crops = cropper::crop_needed_sprites(
        &assets.needed_textures,
        source_atlas_map,
        atlas_source,
        dynamic_entity_atlases,
    )?;

    // 4. Extract resident colormaps and biome_resolver from atlas_source
    let mut colormaps = Vec::new();
    for cm_name in &["grass", "foliage", "dry_foliage"] {
        if let Ok(Some(bytes)) = atlas_source.get_colormap(cm_name) {
            colormaps.push((cm_name.to_string(), bytes));
        }
    }
    let biome_resolver_json = atlas_source.get_biome_resolver_json().ok().flatten();

    // 5. Delegate stitching, PBR decoding, and model tree-shaking to AtlasBuilder
    builder::build_scene_atlas_and_models(
        crops,
        source_models,
        &assets.needed_blockstates,
        voxels,
        config,
        colormaps,
        biome_resolver_json,
        assets.shaken_ctm_rules,
    )
}

/// End-to-end export function: builds the scene interchange and writes `.mtkscene` to disk.
pub fn export_scene_package(
    path: impl AsRef<Path>,
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    config: &SceneExporterConfig,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
    write_options: Option<&ScenePackageWriteOptions>,
) -> Result<SceneExportResult, MtkError> {
    export_scene_package_with_ctm(
        path,
        voxels,
        source_models,
        source_atlas_map,
        atlas_source,
        config,
        dynamic_entity_atlases,
        None,
        write_options,
    )
}

/// End-to-end export function with optional explicit CTM rules.
pub fn export_scene_package_with_ctm(
    path: impl AsRef<Path>,
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    config: &SceneExporterConfig,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
    source_ctm_rules: Option<&[mtk_resource::CtmRule]>,
    write_options: Option<&ScenePackageWriteOptions>,
) -> Result<SceneExportResult, MtkError> {
    let result = build_scene_interchange_with_ctm(
        voxels,
        source_models,
        source_atlas_map,
        atlas_source,
        config,
        dynamic_entity_atlases,
        source_ctm_rules,
    )?;

    let colormaps = if !result.colormaps.is_empty() {
        Some(result.colormaps.as_slice())
    } else {
        None
    };

    write_scene_package(
        path,
        &result.metadata,
        voxels,
        Some(&result.tree_shaken_models),
        Some(&result.mini_atlas_map),
        &result.mini_atlas_textures,
        colormaps,
        result.biome_resolver_json.as_deref(),
        result.ctm_rules_json.as_deref(),
        write_options,
    )?;

    Ok(result)
}
