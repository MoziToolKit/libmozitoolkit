//! Unified package reader and writer for `.mtkscene` scene interchange files.

use std::collections::HashMap;
use std::path::Path;

use mtk_model::BakedModelDatabase;
use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, MtkPackageReader, PackageProfile};
use mtk_texture::atlas::AtlasAddressMap;
use mtk_voxel::SparseVoxelPayload;

use super::metadata::SceneMetadata;
use crate::MtkError;

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
}

/// Helper options for authoring `.mtkscene` packages.
#[derive(Debug, Clone, Default)]
pub struct ScenePackageWriteOptions {
    /// Optional content fingerprint (defaults to zeroed 16 bytes).
    pub fingerprint: Option<[u8; 16]>,
}

/// High-level function to write an `.mtkscene` package file to disk.
pub fn write_scene_package(
    path: impl AsRef<Path>,
    metadata: &SceneMetadata,
    voxels: &SparseVoxelPayload,
    models: Option<&BakedModelDatabase>,
    atlas_mapping: Option<&AtlasAddressMap>,
    atlas_textures: &[(String, Vec<u8>)],
    options: Option<&ScenePackageWriteOptions>,
) -> Result<(), MtkError> {
    let fp = options.and_then(|o| o.fingerprint).unwrap_or([0u8; 16]);

    let mut writer = MtkPackageFileWriter::create(path, PackageProfile::SceneInterchange, fp)?;

    // 1. Scene Metadata
    writer.add_json_chunk(
        *b"META",
        "manifest",
        metadata,
        &ChunkWriteOptions::zstd_fast(),
    )?;

    // 2. Sparse Voxels
    let voxl_bytes = postcard::to_allocvec(voxels)?;
    writer.add_chunk(
        *b"VOXL",
        "voxels/sparse",
        &voxl_bytes,
        &ChunkWriteOptions::zstd_fast(),
    )?;

    // 3. Models (optional)
    if let Some(db) = models {
        let db_bytes = db.to_binary()?;
        writer.add_chunk(
            *b"MODL",
            "models/database",
            &db_bytes,
            &ChunkWriteOptions::zstd_fast(),
        )?;
    }

    // 4. Atlas mapping (optional)
    if let Some(map) = atlas_mapping {
        let map_json = map.to_json()?;
        writer.add_str_chunk(
            *b"ATLS",
            "atlas/mapping",
            &map_json,
            &ChunkWriteOptions::zstd_fast(),
        )?;
    }

    // 5. Atlas textures (optional)
    for (name, bytes) in atlas_textures {
        let chunk_id = if name.starts_with("atlas/textures/") {
            name.clone()
        } else {
            format!("atlas/textures/{}", name)
        };
        writer.add_chunk(*b"ATLS", chunk_id, bytes, &ChunkWriteOptions::raw())?;
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

        write_scene_package(
            &temp_path,
            &metadata,
            &voxels,
            Some(&model_db),
            None,
            &textures,
            None,
        )
        .expect("Write scene package");

        let reader = ScenePackageReader::open_file(&temp_path).expect("Open scene package");
        assert_eq!(reader.metadata().voxel_count, 2);
        assert_eq!(reader.metadata().bounds, [0, 0, 0, 16, 16, 16]);

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
}
