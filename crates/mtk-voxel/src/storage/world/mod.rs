//! # Voxel World Storage
//!
//! Provides sparse 3D chunk-section world management, snapshot diffing,
//! boundary seam dirty tracking, CRC caching, and padded boundary extraction.

mod biome_meta;
mod container;
mod debug;
mod manifest;
mod padded;
mod snapshot;

pub use container::*;
