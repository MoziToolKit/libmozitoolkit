//! # Scene Interchange Package Pipeline (`.mtkscene`)
//!
//! Provides high-level packaging, reading, and writing of lightweight MTK scene
//! interchange bundles containing sparse voxels, tree-shaken models, and compact mini-atlases.

pub mod exporter;
pub mod metadata;
pub mod package;

pub use exporter::{export_scene_package, CtmMode, SceneExportRequest, SceneExporterConfig};
pub use metadata::{SceneCapabilities, SceneMetadata, SCENE_PACKAGE_FORMAT_VERSION};
pub use package::{SceneBundle, ScenePackageReader, ValidationReport};
