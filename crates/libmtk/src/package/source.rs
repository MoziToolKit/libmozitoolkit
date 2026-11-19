//! Profile-agnostic asset source.
//!
//! A host that wants to mesh, tint and texture a world needs the same payloads whether they arrive
//! in a `.mtkcache` (baked from a pack stack) or in a `.mtkscene` (a self-contained scene). This
//! module loads them once, from whichever profile the file declares, and keeps the package open so
//! textures can be read on demand instead of copied into memory up front.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_model::BakedModelDatabase;
use mtk_package::{MtkPackageReader, PackageProfile};
use mtk_resource::CtmSolver;
use mtk_texture::{AtlasAddressMap, RgbaBuffer};

use crate::package::manifest::PackageManifest;
use crate::package::{chunks, PackageContents};
use crate::MtkError;

/// What a package turned out to hold, and how to read the rest of it lazily.
pub struct AssetBundle {
    path: PathBuf,
    profile: PackageProfile,
    reader: MtkPackageReader,
    manifest: PackageManifest,

    /// Baked model database, when the package carries one.
    pub models: Option<Arc<BakedModelDatabase>>,
    /// Atlas address map and its raw JSON (the JSON is what a host re-serializes).
    pub atlas_mapping: Option<AtlasAddressMap>,
    /// Raw atlas address map JSON, kept alongside the decoded form.
    pub atlas_mapping_json: Option<String>,
    /// Resident colormaps, when the pack stack shipped custom ones.
    pub colormaps: Option<Arc<VanillaColormaps>>,
    /// Biome name mapping.
    pub biome_resolver: Option<BiomeResolver>,
    /// Indexed CTM solver, when CTM rules are bundled.
    pub ctm_solver: Option<CtmSolver>,
    /// Dynamic entity slot addressing (`key -> (chunk id, slot)`).
    pub entity_slots: Option<HashMap<String, (u16, u16)>>,
    /// Every atlas texture chunk identifier present in the package.
    pub texture_names: Vec<String>,
}

impl AssetBundle {
    /// Opens any package and loads the payloads shared by both profiles.
    ///
    /// The profile comes from the header, so a host does not have to know whether it holds a cache
    /// or a scene package. Manifest revisions are validated against that profile.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MtkError> {
        let path = path.as_ref().to_path_buf();
        let reader = MtkPackageReader::open_file(&path).map_err(MtkError::Package)?;
        let profile = reader.header().profile;
        let manifest = Self::read_manifest(&reader, profile)?;

        let contents = PackageContents::new(&reader);
        let models = contents.models()?.map(Arc::new);
        let atlas_mapping = contents.atlas_mapping()?;
        let atlas_mapping_json = contents.atlas_mapping_json()?;
        let colormaps = contents.colormaps()?;
        let biome_resolver = contents.biome_resolver()?;
        let ctm_solver = contents.ctm_solver()?;
        let entity_slots = contents.entity_slots()?;
        let texture_names = reader
            .iter_chunk_identifiers(*b"ATLS")
            .filter(|id| id.starts_with(chunks::ATLAS_TEXTURE_PREFIX))
            .map(|id| id.to_string())
            .collect();

        Ok(Self {
            path,
            profile,
            reader,
            manifest,
            models,
            atlas_mapping,
            atlas_mapping_json,
            colormaps,
            biome_resolver,
            ctm_solver,
            entity_slots,
            texture_names,
        })
    }

    fn read_manifest(
        reader: &MtkPackageReader,
        profile: PackageProfile,
    ) -> Result<PackageManifest, MtkError> {
        if !reader.has_chunk("manifest") {
            return Err(MtkError::Package(mtk_package::PackageError::ChunkNotFound(
                "manifest".to_string(),
            )));
        }
        let manifest: PackageManifest = reader.read_chunk_json("manifest")?;
        match profile {
            PackageProfile::AssetCache => {
                if manifest.format_version != crate::package::ASSET_CACHE_FORMAT_VERSION {
                    return Err(MtkError::UnsupportedCacheFormatVersion {
                        found: manifest.format_version,
                    });
                }
            }
            PackageProfile::SceneInterchange => {
                if !manifest.is_valid_scene() {
                    return Err(MtkError::UnsupportedSceneFormatVersion {
                        found: manifest.format_version,
                    });
                }
            }
        }
        Ok(manifest)
    }

    /// Path the bundle was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Container profile of the package.
    pub fn profile(&self) -> PackageProfile {
        self.profile
    }

    /// Manifest of the package.
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }

    /// Typed access to any other payload (voxels, banner data, ...).
    pub fn contents(&self) -> PackageContents<'_> {
        PackageContents::new(&self.reader)
    }

    /// Baked model database, failing when the package has none.
    ///
    /// A host that is about to mesh needs this; anything else can look at [`Self::models`].
    pub fn models_required(&self) -> Result<&BakedModelDatabase, MtkError> {
        self.models.as_deref().ok_or_else(|| {
            MtkError::Package(mtk_package::PackageError::ChunkNotFound(
                chunks::MODELS_DATABASE.to_string(),
            ))
        })
    }

    /// Reads an atlas texture to RGBA by chunk id or file stem.
    pub fn texture_rgba(&self, name_or_id: &str, flip_v: bool) -> Result<RgbaBuffer, MtkError> {
        self.contents().texture_rgba(name_or_id, flip_v)
    }

    /// Reads an atlas texture to normalized float pixels by chunk id or file stem.
    pub fn texture_f32(
        &self,
        name_or_id: &str,
        flip_v: bool,
    ) -> Result<(u32, u32, Vec<f32>), MtkError> {
        self.contents().texture_f32(name_or_id, flip_v)
    }

    /// Sparse voxels, for the scene profile.
    pub fn sparse_voxels(&self) -> Result<mtk_voxel::SparseVoxelPayload, MtkError> {
        self.contents().sparse_voxels()
    }
}
