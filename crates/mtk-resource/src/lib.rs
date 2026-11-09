#![forbid(unsafe_code)]
//! # mtk-resource
//!
//! Minecraft resource pack virtual file system, asset identifier specification,
//! `.png.mcmeta` animation metadata parser, and vanilla `atlases/*.json` data models.

mod atlas;
pub mod banner;
mod ctm;
mod error;
mod identifier;
mod meta;
mod pack;
mod probe;
pub mod skin;
mod texture_key;

pub use atlas::{AtlasCategory, AtlasDefinition, AtlasFilterPattern, AtlasSource, UnstitchRegion};
pub use ctm::{
    extract_block_name, BlockMatch, ConnectLogic, CtmMethod, CtmRule, CtmSolver, CtmSymmetry,
};
pub use error::ResourceError;
pub use identifier::ResourceLocation;
pub use meta::{AnimationFrame, AnimationMetadata};
#[cfg(feature = "zip")]
pub use pack::ZipPack;
pub use pack::{
    DirectoryPack, DiscoveredSprite, MemoryPack, PbrCompanions, ResourcePack, ResourcePackStack,
};
pub use probe::{path_provides_vanilla_client, probe_pack, AssetSummary, PackProbe};
pub use texture_key::TextureKeyVariants;
