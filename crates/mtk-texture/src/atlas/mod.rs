mod address_map;
mod builder;
mod dynamic;
mod pbr;

pub use address_map::{
    AtlasAddressMap, AtlasChunkMeta, AtlasKeySource, AtlasSpriteLocation, AtlasTextureSlot,
    SpriteKind,
};
pub use builder::{AtlasBuilder, AtlasBuilderConfig, BakedAtlas, BakedAtlasChunk};
pub use dynamic::{
    DynamicAtlasChunk, DynamicAtlasManager, DynamicSlotInfo, DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID,
    DYNAMIC_ATLAS_HEIGHT, DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START, DYNAMIC_SLOT_SIZE,
};
pub use pbr::{IorEncoding, NormalConvention, PbrLayout, PbrMeta, PbrSourceFormat, RoughnessSpace};
