#![forbid(unsafe_code)]
//! # mtk-save
//!
//! **mtk-save** provides high-performance Minecraft Java Edition save (Anvil MCA)
//! decoding and spatial ingestion for MoziToolKit.
//!
//! Key components:
//! - [`LevelData`]: `level.dat` metadata deserializer (classic and modern `spawn` layouts).
//! - [`RegionFile`]: Anvil `.mca` 1024-chunk location table parser, LZ4/Zlib/GZip/raw codecs, and `.mcc` external chunk reader.
//! - [`ChunkParser`]: Full Anvil 1.2–26.2 chunk decoder supporting pre-flattening (`Blocks`/`Data`/`Add` + Mojang `BlockStateData` upgrades), 1.13–1.17.1 `Level.Sections` (aligned & unaligned), and modern 1.18+ `sections`.
//! - [`SaveLoader`]: High-level bounding box slicer and dimension directory discovery engine.

mod block_entity;
mod chunk;
mod error;
mod legacy;
mod level;
mod loader;
mod lz4;
mod nbt;
mod plan;
mod region;
mod upgrade;

pub use error::SaveError;
pub use level::LevelData;
pub use loader::{RegionBytes, SaveLoader};
pub use plan::{PlannedExternalChunk, PlannedRegionFile, RegionSelectionPlan};
