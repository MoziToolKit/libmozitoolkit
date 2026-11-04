//! Writer for `.mtkscene` scene interchange packages.

use std::collections::HashMap;
use std::path::Path;

use mtk_model::BakedModelDatabase;
use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, PackageProfile};
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use crate::scene::metadata::SceneMetadata;
use crate::MtkError;

/// Helper options for authoring `.mtkscene` packages.
#[derive(Debug, Clone, Default)]
pub struct ScenePackageWriteOptions {
    /// Optional content fingerprint (defaults to zeroed 16 bytes).
    pub fingerprint: Option<[u8; 16]>,
}

/// Request parameters for writing an `.mtkscene` scene interchange package.
#[derive(Debug, Clone)]
pub struct SceneWriteRequest<'a> {
    pub metadata: &'a SceneMetadata,
    pub voxels: &'a SparseVoxelPayload,
    pub models: Option<&'a BakedModelDatabase>,
    pub atlas_mapping: Option<&'a AtlasAddressMap>,
    pub atlas_textures: &'a [(String, Vec<u8>)],
    pub colormaps: Option<&'a [(String, Vec<u8>)]>,
    pub biome_resolver_json: Option<&'a str>,
    pub ctm_rules: Option<&'a [mtk_resource::CtmRule]>,
    pub dynamic_entity_slots: Option<&'a HashMap<String, (u16, u16)>>,
    pub options: Option<&'a ScenePackageWriteOptions>,
}

impl<'a> SceneWriteRequest<'a> {
    /// Creates a minimal `SceneWriteRequest` with required fields.
    pub fn new(
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
            options: None,
        }
    }
}

/// High-level function to write an `.mtkscene` package file to disk.
pub fn write_scene_package(
    path: impl AsRef<Path>,
    req: SceneWriteRequest<'_>,
) -> Result<(), MtkError> {
    let fp = req.options.and_then(|o| o.fingerprint).unwrap_or([0u8; 16]);

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
