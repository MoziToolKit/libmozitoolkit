//! # Scene Interchange Package Pipeline (`.mtkscene`)
//!
//! Provides high-level packaging, reading, and writing of lightweight MTK scene
//! interchange bundles containing sparse voxels, tree-shaken models, and compact mini-atlases.

mod exporter;
mod metadata;
mod package;

pub use exporter::{
    export_scene_package, CtmMode, SceneAtlasSource, SceneExportRequest, SceneExportResult,
    SceneExporterConfig,
};
pub use metadata::{SceneCapabilities, SceneMetadata};
pub use package::{SceneBundle, ScenePackageReader, ValidationReport};
