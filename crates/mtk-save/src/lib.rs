#![forbid(unsafe_code)]
//! # mtk-save
//!
//! **mtk-save** provides high-performance Minecraft Java Edition save (Anvil MCA)
//! decoding and spatial ingestion for MoziToolKit.
//!
//! Key components:
//! - [`LevelData`]: `level.dat` metadata deserializer.
//! - [`RegionFile`]: Anvil `.mca` 1024-chunk location table parser and direct-seek reader.
//! - [`ChunkParser`]: 1.18+ chunk section bit-unpacker for `block_states` and biomes.
//! - [`SaveLoader`]: High-level bounding box slicer and directory discovery engine.

mod block_entity;
mod chunk;
mod entity;
mod error;
mod level;
mod loader;
mod nbt;
mod region;

pub use block_entity::decode_block_entity_payload;
pub use entity::ParsedEntity;
pub use error::SaveError;
pub use level::LevelData;
pub use loader::SaveLoader;
