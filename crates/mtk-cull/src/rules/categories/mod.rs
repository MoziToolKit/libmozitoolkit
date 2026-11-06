//! # Minecraft Block Categories & Parametric Shape Heuristics
//!
//! Provides catalogued name registries and parametric face occlusion resolvers
//! for standard vanilla Minecraft blocks, slabs, stairs, fluids, leaves, and partial blocks.

mod catalog;
mod meta;
mod parametric;

pub use catalog::is_non_mesh_placeholder_block;
pub use meta::{
    compute_block_cull_meta, is_inherently_submerged_block, parse_block_name_and_props,
};
