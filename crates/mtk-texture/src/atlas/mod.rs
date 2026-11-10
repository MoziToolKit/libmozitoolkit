mod address_map;
mod builder;
mod dynamic;
mod fallback;
mod pbr;

pub(crate) use address_map::AtlasKeySource;
pub use address_map::{
    AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, AtlasTextureSlot, SpriteKind,
};
pub use builder::{AtlasBuilder, AtlasBuilderConfig, BakedAtlas, BakedAtlasChunk};
pub use dynamic::{
    DynamicAtlasChunk, DynamicAtlasManager, DynamicSlotInfo, DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID,
    DYNAMIC_ATLAS_HEIGHT, DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START,
};
pub(crate) use fallback::create_fallback_sprite;
pub use fallback::FALLBACK_TEXTURE_KEY;
pub use pbr::{IorEncoding, NormalConvention, PbrLayout, PbrMeta, PbrSourceFormat, RoughnessSpace};
