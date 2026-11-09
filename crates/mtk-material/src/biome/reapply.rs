//! Flat buffer operations and uniform biome tint re-application.

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use super::batch::MeshBiomeAttributesResult;
use super::color_set::BiomeColorSet;
use super::hardcoded::{
    TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS, TINT_TYPE_HARDCODED, TINT_TYPE_WATER,
};
use super::resolver::BiomeResolver;

/// Flat float buffers produced by the instant uniform-biome recolor path.
#[derive(Debug, Clone, Default)]
pub struct BiomeTintFlatBuffers {
    /// `mtk_biome_tint_data` as flat RGBA floats.
    pub packed_tint_data: Vec<f32>,
    /// `mtk_biome_tint_color` as flat RGBA floats.
    pub tint_colors: Vec<f32>,
}

/// Re-apply a uniform biome palette to mesh faces without re-resolving texture keys.
///
/// `base_tint_types` holds the intrinsic category per face (0=None, 1=Grass,
/// 2=Foliage, 3=Water, 4=Hardcoded, 5=DryFoliage). `packed_tint_data` supplies the
/// existing weights; only its fourth channel (tint type) is rewritten.
/// `existing_tint_colors` preserves hardcoded colors (e.g. redstone, dyed leaves)
/// while custom-grass/foliage overrides intentionally replace their category color.
pub fn reapply_uniform_biome_tint_flat(
    base_tint_types: &[u8],
    packed_tint_data: &[[f32; 4]],
    existing_tint_colors: &[[f32; 4]],
    colors: BiomeColorSet,
    has_custom_grass: bool,
    has_custom_foliage: bool,
    has_custom_dry_foliage: bool,
) -> BiomeTintFlatBuffers {
    let face_count = base_tint_types.len();
    if face_count == 0 {
        return BiomeTintFlatBuffers::default();
    }

    let mut packed_out = vec![0.0f32; face_count * 4];
    let mut colors_out = vec![0.0f32; face_count * 4];

    let write_face = |i: usize, packed: &mut [f32], color: &mut [f32]| {
        let base = base_tint_types[i];
        let target_tt = match base {
            TINT_TYPE_GRASS if has_custom_grass => TINT_TYPE_HARDCODED,
            TINT_TYPE_FOLIAGE if has_custom_foliage => TINT_TYPE_HARDCODED,
            TINT_TYPE_DRY_FOLIAGE if has_custom_dry_foliage => TINT_TYPE_HARDCODED,
            other => other,
        };

        let src = packed_tint_data.get(i).copied().unwrap_or([0.0; 4]);
        packed[0] = src[0];
        packed[1] = src[1];
        packed[2] = src[2];
        packed[3] = target_tt as f32;

        let c = match base {
            TINT_TYPE_GRASS => colors.grass,
            TINT_TYPE_FOLIAGE => colors.foliage,
            TINT_TYPE_DRY_FOLIAGE => colors.dry_foliage,
            TINT_TYPE_WATER => colors.water,
            TINT_TYPE_HARDCODED => existing_tint_colors
                .get(i)
                .copied()
                .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            _ => [1.0, 1.0, 1.0, 1.0],
        };
        color.copy_from_slice(&c);
    };

    #[cfg(feature = "parallel")]
    {
        packed_out
            .par_chunks_mut(4)
            .zip(colors_out.par_chunks_mut(4))
            .enumerate()
            .for_each(|(i, (packed, color))| write_face(i, packed, color));
    }

    #[cfg(not(feature = "parallel"))]
    {
        for i in 0..face_count {
            write_face(
                i,
                &mut packed_out[i * 4..i * 4 + 4],
                &mut colors_out[i * 4..i * 4 + 4],
            );
        }
    }

    BiomeTintFlatBuffers {
        packed_tint_data: packed_out,
        tint_colors: colors_out,
    }
}

/// Array-of-structs variant of [`reapply_uniform_biome_tint_flat`].
pub fn reapply_uniform_biome_tint(
    base_tint_types: &[u8],
    packed_tint_data: &[[f32; 4]],
    existing_tint_colors: &[[f32; 4]],
    colors: BiomeColorSet,
    has_custom_grass: bool,
    has_custom_foliage: bool,
    has_custom_dry_foliage: bool,
) -> MeshBiomeAttributesResult {
    let flat = reapply_uniform_biome_tint_flat(
        base_tint_types,
        packed_tint_data,
        existing_tint_colors,
        colors,
        has_custom_grass,
        has_custom_foliage,
        has_custom_dry_foliage,
    );
    let face_count = base_tint_types.len();
    let mut packed_tint_data = Vec::with_capacity(face_count);
    let mut tint_colors = Vec::with_capacity(face_count);
    for i in 0..face_count {
        packed_tint_data.push([
            flat.packed_tint_data[i * 4],
            flat.packed_tint_data[i * 4 + 1],
            flat.packed_tint_data[i * 4 + 2],
            flat.packed_tint_data[i * 4 + 3],
        ]);
        tint_colors.push([
            flat.tint_colors[i * 4],
            flat.tint_colors[i * 4 + 1],
            flat.tint_colors[i * 4 + 2],
            flat.tint_colors[i * 4 + 3],
        ]);
    }
    MeshBiomeAttributesResult {
        packed_tint_data,
        tint_colors,
    }
}

/// Palette-native full biome attribute computation.
///
/// Resolves tint metadata once per unique palette entry, then writes flat
/// `packed_tint_data` / `tint_colors` for every face. This is
/// the palette counterpart of `compute_mesh_biome_attributes_custom` and avoids
/// materializing one `String` per face.
pub fn compute_mesh_biome_attributes_paletted_flat(
    palette: &[String],
    face_indices: &[u32],
    colors: BiomeColorSet,
    has_custom_grass: bool,
    has_custom_foliage: bool,
    has_custom_dry_foliage: bool,
    resolver: &BiomeResolver,
) -> BiomeTintFlatBuffers {
    let face_count = face_indices.len();
    if face_count == 0 {
        return BiomeTintFlatBuffers::default();
    }

    let palette_infos: Vec<_> = palette
        .iter()
        .map(|key| resolver.get_tint_info(key, None, None))
        .collect();
    let mut packed_out = vec![0.0f32; face_count * 4];
    let mut colors_out = vec![0.0f32; face_count * 4];

    let compute = |i: usize| -> ([f32; 4], [f32; 4]) {
        let info = face_indices
            .get(i)
            .and_then(|idx| palette_infos.get(*idx as usize))
            .cloned()
            .unwrap_or_default();

        let has_custom = match info.tint_type {
            TINT_TYPE_GRASS => has_custom_grass,
            TINT_TYPE_FOLIAGE => has_custom_foliage,
            TINT_TYPE_DRY_FOLIAGE => has_custom_dry_foliage,
            _ => false,
        };
        let target_tt = if info.is_hardcoded || has_custom {
            TINT_TYPE_HARDCODED
        } else {
            info.tint_type
        };
        let packed = [
            info.base_tint_weight,
            info.overlay_tint_weight,
            info.tint_weight,
            target_tt as f32,
        ];
        let color = match info.tint_type {
            TINT_TYPE_GRASS => colors.grass,
            TINT_TYPE_FOLIAGE => colors.foliage,
            TINT_TYPE_DRY_FOLIAGE => colors.dry_foliage,
            TINT_TYPE_WATER => colors.water,
            TINT_TYPE_HARDCODED => info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0]),
            _ => {
                if info.is_hardcoded {
                    info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0])
                } else {
                    [1.0, 1.0, 1.0, 1.0]
                }
            }
        };
        (packed, color)
    };

    #[cfg(feature = "parallel")]
    let results: Vec<([f32; 4], [f32; 4])> = (0..face_count).into_par_iter().map(compute).collect();

    #[cfg(not(feature = "parallel"))]
    let results: Vec<([f32; 4], [f32; 4])> = (0..face_count).map(compute).collect();

    for (i, (packed, color)) in results.into_iter().enumerate() {
        packed_out[i * 4..i * 4 + 4].copy_from_slice(&packed);
        colors_out[i * 4..i * 4 + 4].copy_from_slice(&color);
    }

    BiomeTintFlatBuffers {
        packed_tint_data: packed_out,
        tint_colors: colors_out,
    }
}
