//! # Minecraft Block Categories & Parametric Shape Heuristics
//!
//! Provides catalogued name registries and parametric face occlusion resolvers
//! for standard vanilla Minecraft blocks, slabs, stairs, fluids, leaves, and partial blocks.

mod catalog;
mod meta;
mod parametric;

pub use meta::{compute_block_cull_meta, parse_block_name_and_props};
