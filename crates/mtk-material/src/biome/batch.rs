//! High-Speed Batch Mesh Attribute Generation for Biome Tinting and Colormaps.

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use super::color_set::{
    resolve_biome_color_set_with_colormaps, BiomeColorSet, CustomBiomeSettings,
};
use super::colormap::VanillaColormaps;
use super::hardcoded::{
    TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS, TINT_TYPE_HARDCODED, TINT_TYPE_WATER,
};
use super::palettes::get_biome_palette;
use super::resolver::BiomeResolver;

/// Result structure containing flat aligned arrays for direct Blender mesh attribute injection.
#[derive(Debug, Clone)]
pub struct MeshBiomeAttributesResult {
    /// `mtk_biome_tint_data`: `(base_tint_weight, overlay_tint_weight, tint_weight, tint_type)`
    pub packed_tint_data: Vec<[f32; 4]>,
    /// `mtk_biome_tint_color`: `(r, g, b, a)` Linear RGBA
    pub tint_colors: Vec<[f32; 4]>,
}

/// Computes the packed tint data and final tint color for a single texture key.
#[inline]
pub(crate) fn compute_tint_for_key(
    key: &str,
    resolver: &BiomeResolver,
    colors: &BiomeColorSet,
    (has_cg, has_cf, has_cdf): (bool, bool, bool),
) -> ([f32; 4], [f32; 4]) {
    let tint_info = resolver.get_tint_info(key, None, None);
    let tw = tint_info.tint_weight;
    let base_w = tint_info.base_tint_weight;
    let overlay_w = tint_info.overlay_tint_weight;
    let tt = tint_info.tint_type;
    let is_hc = tint_info.is_hardcoded;

    let has_custom = match tt {
        TINT_TYPE_GRASS => has_cg,
        TINT_TYPE_FOLIAGE => has_cf,
        TINT_TYPE_DRY_FOLIAGE => has_cdf,
        _ => false,
    };

    let tint_type_val = if is_hc || has_custom {
        TINT_TYPE_HARDCODED as f32
    } else {
        tt as f32
    };

    let packed_data = [base_w, overlay_w, tw, tint_type_val];

    let final_col = match tt {
        TINT_TYPE_GRASS => colors.grass,
        TINT_TYPE_FOLIAGE => colors.foliage,
        TINT_TYPE_DRY_FOLIAGE => colors.dry_foliage,
        TINT_TYPE_WATER => colors.water,
        TINT_TYPE_HARDCODED => tint_info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0]),
        _ => {
            if is_hc {
                tint_info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0])
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }
    };

    (packed_data, final_col)
}

/// Compute biome tinting attributes in parallel across all mesh faces with custom biome settings and explicit colormaps.
pub fn compute_mesh_biome_attributes_custom_with_colormaps(
    face_texture_keys: &[String],
    custom: &CustomBiomeSettings,
    resolver: &BiomeResolver,
    colormaps: Option<&VanillaColormaps>,
) -> MeshBiomeAttributesResult {
    let face_count = face_texture_keys.len();
    if face_count == 0 {
        return MeshBiomeAttributesResult {
            packed_tint_data: Vec::new(),
            tint_colors: Vec::new(),
        };
    }

    let (colors, has_cg, has_cf, has_cdf) =
        resolve_biome_color_set_with_colormaps("plains", None, Some(custom), colormaps);

    let compute_face = |key: &String| -> ([f32; 4], [f32; 4]) {
        compute_tint_for_key(key, resolver, &colors, (has_cg, has_cf, has_cdf))
    };

    #[cfg(feature = "parallel")]
    let results: Vec<([f32; 4], [f32; 4])> =
        face_texture_keys.par_iter().map(compute_face).collect();

    #[cfg(not(feature = "parallel"))]
    let results: Vec<([f32; 4], [f32; 4])> = face_texture_keys.iter().map(compute_face).collect();

    let mut packed_tint_data = Vec::with_capacity(face_count);
    let mut tint_colors = Vec::with_capacity(face_count);

    for (p, c) in results {
        packed_tint_data.push(p);
        tint_colors.push(c);
    }

    MeshBiomeAttributesResult {
        packed_tint_data,
        tint_colors,
    }
}

/// Compute biome tinting attributes in parallel across all mesh faces using explicit colormaps.
pub fn compute_mesh_biome_attributes_with_colormaps(
    face_texture_keys: &[String],
    biome_name: &str,
    multi_biomes: Option<&[(String, f32)]>,
    resolver: &BiomeResolver,
    colormaps: Option<&VanillaColormaps>,
) -> MeshBiomeAttributesResult {
    let face_count = face_texture_keys.len();
    if face_count == 0 {
        return MeshBiomeAttributesResult {
            packed_tint_data: Vec::new(),
            tint_colors: Vec::new(),
        };
    }

    let (colors, has_cg, has_cf, has_cdf) =
        resolve_biome_color_set_with_colormaps(biome_name, multi_biomes, None, colormaps);

    let compute_face = |key: &String| -> ([f32; 4], [f32; 4]) {
        compute_tint_for_key(key, resolver, &colors, (has_cg, has_cf, has_cdf))
    };

    #[cfg(feature = "parallel")]
    let results: Vec<([f32; 4], [f32; 4])> =
        face_texture_keys.par_iter().map(compute_face).collect();

    #[cfg(not(feature = "parallel"))]
    let results: Vec<([f32; 4], [f32; 4])> = face_texture_keys.iter().map(compute_face).collect();

    let mut packed_tint_data = Vec::with_capacity(face_count);
    let mut tint_colors = Vec::with_capacity(face_count);

    for (p, c) in results {
        packed_tint_data.push(p);
        tint_colors.push(c);
    }

    MeshBiomeAttributesResult {
        packed_tint_data,
        tint_colors,
    }
}

/// Compute biome tinting attributes in parallel across mesh faces with individual per-face biome names.
pub fn compute_mesh_biome_attributes_per_face(
    face_texture_keys: &[String],
    face_biomes: &[String],
    resolver: &BiomeResolver,
) -> MeshBiomeAttributesResult {
    let face_count = face_texture_keys.len();
    if face_count == 0 {
        return MeshBiomeAttributesResult {
            packed_tint_data: Vec::new(),
            tint_colors: Vec::new(),
        };
    }

    let default_biome = "plains".to_string();

    let compute_face = |(key, b_name): (&String, &String)| -> ([f32; 4], [f32; 4]) {
        let pal = get_biome_palette(b_name);
        let colors = BiomeColorSet {
            grass: pal.grass_linear(),
            foliage: pal.foliage_linear(),
            dry_foliage: pal.dry_foliage_linear(),
            water: pal.water_linear(),
        };
        compute_tint_for_key(
            key,
            resolver,
            &colors,
            (
                pal.has_custom_grass,
                pal.has_custom_foliage,
                pal.has_custom_dry_foliage,
            ),
        )
    };

    let pairs: Vec<(&String, &String)> = face_texture_keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k, face_biomes.get(i).unwrap_or(&default_biome)))
        .collect();

    #[cfg(feature = "parallel")]
    let results: Vec<([f32; 4], [f32; 4])> = pairs.into_par_iter().map(compute_face).collect();

    #[cfg(not(feature = "parallel"))]
    let results: Vec<([f32; 4], [f32; 4])> = pairs.into_iter().map(compute_face).collect();

    let mut packed_tint_data = Vec::with_capacity(face_count);
    let mut tint_colors = Vec::with_capacity(face_count);

    for (p, c) in results {
        packed_tint_data.push(p);
        tint_colors.push(c);
    }

    MeshBiomeAttributesResult {
        packed_tint_data,
        tint_colors,
    }
}
