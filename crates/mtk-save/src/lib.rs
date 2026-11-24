#![forbid(unsafe_code)]
//! # mtk-save
//!
//! **mtk-save** provides high-performance Minecraft Java Edition save (Anvil MCA)
//! decoding and spatial ingestion for MoziToolKit.
//!
//! Key components:
//! - [`LevelData`]: `level.dat` metadata deserializer (classic and modern `spawn` layouts).
//! - [`RegionFile`]: Anvil `.mca` 1024-chunk location table parser, LZ4/Zlib/GZip/raw codecs, and `.mcc` external chunk reader.
//! - [`ChunkParser`]: 1.13–26.2 (`DataVersion >= 1451`) chunk section bit-unpacker (aligned and unaligned), legacy numeric/string biome decoder, and DataVersion upgrade pipeline.
//! - [`SaveLoader`]: High-level bounding box slicer and dimension directory discovery engine.

mod block_entity;
mod chunk;
mod error;
mod level;
mod loader;
mod lz4;
mod nbt;
mod region;
mod upgrade;

pub use error::SaveError;
pub use level::LevelData;
pub use loader::SaveLoader;
