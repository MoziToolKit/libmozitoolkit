//! Minecraft Biome Palettes, Model Tintindex Resolution, and Mesh Attribute Generation.

mod batch;
mod canonical_data;
mod colormap;
mod hardcoded;
mod palettes;
mod resolver;

pub use colormap::{get_default_colormaps, VanillaColormaps};

pub use batch::{
    compute_mesh_biome_attributes_custom_with_colormaps,
    compute_mesh_biome_attributes_paletted_flat, compute_mesh_biome_attributes_per_face,
    compute_mesh_biome_attributes_with_colormaps, reapply_uniform_biome_tint,
    reapply_uniform_biome_tint_flat, resolve_biome_color_set_with_colormaps, BiomeColorSet,
    BiomeTintFlatBuffers, CustomBiomeSettings, MeshBiomeAttributesResult,
};
pub use hardcoded::{
    get_swamp_grass_color, get_unit_cube_tint_indices, TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE,
    TINT_TYPE_GRASS, TINT_TYPE_HARDCODED, TINT_TYPE_NONE, TINT_TYPE_WATER,
};
pub use palettes::{
    default_biome_palette, get_biome_palette, get_colormap_uv, hex_to_linear_rgba, BiomePalette,
    CANONICAL_BIOMES,
};
pub use resolver::{BiomeResolver, TintInfo};
