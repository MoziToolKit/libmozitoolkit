//! Dynamic Atlas Chunk and Manager for runtime-registered entities.

mod chunk;
mod manager;

pub use chunk::{
    DynamicAtlasChunk, DynamicSlotInfo, DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID, DYNAMIC_ATLAS_HEIGHT,
    DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START, DYNAMIC_SLOT_SIZE,
};
pub use manager::DynamicAtlasManager;

#[cfg(test)]
mod tests;
