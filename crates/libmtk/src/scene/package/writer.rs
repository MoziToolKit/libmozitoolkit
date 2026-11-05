//! Writer for `.mtkscene` scene interchange packages.

use std::collections::HashMap;
use std::path::Path;

use mtk_model::BakedModelDatabase;
use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, PackageProfile};
use mtk_texture::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use crate::scene::metadata::SceneMetadata;
use crate::MtkError;

/// Request parameters for writing an `.mtkscene` scene interchange package.
#[derive(Debug, Clone)]
pub(crate) struct SceneWriteRequest<'a> {
    pub(crate) metadata: &'a SceneMetadata,
    pub(crate) voxels: &'a SparseVoxelPayload,
    pub(crate) models: Option<&'a BakedModelDatabase>,
    pub(crate) atlas_mapping: Option<&'a AtlasAddressMap>,
    pub(crate) atlas_textures: &'a [(String, Vec<u8>)],
    pub(crate) colormaps: Option<&'a [(String, Vec<u8>)]>,
    pub(crate) biome_resolver_json: Option<&'a str>,
    pub(crate) ctm_rules: Option<&'a [mtk_resource::CtmRule]>,
    pub(crate) dynamic_entity_slots: Option<&'a HashMap<String, (u16, u16)>>,
}

impl<'a> SceneWriteRequest<'a> {
    /// Creates a minimal `SceneWriteRequest` with required fields.
    pub(crate) fn new(
        metadata: &'a SceneMetadata,
        voxels: &'a SparseVoxelPayload,
        atlas_textures: &'a [(String, Vec<u8>)],
    ) -> Self {
        Self {
            metadata,
            voxels,
            models: None,
            atlas_mapping: None,
            atlas_textures,
            colormaps: None,
            biome_resolver_json: None,
            ctm_rules: None,
            dynamic_entity_slots: None,
        }
    }
}

/// High-level function to write an `.mtkscene` package file to disk.
pub(crate) fn write_scene_package(
    path: impl AsRef<Path>,
    req: SceneWriteRequest<'_>,
) -> Result<(), MtkError> {
    let fp = [0u8; 16];

    let mut writer = MtkPackageFileWriter::create(path, PackageProfile::SceneInterchange, fp)?;

    // 1. Scene Metadata
    writer.add_json_chunk(
        *b"META",
        "manifest",
        req.metadata,
        &ChunkWriteOptions::zstd_fast(),
    )?;

    // 2. Sparse Voxels
    let voxl_bytes = postcard::to_allocvec(req.voxels)?;
    writer.add_chunk(
        *b"VOXL",
        "voxels/sparse",
        &voxl_bytes,
        &ChunkWriteOptions::zstd_fast(),
    )?;

    // 3. Models (optional)
    if let Some(db) = req.models {
        let db_bytes = db.to_binary()?;
        writer.add_chunk(
            *b"MODL",
            "models/database",
            &db_bytes,
            &ChunkWriteOptions::zstd_fast(),
        )?;
    }

    // 4. Atlas mapping (optional)
    if let Some(map) = req.atlas_mapping {
        let map_json = map.to_json()?;
        writer.add_str_chunk(
            *b"ATLS",
            "atlas/mapping",
            &map_json,
            &ChunkWriteOptions::zstd_fast(),
        )?;
    }

    // 5. Atlas textures (optional)
    for (name, bytes) in req.atlas_textures {
        let chunk_id = if name.starts_with("atlas/textures/") {
            name.clone()
        } else {
            format!("atlas/textures/{}", name)
        };
        writer.add_chunk(*b"ATLS", chunk_id, bytes, &ChunkWriteOptions::raw())?;
    }

    // 6. Colormaps (optional)
    if let Some(cms) = req.colormaps {
        for (name, bytes) in cms {
            let chunk_id = format!("biome/colormap/{name}");
            writer.add_chunk(*b"BIOM", chunk_id, bytes, &ChunkWriteOptions::zstd_fast())?;
        }
    }

    // 7. Biome Resolver (optional)
    if let Some(json) = req.biome_resolver_json {
        writer.add_str_chunk(
            *b"BIOM",
            "biome/mapping",
            json,
            &ChunkWriteOptions::zstd_fast(),
        )?;
    }

    // 8. CTM Rules (optional)
    if let Some(rules) = req.ctm_rules {
        if !rules.is_empty() {
            let json = serde_json::to_string(rules).map_err(MtkError::Json)?;
            writer.add_str_chunk(
                *b"CTMR",
                "ctm/rules",
                &json,
                &ChunkWriteOptions::zstd_fast(),
            )?;
        }
    }

    // 9. Dynamic Entity Slots (optional)
    if let Some(slots) = req.dynamic_entity_slots {
        let active_slots: HashMap<String, (u16, u16)> = if let Some(map) = req.atlas_mapping {
            slots
                .iter()
                .filter(|(k, _)| map.resolve_texture_key(k).is_some())
                .map(|(k, v)| (k.clone(), *v))
                .collect()
        } else {
            slots.clone()
        };
        if !active_slots.is_empty() {
            writer.add_json_chunk(
                *b"ENTT",
                "entity/slots",
                &active_slots,
                &ChunkWriteOptions::zstd_fast(),
            )?;
        }
    }

    writer.finish()?;
    Ok(())
}
