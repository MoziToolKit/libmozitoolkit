//! Unified package reader and writer for `.mtkscene` scene interchange files.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use mtk_model::BakedModelDatabase;
use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, MtkPackageReader, PackageProfile};
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use super::metadata::SceneMetadata;
use crate::MtkError;

/// Integrity and validation report produced by `ScenePackageReader::validate`.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
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
        if metadata.format_version != super::metadata::SCENE_PACKAGE_FORMAT_VERSION {
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
    ///
    /// If the package was generated with an older version where `capabilities` was omitted
    /// (yielding all false defaults from serde), this method falls back to chunk presence probing.
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

    /// Performs an integrity check on models, atlas textures, colormaps, and CTM rules.
    pub fn validate(&self) -> Result<ValidationReport, MtkError> {
        let mut report = ValidationReport::default();

        // 1. Check colormaps PNG validity and decodability
        if self.has_colormaps() {
            match self.read_colormaps() {
                Ok(Some(_)) => {}
                Ok(None) => {}
                Err(e) => {
                    report
                        .errors
                        .push(format!("Corrupted colormap in package: {e}"));
                }
            }
        }

        // 2. Check Biome Resolver
        if self.reader.has_chunk("biome/mapping") || self.reader.has_chunk("biome/resolver") {
            if let Err(e) = self.read_biome_resolver() {
                report
                    .errors
                    .push(format!("Invalid BiomeResolver in package: {e}"));
            }
        }

        // 3. Read and check Atlas Mapping
        let maybe_atlas_map = match self.read_atlas_mapping() {
            Ok(Some(map)) => Some(map),
            Ok(None) => {
                report
                    .errors
                    .push("Scene package is missing 'atlas/mapping' chunk".to_string());
                None
            }
            Err(e) => {
                report
                    .errors
                    .push(format!("Corrupted 'atlas/mapping' chunk: {e}"));
                None
            }
        };

        // 4. Verify that each atlas texture slice referenced by chunk metadata actually exists
        if let Some(ref map) = maybe_atlas_map {
            for chunk in &map.chunks {
                for slot in chunk.active_textures() {
                    let chunk_path = format!("atlas/textures/{}", slot.file_name);
                    if !self.reader.has_chunk(&chunk_path) {
                        report.errors.push(format!(
                            "Missing atlas texture slice '{chunk_path}' for chunk_id {}",
                            chunk.chunk_id
                        ));
                    }
                }
            }
        }

        // 5. Read Models Database
        let maybe_models = match self.read_models() {
            Ok(Some(models)) => Some(models),
            Ok(None) => {
                report
                    .errors
                    .push("Scene package is missing 'models/database' chunk".to_string());
                None
            }
            Err(e) => {
                report
                    .errors
                    .push(format!("Corrupted 'models/database' chunk: {e}"));
                None
            }
        };

        // 6. Check Models (elements, faces, obj_faces, and variant groups) vs Atlas Mapping
        if let (Some(models), Some(map)) = (maybe_models.as_ref(), maybe_atlas_map.as_ref()) {
            let check_face = |face_tex: &str, source_desc: &str, rep: &mut ValidationReport| {
                if !face_tex.is_empty() && map.resolve_texture_key(face_tex).is_none() {
                    rep.warnings.push(format!(
                        "{source_desc} references texture '{face_tex}' not found in mini-atlas"
                    ));
                }
            };

            let check_model =
                |model: &mtk_model::BakedModel, desc: &str, rep: &mut ValidationReport| {
                    for element in &model.elements {
                        for face in element.faces.values() {
                            check_face(&face.texture, &format!("{desc} (element face)"), rep);
                        }
                    }
                    for face in &model.faces {
                        check_face(&face.texture, &format!("{desc} (directional face)"), rep);
                    }
                    for obj_face in &model.obj_faces {
                        check_face(&obj_face.texture, &format!("{desc} (obj face)"), rep);
                    }
                };

            for (state, model) in &models.models {
                check_model(model, &format!("Model for state '{state}'"), &mut report);
            }

            for (state, group) in &models.variant_groups {
                for (idx, model) in group.models.iter().enumerate() {
                    check_model(
                        model,
                        &format!("Variant group '{state}' [variant {idx}]"),
                        &mut report,
                    );
                }
            }
        }

        // 7. Check CTM Rules vs Mini-Atlas
        if self.has_ctm_rules() {
            match self.read_ctm_solver() {
                Ok(Some(solver)) => {
                    if let Some(ref map) = maybe_atlas_map {
                        for rule in &solver.rules {
                            for tile in rule.tiles.iter().flatten() {
                                if map.lookup(tile).is_none() {
                                    report.warnings.push(format!(
                                        "CTM rule '{}' references tile '{}' not found in mini-atlas",
                                        rule.name, tile
                                    ));
                                }
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    report
                        .errors
                        .push(format!("Corrupted CTM rules JSON in package: {e}"));
                }
            }
        }

        // 8. Check Dynamic Entity Slots vs Mini-Atlas Sprites
        if self.has_dynamic_entity_slots() {
            match self.read_dynamic_entity_slots() {
                Ok(Some(slots)) => {
                    if let Some(ref map) = maybe_atlas_map {
                        for (key, (_chunk_id, _slot_idx)) in &slots {
                            if map.resolve_texture_key(key).is_none() {
                                report.errors.push(format!(
                                    "Dynamic entity slot '{key}' cannot be resolved to any sprite in mini-atlas"
                                ));
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    report
                        .errors
                        .push(format!("Corrupted entity/slots in package: {e}"));
                }
            }
        }

        report.is_valid = report.errors.is_empty();
        Ok(report)
    }
}

/// Installs resource-pack colormap overrides carried by a scene package.
///
/// Missing channels fall back to embedded vanilla; a package without colormap
/// chunks clears any previously active overrides.
fn install_scene_colormaps(reader: &MtkPackageReader) -> Result<(), MtkError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_model::BlockEntityData;

    #[test]
    fn test_scene_package_roundtrip() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_scene_{}_{}.mtkscene",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut voxels = SparseVoxelPayload::new();
        voxels.bounds = [0, 0, 0, 16, 16, 16];
        voxels.push(
            1,
            2,
            3,
            "minecraft:stone",
            "minecraft:plains",
            None,
            Some(15),
        );
        voxels.push(
            1,
            3,
            3,
            "minecraft:item_frame[facing=north]",
            "minecraft:plains",
            Some(&BlockEntityData::new_item_frame(
                "minecraft:compass",
                1,
                0,
                false,
                false,
            )),
            Some(14),
        );

        let mut metadata = SceneMetadata::new(voxels.bounds, voxels.len());
        metadata.unique_states_count = voxels.state_palette.len();
        metadata.unique_biomes_count = voxels.biome_palette.len();
        metadata.unique_entities_count = voxels.entity_palette.len();

        let dummy_png = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG header stub
        let textures = vec![("atlas_0.png".to_string(), dummy_png.clone())];

        let mut model_db = BakedModelDatabase::new();
        model_db.insert(
            "minecraft:stone".to_string(),
            mtk_model::baker::baked_model::BakedModel {
                block_state: "minecraft:stone".to_string(),
                is_cube: true,
                is_opaque: true,
                ..Default::default()
            },
        );

        let mut write_req = SceneWriteRequest::new(&metadata, &voxels, &textures);
        write_req.models = Some(&model_db);
        write_scene_package(&temp_path, write_req).expect("Write scene package");

        let reader = ScenePackageReader::open_file(&temp_path).expect("Open scene package");
        assert_eq!(reader.metadata().voxel_count, 2);
        assert_eq!(reader.metadata().bounds, [0, 0, 0, 16, 16, 16]);
        assert!(!reader.has_ctm_rules());
        assert!(reader.read_ctm_solver().unwrap().is_none());

        let read_models = reader
            .read_models()
            .expect("Read models")
            .expect("Models present");
        assert_eq!(read_models.len(), 1);
        assert!(read_models.get("minecraft:stone").is_some());

        let read_voxels = reader.read_sparse_voxels().expect("Read sparse voxels");
        assert_eq!(read_voxels.len(), 2);
        assert_eq!(read_voxels.state_palette, voxels.state_palette);
        assert_eq!(read_voxels.columns.x, voxels.columns.x);

        let read_textures = reader.read_atlas_textures().expect("Read textures");
        assert_eq!(read_textures.len(), 1);
        assert!(read_textures.contains_key("atlas/textures/atlas_0.png"));
        assert_eq!(
            read_textures.get("atlas/textures/atlas_0.png").unwrap(),
            &dummy_png
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_scene_package_gates_profile_and_version() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_gate_{}_{}.mtkscene",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // 1. Write an AssetCache profile package
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::AssetCache,
            [0u8; 16],
        )
        .expect("Create writer");
        let mut metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();
        writer.finish().unwrap();

        // Must fail with InvalidSceneProfile
        let err = match ScenePackageReader::open_file(&temp_path) {
            Ok(_) => panic!("Expected error, got Ok"),
            Err(e) => e,
        };
        match err {
            MtkError::InvalidSceneProfile(PackageProfile::AssetCache) => {}
            other => panic!("Expected InvalidSceneProfile, got {:?}", other),
        }

        // 2. Write with wrong format_version (e.g. 99)
        metadata.format_version = 99;
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::SceneInterchange,
            [0u8; 16],
        )
        .expect("Create writer");
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();
        writer.finish().unwrap();

        let err = match ScenePackageReader::open_file(&temp_path) {
            Ok(_) => panic!("Expected error, got Ok"),
            Err(e) => e,
        };
        match err {
            MtkError::UnsupportedSceneFormatVersion(99) => {}
            other => panic!(
                "Expected UnsupportedSceneFormatVersion(99), got {:?}",
                other
            ),
        }

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_scene_package_corrupted_colormap_error() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_corrupt_cm_{}.mtkscene",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::SceneInterchange,
            [0u8; 16],
        )
        .unwrap();
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();
        writer
            .add_chunk(
                *b"CMAP",
                "biome/colormap/grass",
                b"not a valid png file",
                &ChunkWriteOptions::raw(),
            )
            .unwrap();
        writer.finish().unwrap();

        let reader = ScenePackageReader::open_file(&temp_path).unwrap();
        let res = reader.read_colormaps();
        assert!(res.is_err(), "Corrupted colormap must return Err, not Ok");

        let validation = reader.validate().expect("validate report");
        assert!(!validation.is_valid);
        assert!(!validation.errors.is_empty());

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_validate_missing_atlas_mapping_and_models() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_val_missing_{}.mtkscene",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::SceneInterchange,
            [0u8; 16],
        )
        .unwrap();
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();
        writer.finish().unwrap();

        let reader = ScenePackageReader::open_file(&temp_path).unwrap();
        let report = reader.validate().unwrap();
        assert!(!report.is_valid);
        assert!(report.errors.iter().any(|e| e.contains("atlas/mapping")));
        assert!(report.errors.iter().any(|e| e.contains("models/database")));

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_validate_missing_texture_slice_and_unmapped_model_texture() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_val_textures_{}.mtkscene",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::SceneInterchange,
            [0u8; 16],
        )
        .unwrap();
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();

        let mut atlas_map = AtlasAddressMap::new();
        atlas_map.chunks.push(mtk_texture::AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 0,
            width: 512,
            height: 512,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: vec![mtk_texture::AtlasTextureSlot {
                role: "albedo".to_string(),
                file_name: "atlas_0.png".to_string(),
                colorspace: "sRGB".to_string(),
            }],
        });
        writer
            .add_json_chunk(
                *b"MAPG",
                "atlas/mapping",
                &atlas_map,
                &ChunkWriteOptions::raw(),
            )
            .unwrap();

        let mut model_db = BakedModelDatabase::new();
        let mut model = mtk_model::baker::baked_model::BakedModel {
            block_state: "minecraft:stone".to_string(),
            ..Default::default()
        };
        model.faces[0].texture = "minecraft:block/missing_stone".to_string();
        model_db.insert("minecraft:stone".to_string(), model);

        let model_bytes = model_db.to_binary().unwrap();
        writer
            .add_chunk(
                *b"MODL",
                "models/database",
                &model_bytes,
                &ChunkWriteOptions::raw(),
            )
            .unwrap();
        writer.finish().unwrap();

        let reader = ScenePackageReader::open_file(&temp_path).unwrap();
        let report = reader.validate().unwrap();

        // 1. Missing texture slice error (atlas_0.png was declared in map but not written to package)
        assert!(!report.is_valid);
        assert!(report.errors.iter().any(|e| e.contains("atlas_0.png")));

        // 2. Unmapped texture warning (missing_stone is not in atlas map)
        assert!(report.warnings.iter().any(|w| w.contains("missing_stone")));

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_validate_dynamic_entity_slots_missing_chunk() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_val_dyn_slots_{}.mtkscene",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::SceneInterchange,
            [0u8; 16],
        )
        .unwrap();
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();

        let atlas_map = AtlasAddressMap::new();
        writer
            .add_json_chunk(
                *b"MAPG",
                "atlas/mapping",
                &atlas_map,
                &ChunkWriteOptions::raw(),
            )
            .unwrap();
        let model_db = BakedModelDatabase::new();
        let model_bytes = model_db.to_binary().unwrap();
        writer
            .add_chunk(
                *b"MODL",
                "models/database",
                &model_bytes,
                &ChunkWriteOptions::raw(),
            )
            .unwrap();

        let mut slots = HashMap::new();
        slots.insert("custom_skin_steve".to_string(), (0xFFF0u16, 0u16));
        writer
            .add_json_chunk(*b"ESLT", "entity/slots", &slots, &ChunkWriteOptions::raw())
            .unwrap();
        writer.finish().unwrap();

        let reader = ScenePackageReader::open_file(&temp_path).unwrap();
        let report = reader.validate().unwrap();
        assert!(!report.is_valid);
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("custom_skin_steve")));

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_validate_dynamic_entity_slots_valid() {
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!(
            "test_validate_dyn_slots_valid_{}.mtkscene",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let metadata = SceneMetadata::new([0, 0, 0, 1, 1, 1], 1);
        let mut writer = mtk_package::MtkPackageFileWriter::create(
            &temp_path,
            PackageProfile::SceneInterchange,
            [0u8; 16],
        )
        .unwrap();
        writer
            .add_json_chunk(*b"META", "manifest", &metadata, &ChunkWriteOptions::raw())
            .unwrap();

        let mut map = AtlasAddressMap::new();
        let chunk_meta = mtk_texture::AtlasChunkMeta {
            chunk_id: 0,
            category: "scene".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 64,
            height: 64,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        map.chunks.push(chunk_meta);

        let sprite_loc = mtk_texture::AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 64, 64],
            frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            ..Default::default()
        };
        map.sprites.insert(
            mtk_resource::ResourceLocation::new("minecraft", "skin/player_hero"),
            sprite_loc,
        );

        writer
            .add_json_chunk(*b"MAPG", "atlas/mapping", &map, &ChunkWriteOptions::raw())
            .unwrap();
        writer
            .add_chunk(
                *b"TX2D",
                "atlas/textures/scene_chunk_001.png",
                &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
                &ChunkWriteOptions::raw(),
            )
            .unwrap();

        let model_db = BakedModelDatabase::new();
        let model_bytes = model_db.to_binary().unwrap();
        writer
            .add_chunk(
                *b"MODL",
                "models/database",
                &model_bytes,
                &ChunkWriteOptions::raw(),
            )
            .unwrap();

        let mut slots = HashMap::new();
        slots.insert("player_Hero".to_string(), (0xFFF0u16, 0u16));
        writer
            .add_json_chunk(*b"ESLT", "entity/slots", &slots, &ChunkWriteOptions::raw())
            .unwrap();
        writer.finish().unwrap();

        let reader = ScenePackageReader::open_file(&temp_path).unwrap();
        let report = reader.validate().unwrap();
        assert!(
            report.is_valid,
            "Expected valid report, but got errors: {:?}",
            report.errors
        );
        assert!(report.errors.is_empty());

        let _ = std::fs::remove_file(&temp_path);
    }
}
