//! Asset cache reader and manifest verification for `.mtkcache` packages.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use mtk_material::BiomeResolver;
use mtk_model::BakedModelDatabase;
use mtk_package::{MtkPackageReader, CURRENT_VERSION_MAJOR};
use mtk_texture::atlas::AtlasAddressMap;

use crate::MtkError;

/// Format version for compiled cache compatibility checking.
///
/// v5: the baked model database uses Postcard + Zstd instead of Bincode,
///     producing smaller `models.bin` payloads; older caches are invalidated.
/// v4: LabPBR is decoded at precompile time; atlas chunks carry `_n` (GL normal + height),
///     `_orm` (AO/roughness/metallic/emission) and `_aux` (IOR/porosity/SSS/transmission).
pub const ASSET_CACHE_FORMAT_VERSION: u32 = 6;

/// Converts a string fingerprint (e.g. `"mtk_fp_1234abcd5678ef00"`) to a 16-byte array.
pub fn fingerprint_str_to_bytes16(fp: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let bytes = fp.as_bytes();
    if let Some(hex_part) = fp.strip_prefix("mtk_fp_") {
        if let Ok(val) = u64::from_str_radix(hex_part, 16) {
            out[..8].copy_from_slice(&val.to_le_bytes());
            return out;
        }
    }
    let copy_len = bytes.len().min(16);
    out[..copy_len].copy_from_slice(&bytes[..copy_len]);
    out
}

/// Metadata manifest stored in `.mtkcache` container for integrity and invalidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifest {
    pub format_version: u32,
    pub fingerprint: String,
    pub pack_count: usize,
    pub atlas_chunks: usize,
    pub baked_models: usize,
    pub created_at_epoch_secs: u64,
}

impl CacheManifest {
    /// Read manifest directly from a `.mtkcache` package file.
    pub fn read_from_package(path: &Path) -> Option<Self> {
        let reader = MtkPackageReader::open_file(path).ok()?;
        reader.read_chunk_json("manifest").ok()
    }

    /// Read manifest from a package file or by locating `<fingerprint>.mtkcache` in `cache_dir`.
    pub fn read_from_dir(cache_dir: &Path) -> Option<Self> {
        if cache_dir.is_file() {
            return Self::read_from_package(cache_dir);
        }
        if let Ok(entries) = fs::read_dir(cache_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "mtkcache") {
                    if let Some(manifest) = Self::read_from_package(&p) {
                        return Some(manifest);
                    }
                }
            }
        }
        None
    }

    /// Check if manifest matches a given stack fingerprint and format version.
    pub fn is_valid_for(&self, fingerprint: &str) -> bool {
        self.format_version == ASSET_CACHE_FORMAT_VERSION && self.fingerprint == fingerprint
    }
}

/// High-performance reader for precompiled asset cache packages (`.mtkcache`).
pub struct AssetCacheReader {
    reader: MtkPackageReader,
}

impl AssetCacheReader {
    /// Opens an asset cache package file using memory mapping (read-only, no side effects).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MtkError> {
        let reader = MtkPackageReader::open_file(path)?;
        Ok(Self { reader })
    }

    /// Checks if a package at `path` is valid for the given fingerprint using only the 64-byte header.
    pub fn is_valid_cache_file(path: impl AsRef<Path>, fingerprint: &str) -> bool {
        let p = path.as_ref();
        if !p.exists() || !p.is_file() {
            return false;
        }
        if let Ok(hdr) = MtkPackageReader::read_header_only(p) {
            let fp_bytes = fingerprint_str_to_bytes16(fingerprint);
            hdr.fingerprint == fp_bytes && hdr.version_major == CURRENT_VERSION_MAJOR
        } else {
            false
        }
    }

    /// Returns package header reference.
    pub fn header(&self) -> &mtk_package::PackageHeader {
        self.reader.header()
    }

    /// Returns 16-byte fingerprint.
    pub fn fingerprint(&self) -> &[u8; 16] {
        self.reader.fingerprint()
    }

    /// Reads cache manifest metadata.
    pub fn manifest(&self) -> Result<CacheManifest, MtkError> {
        let manifest: CacheManifest = self.reader.read_chunk_json("manifest")?;
        Ok(manifest)
    }

    /// Loads baked model database directly from package.
    pub fn load_models(&self) -> Result<BakedModelDatabase, MtkError> {
        let raw = self.reader.read_chunk_decompressed("models/database")?;
        let db = BakedModelDatabase::from_binary(&raw)?;
        Ok(db)
    }

    /// Loads atlas address mapping directly from package.
    pub fn load_atlas_mapping(&self) -> Result<AtlasAddressMap, MtkError> {
        let json_str = self.load_atlas_mapping_json()?;
        let map = AtlasAddressMap::from_json(&json_str)?;
        Ok(map)
    }

    /// Loads raw atlas mapping JSON string directly from package.
    pub fn load_atlas_mapping_json(&self) -> Result<String, MtkError> {
        let json_str = self.reader.read_chunk_str("atlas/mapping")?;
        Ok(json_str)
    }

    /// Reads and decodes a texture chunk (PNG) directly into an RgbaBuffer in memory.
    pub fn read_texture_rgba(&self, chunk_id: &str) -> Result<mtk_texture::RgbaBuffer, MtkError> {
        self.read_texture_rgba_with_orientation(chunk_id, false)
    }

    /// Reads and decodes a texture chunk (PNG) with optional vertical flipping.
    pub fn read_texture_rgba_with_orientation(
        &self,
        chunk_id: &str,
        flip_v: bool,
    ) -> Result<mtk_texture::RgbaBuffer, MtkError> {
        let png_bytes = self.reader.read_chunk_decompressed(chunk_id)?;
        let mut buf = mtk_texture::RgbaBuffer::from_png_bytes(&png_bytes)?;
        if flip_v {
            buf.flip_v();
        }
        Ok(buf)
    }

    /// Reads and decodes a texture chunk (PNG) directly into normalized [0.0, 1.0] float32 pixels with optional vertical flipping.
    pub fn read_texture_f32_with_orientation(
        &self,
        chunk_id: &str,
        flip_v: bool,
    ) -> Result<(u32, u32, Vec<f32>), MtkError> {
        let png_bytes = self.reader.read_chunk_decompressed(chunk_id)?;
        let buf = mtk_texture::RgbaBuffer::from_png_bytes(&png_bytes)?;
        let (w, h) = (buf.width, buf.height);
        let f32_pixels = buf.to_f32_buffer(flip_v);
        Ok((w, h, f32_pixels))
    }

    /// Reads raw decompressed bytes for any chunk in package.
    pub fn read_chunk_bytes(&self, chunk_id: &str) -> Result<Vec<u8>, MtkError> {
        let bytes = self.reader.read_chunk_decompressed(chunk_id)?;
        Ok(bytes)
    }

    /// Loads biome resolver directly from package.
    pub fn load_biome_resolver(&self) -> Result<BiomeResolver, MtkError> {
        let json_str = self.reader.read_chunk_str("biome/mapping")?;
        let resolver = BiomeResolver::from_json(&json_str)?;
        Ok(resolver)
    }

    /// Loads cached CTM rules if present in the package.
    pub fn load_ctm_rules(&self) -> Result<Option<Vec<mtk_resource::CtmRule>>, MtkError> {
        if !self.reader.has_chunk("ctm/rules") {
            return Ok(None);
        }
        let json_str = self.reader.read_chunk_str("ctm/rules")?;
        let rules: Vec<mtk_resource::CtmRule> = serde_json::from_str(&json_str)?;
        Ok(Some(rules))
    }

    /// Loads a pre-populated `BannerBaker` containing cached base cloth and pattern sprites.
    pub fn load_banner_baker(&self) -> Result<mtk_texture::BannerBaker, MtkError> {
        let mut baker = mtk_texture::BannerBaker::default();
        let base_png = if self.reader.has_chunk("banner/base") {
            Some(self.reader.read_chunk_decompressed("banner/base")?)
        } else {
            None
        };
        let patterns: HashMap<String, Vec<u8>> = if self.reader.has_chunk("banner/patterns") {
            let bytes = self.reader.read_chunk_decompressed("banner/patterns")?;
            postcard::from_bytes(&bytes).map_err(|e| {
                MtkError::Resource(mtk_resource::ResourceError::NotFound(format!(
                    "Corrupted 'banner/patterns' chunk: {e}"
                )))
            })?
        } else {
            HashMap::new()
        };
        baker
            .load_from_bytes(base_png.as_deref(), &patterns)
            .map_err(MtkError::Texture)?;
        Ok(baker)
    }

    /// Extracts a specific atlas texture chunk to disk.
    pub fn extract_atlas_texture(
        &self,
        filename: &str,
        output_path: impl AsRef<Path>,
    ) -> Result<(), MtkError> {
        let chunk_id = format!("atlas/textures/{}", filename);
        self.reader.extract_chunk_to_file(&chunk_id, output_path)?;
        Ok(())
    }

    /// Extracts all atlas textures (`atlas/textures/*`) and `atlas_mapping.json` into the target directory.
    pub fn extract_all_atlas_textures(
        &self,
        output_dir: impl AsRef<Path>,
    ) -> Result<Vec<PathBuf>, MtkError> {
        let out_dir = output_dir.as_ref();
        fs::create_dir_all(out_dir)?;
        let mut extracted = Vec::new();

        for entry in self.reader.toc().iter_by_type(*b"ATLS") {
            if let Some(filename) = entry.identifier.strip_prefix("atlas/textures/") {
                let target = out_dir.join(filename);
                self.reader
                    .extract_chunk_to_file(&entry.identifier, &target)?;
                extracted.push(target);
            }
        }
        if let Ok(mapping_str) = self.reader.read_chunk_str("atlas/mapping") {
            let target = out_dir.join("atlas_mapping.json");
            fs::write(&target, mapping_str)?;
            extracted.push(target);
        }
        Ok(extracted)
    }

    /// Loads resident colormaps carried by this asset cache without global side effects.
    pub fn load_colormaps(&self) -> Result<Option<Arc<mtk_material::VanillaColormaps>>, MtkError> {
        let read = |name: &str| -> Result<Option<Vec<u8>>, MtkError> {
            let chunk_id = format!("biome/colormap/{name}");
            if self.reader.has_chunk(&chunk_id) {
                let bytes = self.reader.read_chunk_decompressed(&chunk_id)?;
                Ok(Some(bytes))
            } else {
                Ok(None)
            }
        };
        let grass = read("grass")?;
        let foliage = read("foliage")?;
        let dry_foliage = read("dry_foliage")?;

        if grass.is_none() && foliage.is_none() && dry_foliage.is_none() {
            return Ok(None);
        }

        let colormaps = mtk_material::VanillaColormaps::with_overrides(
            grass.as_deref(),
            foliage.as_deref(),
            dry_foliage.as_deref(),
        )
        .map_err(|e| MtkError::Texture(mtk_texture::error::TextureError::Baking(e)))?;

        Ok(Some(Arc::new(colormaps)))
    }

    /// Raw package reader accessor.
    pub fn package_reader(&self) -> &MtkPackageReader {
        &self.reader
    }
}
