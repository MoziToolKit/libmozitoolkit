//! # Voxel World Storage
//!
//! Provides sparse 3D chunk-section world management, snapshot diffing,
//! boundary seam dirty tracking, known empty sections, and padded boundary extraction.

mod biome_meta;
mod bounds;
mod container;
mod debug;
mod manifest;
mod padded;
mod snapshot;

pub use container::*;
