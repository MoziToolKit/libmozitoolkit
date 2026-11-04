#![forbid(unsafe_code)]

//! # mtk-material
//!
//! High-performance, host-agnostic material name resolution, external alias mapping,
//! grid-based atlas unscrambling, and multi-threaded parallel UV re-addressing to libmtk AtlasAddressMaps.

pub mod biome;
pub mod error;
pub mod pbr;
pub mod remap;
pub mod resolver;
pub mod types;

pub use pbr::{
    decode_baked_atlas, decode_ior, decode_labpbr13, DecodedPbr, IorEncoding, NormalConvention,
    PbrDecodeSummary, PbrLayout, PbrMeta, PbrSourceFormat, RoughnessSpace,
};

pub use biome::{
    apply_dark_forest_grass_modifier, blend_biome_colors, classify_tint_category,
    clear_cached_colormaps, compute_mesh_biome_attributes, compute_mesh_biome_attributes_custom,
    compute_mesh_biome_attributes_paletted_flat, compute_mesh_biome_attributes_per_face,
    compute_resolved_tint_color, default_biome_palette, get_biome_palette, get_colormap_uv,
    get_default_colormaps, get_hardcoded_tint, get_hardcoded_tint_hex, get_swamp_grass_color,
    get_unit_cube_tint_index, get_unit_cube_tint_indices, hex_to_linear_rgba, hex_to_srgb,
    linear_to_srgb, reapply_uniform_biome_tint, reapply_uniform_biome_tint_flat,
    resolve_biome_color_set, sample_colormap_hex, sample_colormap_pixel, sample_colormap_pixel_u8,
    sample_colormap_pixel_uv, sample_colormap_pixel_uv_u8, set_cached_colormaps, srgb_to_linear,
    BiomeColorSet, BiomePalette, BiomeResolver, BiomeTintFlatBuffers, CustomBiomeSettings,
    MeshBiomeAttributesResult, TintInfo, VanillaColormaps, BLOCK_TINT_REGISTRY, CANONICAL_BIOMES,
    HARDCODED_BLOCK_TINTS, TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS,
    TINT_TYPE_HARDCODED, TINT_TYPE_NONE, TINT_TYPE_WATER,
};
pub use error::MaterialError;
pub use remap::batch::{
    remap_mesh_multi_uvs_paletted_with_options, remap_mesh_multi_uvs_parallel_with_options,
};
pub use remap::routes::{reroute_mesh_animation, resolve_mesh_material_routes};
pub use remap::{
    is_quad_uv_diamond, remap_atlas_to_local, remap_local_to_atlas, remap_sprite_to_sprite,
    straighten_diamond_quad_uv,
};
pub use resolver::{
    builtin_biome_suffixes, clean_identifier, decode_grid_atlas_uv, extract_animation_metrics,
    remap_grid_atlas_uv_to_local, resolve_animated_sprite, resolve_routed_sprite, MaterialResolver,
};
pub use types::{
    GridAtlasSpec, MeshAnimationRerouteResult, MeshMaterialRoute, MeshMultiUvRemapResult,
    MeshPalettedUvRemapResult,
};
