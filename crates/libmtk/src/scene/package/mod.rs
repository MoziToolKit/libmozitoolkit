//! Unified package reader, writer, and validator for `.mtkscene` scene interchange files.

pub mod reader;
pub mod validator;
pub mod writer;

#[cfg(test)]
mod tests;

pub use reader::{SceneBundle, ScenePackageReader};
pub use validator::ValidationReport;
pub use writer::{write_scene_package, ScenePackageWriteOptions, SceneWriteRequest};
