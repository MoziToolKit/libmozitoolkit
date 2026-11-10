#![forbid(unsafe_code)]
//! # mtk-texture
//!
//! Vanilla-style binary partitioning Stitcher, parallel image decoding,
//! PalettedPermutations baking, edge padding, and PBR-synced Atlas generator.

mod atlas;
mod banner;
mod error;
mod image;
mod stitcher;

pub use atlas::{
    AtlasAddressMap, AtlasBuilder, AtlasBuilderConfig, AtlasChunkMeta, AtlasSpriteLocation,
    AtlasTextureSlot, BakedAtlas, BakedAtlasChunk, DynamicAtlasChunk, DynamicAtlasManager,
    DynamicSlotInfo, IorEncoding, NormalConvention, PbrLayout, PbrMeta, PbrSourceFormat,
    RoughnessSpace, SpriteKind, DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID, DYNAMIC_ATLAS_HEIGHT,
    DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START, FALLBACK_TEXTURE_KEY,
};
pub use banner::BannerBaker;
pub use error::TextureError;
pub use image::{
    batch_analyze_transparent_faces_f32, sample_alpha_f32, DecodedSprite, RgbaBuffer, SampleMode,
};
