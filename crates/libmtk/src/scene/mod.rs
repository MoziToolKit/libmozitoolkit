//! # Scene Interchange Package Pipeline (`.mtkscene`)
//!
//! Provides high-level packaging, reading, and writing of lightweight MTK scene
//! interchange bundles containing sparse voxels, tree-shaken models, and compact mini-atlases.

pub mod exporter;
pub mod metadata;
pub mod package;

pub use exporter::{
    build_scene_interchange, export_scene_package, CtmMode, SceneAtlasSource, SceneExportRequest,
    SceneExportResult, SceneExporterConfig,
};
pub use metadata::{SceneMetadata, SCENE_PACKAGE_FORMAT_VERSION};
pub use package::{write_scene_package, ScenePackageReader, ScenePackageWriteOptions};
