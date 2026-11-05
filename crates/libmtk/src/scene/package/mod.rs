//! Unified package reader, writer, and validator for `.mtkscene` scene interchange files.

mod reader;
mod validator;
mod writer;

#[cfg(test)]
mod tests;

pub use reader::{SceneBundle, ScenePackageReader};
pub use validator::ValidationReport;
pub(crate) use writer::{write_scene_package, SceneWriteRequest};
