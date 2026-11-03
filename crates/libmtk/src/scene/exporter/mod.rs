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
    // 1. Collect all unique texture and blockstate identifiers needed by the scene
    let assets = collector::collect_needed_assets(voxels, source_models);

    // 2. Crop referenced sprites into DecodedSprite instances
    let crops = cropper::crop_needed_sprites(
        &assets.needed_textures,
        source_atlas_map,
        atlas_source,
        dynamic_entity_atlases,
    )?;

    // 3. Delegate stitching, PBR decoding, and model tree-shaking to AtlasBuilder
    builder::build_scene_atlas_and_models(
        crops,
        source_models,
        &assets.needed_blockstates,
        voxels,
        config,
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
    let result = build_scene_interchange(
        voxels,
        source_models,
        source_atlas_map,
        atlas_source,
        config,
        dynamic_entity_atlases,
    )?;

    write_scene_package(
        path,
        &result.metadata,
        voxels,
        Some(&result.tree_shaken_models),
        Some(&result.mini_atlas_map),
        &result.mini_atlas_textures,
        write_options,
    )?;

    Ok(result)
}
