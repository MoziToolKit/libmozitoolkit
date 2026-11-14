#![forbid(unsafe_code)]
//! Procedural extruded item models and 2D-to-3D meshing.

pub mod generator;
pub mod options;

pub use generator::{bake_item_from_png_bytes, bake_item_generated_model};
pub use options::ItemMeshOptions;
