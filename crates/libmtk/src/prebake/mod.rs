//! # Headless Asset Prebaking and Cache Engine
//!
//! Performs unified end-to-end asset precompilation (Atlas stitching, Standalone PBR alignment,
//! and multi-threaded Model baking) directly into `.mtkcache` binary package files.

mod engine;
mod models;
mod reader;
mod resolver;

pub use engine::{
    precompile_all_assets, precompile_all_assets_with_progress, PrecompileConfig, PrecompileResult,
};
pub use models::prebake_all_models;
pub use reader::{
    fingerprint_str_to_bytes16, AssetCacheReader, CacheManifest, ASSET_CACHE_FORMAT_VERSION,
};

pub use resolver::{evaluate_cache_status, resolve_active_cache_package};
