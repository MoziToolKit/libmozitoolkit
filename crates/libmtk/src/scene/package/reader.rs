//! Reader for `.mtkscene` scene interchange packages.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use mtk_model::BakedModelDatabase;
use mtk_package::{MtkPackageReader, PackageProfile};
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use super::validator::{validate_scene_package, ValidationReport};
use crate::scene::metadata::SceneMetadata;
use crate::MtkError;

/// Unpacked contents of a `.mtkscene` scene interchange package.
#[derive(Debug, Clone)]
pub struct SceneBundle {
    pub metadata: SceneMetadata,
    pub sparse_voxels: SparseVoxelPayload,
    pub models: Option<BakedModelDatabase>,
    pub atlas_mapping: Option<AtlasAddressMap>,
    pub atlas_mapping_json: Option<String>,
    pub texture_names: Vec<String>,
    pub biome_resolver: Option<mtk_material::BiomeResolver>,
    pub colormaps: Option<Arc<mtk_material::VanillaColormaps>>,
    pub dynamic_entity_slots: Option<HashMap<String, (u16, u16)>>,
    pub ctm_solver: Option<mtk_resource::CtmSolver>,
}

/// High-level reader for `.mtkscene` packages.
pub struct ScenePackageReader {
    reader: MtkPackageReader,
    metadata: SceneMetadata,
}

impl ScenePackageReader {
    /// Opens an `.mtkscene` package from a file path.
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self, MtkError> {
        let reader = MtkPackageReader::open_file(path)?;
        Self::validate_and_build(reader)
    }

    /// Unpacks all major components of the scene package in a single pass.
    pub fn unpack(&self) -> Result<SceneBundle, MtkError> {
        let metadata = self.metadata.clone();
        let sparse_voxels = self.read_sparse_voxels()?;
        let models = self.read_models()?;
        let atlas_mapping = self.read_atlas_mapping()?;
        let atlas_mapping_json = if self.reader.has_chunk("atlas/mapping") {
            Some(self.reader.read_chunk_str("atlas/mapping")?)
        } else {
            None
        };
        let texture_names = self
            .reader
            .toc()
            .iter_by_type(*b"ATLS")
            .filter(|e| e.identifier.starts_with("atlas/textures/"))
            .map(|e| e.identifier.clone())
            .collect();
        let biome_resolver = self.read_biome_resolver()?;
        let colormaps = self.read_colormaps()?;
        let dynamic_entity_slots = self.read_dynamic_entity_slots()?;
        let ctm_solver = self.read_ctm_solver()?;

        Ok(SceneBundle {
            metadata,
            sparse_voxels,
            models,
            atlas_mapping,
            atlas_mapping_json,
            texture_names,
            biome_resolver,
            colormaps,
            dynamic_entity_slots,
            ctm_solver,
        })
    }

    /// Creates a reader from in-memory package bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, MtkError> {
        let reader = MtkPackageReader::from_bytes(bytes)?;
        Self::validate_and_build(reader)
    }

    fn validate_and_build(reader: MtkPackageReader) -> Result<Self, MtkError> {
        let profile = reader.header().profile;
        if profile != PackageProfile::SceneInterchange {
            return Err(MtkError::InvalidSceneProfile(profile));
        }

        let metadata: SceneMetadata = reader.read_chunk_json("manifest")?;
        if metadata.format_version != crate::scene::metadata::SCENE_PACKAGE_FORMAT_VERSION {
            return Err(MtkError::UnsupportedSceneFormatVersion(
                metadata.format_version,
            ));
        }

        Ok(Self { reader, metadata })
    }

    /// Returns the global scene metadata.
    #[inline]
    pub fn metadata(&self) -> &SceneMetadata {
        &self.metadata
    }

    /// Returns the raw underlying package reader.
    #[inline]
    pub fn inner_reader(&self) -> &MtkPackageReader {
        &self.reader
    }

    /// Reads and deserializes the sparse voxel grid payload.
    pub fn read_sparse_voxels(&self) -> Result<SparseVoxelPayload, MtkError> {
        let bytes = self.reader.read_chunk_decompressed("voxels/sparse")?;
        let voxels: SparseVoxelPayload = postcard::from_bytes(&bytes)?;
        Ok(voxels)
    }

    /// Reads the tree-shaken baked model database if present.
    pub fn read_models(&self) -> Result<Option<BakedModelDatabase>, MtkError> {
        if !self.reader.has_chunk("models/database") {
            return Ok(None);
        }
        let bytes = self.reader.read_chunk_decompressed("models/database")?;
        let db = BakedModelDatabase::from_binary(&bytes)?;
        Ok(Some(db))
    }

    /// Reads the scene mini-atlas address map if present.
    pub fn read_atlas_mapping(&self) -> Result<Option<AtlasAddressMap>, MtkError> {
        if !self.reader.has_chunk("atlas/mapping") {
            return Ok(None);
        }
        let json = self.reader.read_chunk_str("atlas/mapping")?;
        let map = AtlasAddressMap::from_json(&json)?;
        Ok(Some(map))
    }

    /// Reads all mini-atlas textures (`atlas/textures/*.png`) present in the package.
    pub fn read_atlas_textures(&self) -> Result<HashMap<String, Vec<u8>>, MtkError> {
        let mut textures = HashMap::new();
        for entry in self.reader.toc().iter_by_type(*b"ATLS") {
            if entry.identifier.starts_with("atlas/textures/") {
                let bytes = self.reader.get_chunk_raw(&entry.identifier)?;
                textures.insert(entry.identifier.clone(), bytes.to_vec());
            }
        }
        Ok(textures)
    }

    /// Reads and decodes a texture chunk (PNG) directly into an `RgbaBuffer` in memory with optional vertical flip.
    pub fn read_texture_rgba_with_orientation(
        &self,
        chunk_id: &str,
        flip_v: bool,
    ) -> Result<mtk_texture::RgbaBuffer, MtkError> {
        let normalized_id = if chunk_id.starts_with("atlas/textures/") {
            chunk_id.to_string()
        } else {
            format!("atlas/textures/{}", chunk_id)
        };
        let png_bytes = self.reader.read_chunk_decompressed(&normalized_id)?;
        let mut buf = mtk_texture::RgbaBuffer::from_png_bytes(&png_bytes)?;
        if flip_v {
            buf.flip_v();
        }
        Ok(buf)
    }

    /// Reads and decodes a texture chunk (PNG) directly into normalized [0.0, 1.0] float32 pixels.
    pub fn read_texture_f32_with_orientation(
        &self,
        chunk_id: &str,
        flip_v: bool,
    ) -> Result<(u32, u32, Vec<f32>), MtkError> {
        let normalized_id = if chunk_id.starts_with("atlas/textures/") {
            chunk_id.to_string()
        } else {
            format!("atlas/textures/{}", chunk_id)
        };
        let png_bytes = self.reader.read_chunk_decompressed(&normalized_id)?;
        let buf = mtk_texture::RgbaBuffer::from_png_bytes(&png_bytes)?;
        let (w, h) = (buf.width, buf.height);
        let f32_pixels = buf.to_f32_buffer(flip_v);
        Ok((w, h, f32_pixels))
    }

    /// Extracts a single texture chunk to disk.
    pub fn extract_atlas_texture(
        &self,
        chunk_id: &str,
        output_path: impl AsRef<Path>,
    ) -> Result<(), MtkError> {
        let normalized_id = if chunk_id.starts_with("atlas/textures/") {
            chunk_id.to_string()
        } else {
            format!("atlas/textures/{}", chunk_id)
        };
        let bytes = self.reader.get_chunk_raw(&normalized_id)?;
        if let Some(parent) = output_path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, bytes)?;
        Ok(())
    }

    /// Checks whether the package contains custom resource pack colormaps.
    pub fn has_colormaps(&self) -> bool {
        self.reader.has_chunk("biome/colormap/grass")
            || self.reader.has_chunk("biome/colormap/foliage")
            || self.reader.has_chunk("biome/colormap/dry_foliage")
    }

    /// Reads raw decompressed PNG bytes for a specific colormap channel (e.g. `"grass"`, `"foliage"`).
    pub fn read_colormap(&self, name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        let chunk_id = format!("biome/colormap/{name}");
        if !self.reader.has_chunk(&chunk_id) {
            return Ok(None);
        }
        let bytes = self
            .reader
            .read_chunk_decompressed(&chunk_id)
            .map_err(MtkError::Package)?;
        Ok(Some(bytes))
    }

    /// Reads resident colormaps carried by this scene package without global side effects.
    pub fn read_colormaps(&self) -> Result<Option<Arc<mtk_material::VanillaColormaps>>, MtkError> {
        let grass = self.read_colormap("grass")?;
        let foliage = self.read_colormap("foliage")?;
        let dry_foliage = self.read_colormap("dry_foliage")?;

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

    /// Re-installs or updates the resident colormaps from this package into the process-level cache.
    pub fn install_colormaps(&self) -> Result<(), MtkError> {
        install_scene_colormaps(&self.reader)
    }

    /// Reads the packaged BiomeResolver if present in the package.
    pub fn read_biome_resolver(&self) -> Result<Option<mtk_material::BiomeResolver>, MtkError> {
        let chunk_id = if self.reader.has_chunk("biome/mapping") {
            "biome/mapping"
        } else if self.reader.has_chunk("biome/resolver") {
            // DEPRECATED: "biome/resolver" chunk ID is deprecated and will be removed in version 2.2. Use "biome/mapping".
            "biome/resolver"
        } else {
            return Ok(None);
        };
        let json_str = self.reader.read_chunk_str(chunk_id)?;
        let resolver = mtk_material::BiomeResolver::from_json(&json_str)?;
        Ok(Some(resolver))
    }

    /// Checks whether custom CTM rules are carried in this package.
    pub fn has_ctm_rules(&self) -> bool {
        self.reader.has_chunk("ctm/rules")
    }

    /// Reads raw CTM rules JSON from this package if present.
    pub fn read_ctm_rules_json(&self) -> Result<Option<String>, MtkError> {
        if !self.has_ctm_rules() {
            return Ok(None);
        }
        let json = self.reader.read_chunk_str("ctm/rules")?;
        Ok(Some(json))
    }

    /// Reads and constructs an indexed CtmSolver from the packaged rules if present.
    pub fn read_ctm_solver(&self) -> Result<Option<mtk_resource::CtmSolver>, MtkError> {
        if let Some(json) = self.read_ctm_rules_json()? {
            let solver = mtk_resource::CtmSolver::from_json(&json).map_err(MtkError::Json)?;
            Ok(Some(solver))
        } else {
            Ok(None)
        }
    }

    /// Checks whether dynamic entity slots mapping is carried in this package.
    pub fn has_dynamic_entity_slots(&self) -> bool {
        self.reader.has_chunk("entity/slots")
    }

    /// Reads dynamic entity slots mapping `key -> (chunk_id, slot_idx)` from this package if present.
    pub fn read_dynamic_entity_slots(
        &self,
    ) -> Result<Option<HashMap<String, (u16, u16)>>, MtkError> {
        if !self.has_dynamic_entity_slots() {
            return Ok(None);
        }
        let map: HashMap<String, (u16, u16)> = self.reader.read_chunk_json("entity/slots")?;
        Ok(Some(map))
    }

    /// Returns the effective capabilities of this scene package.
    pub fn effective_capabilities(&self) -> crate::scene::metadata::SceneCapabilities {
        let mut caps = self.metadata.capabilities;
        if !caps.has_colormaps && self.has_colormaps() {
            caps.has_colormaps = true;
        }
        if !caps.has_ctm && self.has_ctm_rules() {
            caps.has_ctm = true;
        }
        if let Ok(Some(map)) = self.read_atlas_mapping() {
            if !caps.has_pbr {
                caps.has_pbr = map.chunks.iter().any(|c| {
                    c.has_normal
                        || c.has_orm
                        || c.has_aux
                        || c.pbr.is_some()
                        || self
                            .reader
                            .has_chunk(&format!("atlas/textures/{}_n.png", c.file_stem()))
                        || self
                            .reader
                            .has_chunk(&format!("atlas/textures/{}_orm.png", c.file_stem()))
                        || self
                            .reader
                            .has_chunk(&format!("atlas/textures/{}_aux.png", c.file_stem()))
                });
            }
            if !caps.has_dynamic_entities {
                caps.has_dynamic_entities = (self.has_dynamic_entity_slots()
                    && self
                        .read_dynamic_entity_slots()
                        .map(|opt| opt.map(|s| !s.is_empty()).unwrap_or(false))
                        .unwrap_or(false))
                    || map.sprites.keys().any(|k| {
                        k.path.starts_with("skin/")
                            || k.path.starts_with("banner/")
                            || k.namespace == "skin"
                            || k.namespace == "banner"
                    });
            }
        } else if !caps.has_dynamic_entities && self.has_dynamic_entity_slots() {
            caps.has_dynamic_entities = self
                .read_dynamic_entity_slots()
                .map(|opt| opt.map(|s| !s.is_empty()).unwrap_or(false))
                .unwrap_or(false);
        }
        caps
    }

    /// Performs an integrity check on models, atlas textures, colormaps, dynamic entities, and CTM rules.
    pub fn validate(&self) -> Result<ValidationReport, MtkError> {
        validate_scene_package(self)
    }
}

/// Installs resource-pack colormap overrides carried by a scene package into global cache.
pub(crate) fn install_scene_colormaps(reader: &MtkPackageReader) -> Result<(), MtkError> {
    let read = |name: &str| -> Result<Option<Vec<u8>>, MtkError> {
        let chunk_id = format!("biome/colormap/{name}");
        if reader.has_chunk(&chunk_id) {
            let bytes = reader.read_chunk_decompressed(&chunk_id)?;
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    };
    let grass = read("grass")?;
    let foliage = read("foliage")?;
    let dry_foliage = read("dry_foliage")?;

    if grass.is_none() && foliage.is_none() && dry_foliage.is_none() {
        mtk_material::biome::clear_cached_colormaps();
        return Ok(());
    }

    mtk_material::biome::set_cached_colormaps(
        grass.as_deref(),
        foliage.as_deref(),
        dry_foliage.as_deref(),
    )
    .map_err(|e| MtkError::Texture(mtk_texture::error::TextureError::Baking(e)))?;

    Ok(())
}
