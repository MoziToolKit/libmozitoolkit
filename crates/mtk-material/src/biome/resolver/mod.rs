//! # Biome Resolver and Model Tint Scanning
//!
//! Provides resource pack block model parsing, `tintindex` discovery,
//! and authoritative SSOT vanilla biome mapping metadata.

mod defaults;
mod lookup;
mod scanner;
mod types;

pub use types::*;
