#![forbid(unsafe_code)]
//! # mtk-resource
//!
//! Minecraft resource pack virtual file system, asset identifier specification,
//! `.png.mcmeta` animation metadata parser, and vanilla `atlases/*.json` data models.

pub mod atlas;
pub mod banner;
pub mod ctm;
pub mod error;
pub mod identifier;
pub mod meta;
pub mod pack;
pub mod skin;

pub use atlas::{AtlasCategory, AtlasDefinition, AtlasFilterPattern, AtlasSource, UnstitchRegion};
pub use banner::{
    dye_color_to_rgb, dye_id_to_name, dye_rgb, extract_banner_color_from_state, name_to_dye_id,
    normalize_banner_pattern, DYE_NAMES, DYE_RGB, VANILLA_BANNER_DYES,
};
pub use ctm::{
    coordinate_random, extract_block_name, get_face_tangents, BlockMatch, ConnectLogic, CtmMethod,
    CtmRule, CtmSolver, CtmSymmetry, CTM_47_LOOKUP, OVERLAY_17_LOOKUP,
};
pub use error::ResourceError;
pub use identifier::{ResourceLocation, DEFAULT_NAMESPACE};
pub use meta::{AnimationFrame, AnimationMetadata, TextureMetadata};
#[cfg(feature = "zip")]
pub use pack::ZipPack;
pub use pack::{
    is_companion_asset_path, DirectoryPack, DiscoveredSprite, MemoryPack, PbrCompanions,
    ResourcePack, ResourcePackStack,
};
