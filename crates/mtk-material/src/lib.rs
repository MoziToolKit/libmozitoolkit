#![forbid(unsafe_code)]

//! # mtk-material
//!
//! High-performance, host-agnostic material name resolution, external alias mapping,
//! grid-based atlas unscrambling, and multi-threaded parallel UV re-addressing to libmtk AtlasAddressMaps.

mod biome;
mod error;
mod pbr;
mod remap;
mod resolver;
mod types;

pub use pbr::decode_baked_atlas;

pub use biome::{
    compute_mesh_biome_attributes_paletted_flat, default_biome_palette, get_biome_palette,
    get_colormap_uv, get_default_colormaps, get_swamp_grass_color, get_unit_cube_tint_indices,
    hex_to_linear_rgba, reapply_uniform_biome_tint_flat, resolve_biome_color_set_with_colormaps,
    BiomeColorSet, BiomePalette, BiomeResolver, CustomBiomeSettings, VanillaColormaps,
    CANONICAL_BIOMES, TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS,
    TINT_TYPE_HARDCODED, TINT_TYPE_NONE, TINT_TYPE_WATER,
};
pub use error::MaterialError;
pub use remap::{
    remap_mesh_multi_uvs_paletted_with_options, reroute_mesh_animation,
    resolve_mesh_material_routes,
};
pub use resolver::{
    builtin_biome_suffixes, clean_identifier, extract_animation_metrics, parse_overrides,
    resolve_animated_sprite, MaterialResolver, ResolverOverrides,
};
pub use types::GridAtlasSpec;
