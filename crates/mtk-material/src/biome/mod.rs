//! Minecraft Biome Palettes, Model Tintindex Resolution, and Mesh Attribute Generation.

pub mod batch;
pub mod canonical_data;
pub mod colormap;
pub mod hardcoded;
pub mod palettes;
pub mod resolver;

pub use colormap::{
    apply_dark_forest_grass_modifier, clear_cached_colormaps, compute_resolved_tint_color,
    get_default_colormaps, set_cached_colormaps, VanillaColormaps,
};

pub use batch::{
    compute_mesh_biome_attributes, compute_mesh_biome_attributes_custom,
    compute_mesh_biome_attributes_paletted_flat, compute_mesh_biome_attributes_per_face,
    reapply_uniform_biome_tint, reapply_uniform_biome_tint_flat, resolve_biome_color_set,
    BiomeColorSet, BiomeTintFlatBuffers, CustomBiomeSettings, MeshBiomeAttributesResult,
};
pub use hardcoded::{
    classify_tint_category, get_hardcoded_tint, get_hardcoded_tint_hex, get_swamp_grass_color,
    get_unit_cube_tint_index, get_unit_cube_tint_indices, BLOCK_TINT_REGISTRY,
    HARDCODED_BLOCK_TINTS, TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS,
    TINT_TYPE_HARDCODED, TINT_TYPE_NONE, TINT_TYPE_WATER,
};
pub use palettes::{
    blend_biome_colors, default_biome_palette, get_biome_palette, get_colormap_uv,
    hex_to_linear_rgba, hex_to_srgb, linear_to_srgb, sample_colormap_hex, sample_colormap_pixel,
    sample_colormap_pixel_u8, sample_colormap_pixel_uv, sample_colormap_pixel_uv_u8,
    srgb_to_linear, BiomePalette, CANONICAL_BIOMES,
};
pub use resolver::{BiomeResolver, TintInfo};
