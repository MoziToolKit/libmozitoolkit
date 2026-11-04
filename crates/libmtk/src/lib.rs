#![forbid(unsafe_code)]
//! # libmtk
//!
//! **Application layer** of the libmozitoolkit core engine. This crate owns the
//! headless asset precompilation pipeline:
//!
//! - [`prebake`]: end-to-end headless asset precompilation into a single `.mtkcache` container
//!   (atlas stitching, biome resolver extraction, model database baking, manifest).
//!
//! Domain primitives (`MeshData`, `BakedModel`, `FaceCuller`, `SectionMesher`, ...) live in the
//! individual `mtk-*` crates. This crate deliberately does **not** re-export them: bindings and
//! tools depend on the domain crate matching the layer they use, so the dependency graph stays
//! honest. See `docs/ARCHITECTURE.md` for the layering contract.

use thiserror::Error;

pub mod prebake;
pub mod scene;

pub use prebake::{
    fingerprint_str_to_bytes16, prebake_all_models, prebake_all_models_with_progress,
    precompile_all_assets, precompile_all_assets_with_progress, resolve_active_cache_package,
    AssetCacheReader, CacheManifest, PrecompileConfig, PrecompileResult,
    ASSET_CACHE_FORMAT_VERSION,
};
pub use scene::{
    build_scene_interchange, export_scene_package, write_scene_package, AtlasLayerSource,
    BiomeAssetSource, CtmAssetSource, CtmMode, EntityAssetSource, SceneAtlasSource,
    SceneCapabilities, SceneExportRequest, SceneExportResult, SceneExporterConfig, SceneMetadata,
    ScenePackageReader, ScenePackageWriteOptions, SceneSourceCapabilities, SceneWriteRequest,
    ValidationReport, SCENE_PACKAGE_FORMAT_VERSION,
};

/// Unified error type for all `libmtk` application-layer operations.
///
/// The Python/C bindings do not consume this type directly — each binding maps the domain error
/// to its own host-native exception at the boundary.
#[derive(Debug, Error)]
pub enum MtkError {
    #[error("Model error: {0}")]
    Model(#[from] mtk_model::ModelError),

    #[error("Voxel error: {0}")]
    Voxel(#[from] mtk_voxel::VoxelError),

    #[error("Texture error: {0}")]
    Texture(#[from] mtk_texture::error::TextureError),

    #[error("Resource error: {0}")]
    Resource(#[from] mtk_resource::error::ResourceError),

    #[error("Material error: {0}")]
    Material(#[from] mtk_material::MaterialError),

    #[cfg(feature = "save")]
    #[error("Save error: {0}")]
    Save(#[from] mtk_save::SaveError),

    #[error("Package container error: {0}")]
    Package(#[from] mtk_package::PackageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Postcard error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("Invalid package profile for scene: expected SceneInterchange, found {0:?}")]
    InvalidSceneProfile(mtk_package::PackageProfile),

    #[error("Unsupported scene package format version: expected 1, found {0}")]
    UnsupportedSceneFormatVersion(u32),

    #[error("Thread pool error: {0}")]
    ThreadPool(String),
}
