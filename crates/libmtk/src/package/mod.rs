//! Typed access to the payloads a package can carry.
//!
//! `.mtkcache` (asset cache) and `.mtkscene` (scene interchange) are the same container on disk:
//! `mtk-package` header + table of contents + zstd chunks, distinguished only by the header
//! profile. They also share chunk identifiers for models, atlas data, colormaps, biome mapping, CTM
//! rules and entity slots.
//!
//! This module is the single source of truth for those identifiers and their decoding. A new
//! payload is therefore added once — not in each reader — which keeps the cache, the scene package
//! and a future asset library from drifting apart.

use std::collections::HashMap;
use std::sync::Arc;

use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_model::BakedModelDatabase;
use mtk_package::MtkPackageReader;
use mtk_texture::RgbaBuffer;
use mtk_voxel::SparseVoxelPayload;

pub mod manifest;
pub mod source;

pub use manifest::{
    PackageManifest, SceneCapabilities, ASSET_CACHE_FORMAT_VERSION, SCENE_PACKAGE_FORMAT_VERSION,
};
pub use source::AssetBundle;

use crate::MtkError;
use mtk_texture::AtlasAddressMap;

/// Chunk identifiers shared by every package profile.
pub mod chunks {
    /// Baked model database (postcard + zstd payload).
    pub const MODELS_DATABASE: &str = "models/database";
    /// Atlas address map, as JSON.
    pub const ATLAS_MAPPING: &str = "atlas/mapping";
    /// Prefix for the PNG chunks holding one atlas texture or PBR companion.
    pub const ATLAS_TEXTURE_PREFIX: &str = "atlas/textures/";
    /// Biome name mapping, as JSON.
    pub const BIOME_MAPPING: &str = "biome/mapping";
    /// Prefix for a colormap PNG chunk (`grass`, `foliage`, `dry_foliage`).
    pub const BIOME_COLMAP_PREFIX: &str = "biome/colormap/";
    /// CTM rule set, as JSON.
    pub const CTM_RULES: &str = "ctm/rules";
    /// Dynamic entity slot addressing (`key -> (chunk id, slot)`), as JSON.
    pub const ENTITY_SLOTS: &str = "entity/slots";
    /// Sparse voxel payload of a scene package.
    pub const VOXELS_SPARSE: &str = "voxels/sparse";
    /// Banner base cloth PNG (asset cache only).
    pub const BANNER_BASE: &str = "banner/base";
    /// Banner pattern PNGs, as JSON metadata (asset cache only).
    pub const BANNER_PATTERNS: &str = "banner/patterns";
    /// Resolver overrides baked into the cache.
    pub const RESOLVER_OVERRIDES: &str = "resolver/overrides";

    /// Full chunk id of an atlas texture.
    ///
    /// Accepts either a file stem (`blocks_0`) or an already-prefixed id, so callers that receive a
    /// chunk name from a package table do not have to know which form they hold.
    pub fn atlas_texture(stem_or_id: &str) -> String {
        if stem_or_id.starts_with(ATLAS_TEXTURE_PREFIX) {
            stem_or_id.to_string()
        } else {
            format!("{ATLAS_TEXTURE_PREFIX}{stem_or_id}")
        }
    }

    /// Chunk id of a colormap channel.
    pub fn colormap(name: &str) -> String {
        format!("{BIOME_COLMAP_PREFIX}{name}")
    }
}

/// Reader for the payloads shared by all package profiles.
pub struct PackageContents<'a> {
    reader: &'a MtkPackageReader,
}

impl<'a> PackageContents<'a> {
    /// Wraps a container reader.
    pub fn new(reader: &'a MtkPackageReader) -> Self {
        Self { reader }
    }

    /// Whether the package carries a chunk with this id.
    pub fn has(&self, chunk_id: &str) -> bool {
        self.reader.has_chunk(chunk_id)
    }

    /// Baked model database, when present.
    pub fn models(&self) -> Result<Option<BakedModelDatabase>, MtkError> {
        if !self.has(chunks::MODELS_DATABASE) {
            return Ok(None);
        }
        let bytes = self
            .reader
            .read_chunk_decompressed(chunks::MODELS_DATABASE)?;
        Ok(Some(BakedModelDatabase::from_binary(&bytes)?))
    }

    /// Atlas address map, when present.
    pub fn atlas_mapping(&self) -> Result<Option<AtlasAddressMap>, MtkError> {
        let Some(json) = self.atlas_mapping_json()? else {
            return Ok(None);
        };
        Ok(Some(AtlasAddressMap::from_json(&json)?))
    }

    /// Raw atlas address map JSON, when present.
    pub fn atlas_mapping_json(&self) -> Result<Option<String>, MtkError> {
        if !self.has(chunks::ATLAS_MAPPING) {
            return Ok(None);
        }
        Ok(Some(self.reader.read_chunk_str(chunks::ATLAS_MAPPING)?))
    }

    /// Decodes an atlas texture chunk to RGBA, optionally flipping v.
    ///
    /// Accepts a file stem or a full `atlas/textures/...` id.
    pub fn texture_rgba(&self, chunk_id: &str, flip_v: bool) -> Result<RgbaBuffer, MtkError> {
        let png = self
            .reader
            .read_chunk_decompressed(&chunks::atlas_texture(chunk_id))?;
        let mut buffer = RgbaBuffer::from_png_bytes(&png)?;
        if flip_v {
            buffer.flip_v();
        }
        Ok(buffer)
    }

    /// Decodes an atlas texture chunk to normalized float pixels, optionally flipping v.
    pub fn texture_f32(
        &self,
        chunk_id: &str,
        flip_v: bool,
    ) -> Result<(u32, u32, Vec<f32>), MtkError> {
        let png = self
            .reader
            .read_chunk_decompressed(&chunks::atlas_texture(chunk_id))?;
        let buffer = RgbaBuffer::from_png_bytes(&png)?;
        let (width, height) = (buffer.width, buffer.height);
        Ok((width, height, buffer.to_f32_buffer(flip_v)))
    }

    /// Raw bytes of a colormap channel, when present.
    pub fn colormap(&self, name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        let chunk_id = chunks::colormap(name);
        if !self.has(&chunk_id) {
            return Ok(None);
        }
        Ok(Some(self.reader.read_chunk_decompressed(&chunk_id)?))
    }

    /// Colormaps carried by this package, when any channel is present.
    pub fn colormaps(&self) -> Result<Option<Arc<VanillaColormaps>>, MtkError> {
        let grass = self.colormap("grass")?;
        let foliage = self.colormap("foliage")?;
        let dry_foliage = self.colormap("dry_foliage")?;
        if grass.is_none() && foliage.is_none() && dry_foliage.is_none() {
            return Ok(None);
        }
        let colormaps = VanillaColormaps::with_overrides(
            grass.as_deref(),
            foliage.as_deref(),
            dry_foliage.as_deref(),
        )
        .map_err(|error| MtkError::Texture(mtk_texture::TextureError::Baking(error)))?;
        Ok(Some(Arc::new(colormaps)))
    }

    /// Whether any colormap channel is present.
    pub fn has_colormaps(&self) -> bool {
        ["grass", "foliage", "dry_foliage"]
            .into_iter()
            .any(|name| self.has(&chunks::colormap(name)))
    }

    /// Biome name mapping, when present.
    pub fn biome_resolver(&self) -> Result<Option<BiomeResolver>, MtkError> {
        if !self.has(chunks::BIOME_MAPPING) {
            return Ok(None);
        }
        let json = self.reader.read_chunk_str(chunks::BIOME_MAPPING)?;
        Ok(Some(BiomeResolver::from_json(&json)?))
    }

    /// CTM rule set, when present.
    pub fn ctm_rules_json(&self) -> Result<Option<String>, MtkError> {
        if !self.has(chunks::CTM_RULES) {
            return Ok(None);
        }
        Ok(Some(self.reader.read_chunk_str(chunks::CTM_RULES)?))
    }

    /// Parsed CTM rule set, when present.
    pub fn ctm_rules(&self) -> Result<Option<Vec<mtk_resource::CtmRule>>, MtkError> {
        let Some(json) = self.ctm_rules_json()? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&json)?))
    }

    /// Indexed CTM solver built from the rule set, when present.
    pub fn ctm_solver(&self) -> Result<Option<mtk_resource::CtmSolver>, MtkError> {
        let Some(json) = self.ctm_rules_json()? else {
            return Ok(None);
        };
        Ok(Some(
            mtk_resource::CtmSolver::from_json(&json).map_err(MtkError::Json)?,
        ))
    }

    /// Dynamic entity slot addressing (`key -> (chunk id, slot)`), when present.
    pub fn entity_slots(&self) -> Result<Option<HashMap<String, (u16, u16)>>, MtkError> {
        if !self.has(chunks::ENTITY_SLOTS) {
            return Ok(None);
        }
        Ok(Some(self.reader.read_chunk_json(chunks::ENTITY_SLOTS)?))
    }

    /// Sparse voxel payload (scene packages).
    pub fn sparse_voxels(&self) -> Result<SparseVoxelPayload, MtkError> {
        let bytes = self.reader.read_chunk_decompressed(chunks::VOXELS_SPARSE)?;
        Ok(postcard::from_bytes(&bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::chunks;

    #[test]
    fn chunk_ids_are_stable() {
        // These strings are on-disk identifiers: changing one silently invalidates every package.
        assert_eq!(chunks::MODELS_DATABASE, "models/database");
        assert_eq!(chunks::ATLAS_MAPPING, "atlas/mapping");
        assert_eq!(chunks::BIOME_MAPPING, "biome/mapping");
        assert_eq!(chunks::CTM_RULES, "ctm/rules");
        assert_eq!(chunks::VOXELS_SPARSE, "voxels/sparse");
        assert_eq!(chunks::atlas_texture("blocks_0"), "atlas/textures/blocks_0");
        assert_eq!(
            chunks::atlas_texture("atlas/textures/blocks_0"),
            "atlas/textures/blocks_0"
        );
        assert_eq!(
            chunks::colormap("dry_foliage"),
            "biome/colormap/dry_foliage"
        );
    }
}
