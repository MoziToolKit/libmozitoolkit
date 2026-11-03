//! # Scene Interchange Package Pipeline (`.mtkscene`)
//!
//! Provides high-level packaging, reading, and writing of lightweight MTK scene
//! interchange bundles containing sparse voxels, tree-shaken models, and compact mini-atlases.

pub mod metadata;
pub mod package;

pub use metadata::{SceneMetadata, SCENE_PACKAGE_FORMAT_VERSION};
pub use package::{write_scene_package, ScenePackageReader, ScenePackageWriteOptions};
