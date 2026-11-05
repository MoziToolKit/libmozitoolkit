//! High-Speed Batch Mesh Attribute Generation for Biome Tinting and Colormaps.

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use super::colormap::VanillaColormaps;
use super::hardcoded::{
    TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS, TINT_TYPE_HARDCODED, TINT_TYPE_WATER,
};
use super::palettes::{blend_biome_colors, get_biome_palette};
use super::resolver::BiomeResolver;

/// Custom user-defined biome settings for temperature, humidity, and direct color overrides.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomBiomeSettings {
    pub temperature: f32,
    pub humidity: f32,
    pub grass_color: Option<[f32; 4]>,
    pub foliage_color: Option<[f32; 4]>,
    pub dry_foliage_color: Option<[f32; 4]>,
    pub water_color: Option<[f32; 4]>,
    pub has_custom_grass: bool,
    pub has_custom_foliage: bool,
    pub has_custom_dry_foliage: bool,
}

impl Default for CustomBiomeSettings {
    fn default() -> Self {
        Self {
            temperature: 0.8,
            humidity: 0.4,
            grass_color: None,
            foliage_color: None,
            dry_foliage_color: None,
            water_color: None,
            has_custom_grass: false,
            has_custom_foliage: false,
            has_custom_dry_foliage: false,
        }
    }
}

/// Resolved uniform biome colors used by the instant in-place recolor path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiomeColorSet {
    pub grass: [f32; 4],
    pub foliage: [f32; 4],
    pub dry_foliage: [f32; 4],
    pub water: [f32; 4],
}

/// Resolves the four tint channels and custom flags for a biome preset.
pub fn resolve_biome_color_set(
    biome_name: &str,
    multi_biomes: Option<&[(String, f32)]>,
    custom: Option<&CustomBiomeSettings>,
) -> (BiomeColorSet, bool, bool, bool) {
    resolve_biome_color_set_with_colormaps(biome_name, multi_biomes, custom, None)
}

/// Resolves the four tint channels and custom flags for a biome preset using explicit colormaps.
pub fn resolve_biome_color_set_with_colormaps(
    biome_name: &str,
    multi_biomes: Option<&[(String, f32)]>,
    custom: Option<&CustomBiomeSettings>,
    colormaps: Option<&VanillaColormaps>,
) -> (BiomeColorSet, bool, bool, bool) {
    if let Some(settings) = custom {
        let default_cms = super::colormap::get_default_colormaps();
        let cms = colormaps.unwrap_or(&default_cms);
        let default_pal = get_biome_palette("plains");
        let sampled_grass = cms.sample_grass_linear(settings.temperature, settings.humidity);
        let sampled_foliage = cms.sample_foliage_linear(settings.temperature, settings.humidity);
        let sampled_dry = cms.sample_dry_foliage_linear(settings.temperature, settings.humidity);

        return (
            BiomeColorSet {
                grass: settings.grass_color.unwrap_or(sampled_grass),
                foliage: settings.foliage_color.unwrap_or(sampled_foliage),
                dry_foliage: settings.dry_foliage_color.unwrap_or(sampled_dry),
                water: settings
                    .water_color
                    .unwrap_or_else(|| default_pal.water_linear()),
            },
            settings.has_custom_grass,
            settings.has_custom_foliage,
            settings.has_custom_dry_foliage,
        );
    }

    if let Some(mb) = multi_biomes.filter(|mb| !mb.is_empty()) {
        let weights: Vec<(&str, f32)> = mb.iter().map(|(n, w)| (n.as_str(), *w)).collect();
        return (
            BiomeColorSet {
                grass: blend_biome_colors(&weights, "grass"),
                foliage: blend_biome_colors(&weights, "foliage"),
                dry_foliage: blend_biome_colors(&weights, "dry_foliage"),
                water: blend_biome_colors(&weights, "water"),
            },
            false,
            false,
            false,
        );
    }

    let pal = get_biome_palette(biome_name);
    let default_cms = super::colormap::get_default_colormaps();
    let cms = colormaps.unwrap_or(&default_cms);
    let grass = if !pal.has_custom_grass {
        cms.sample_grass_linear(pal.temperature, pal.humidity)
    } else {
        pal.grass_linear()
    };
    let foliage = if !pal.has_custom_foliage {
        cms.sample_foliage_linear(pal.temperature, pal.humidity)
    } else {
        pal.foliage_linear()
    };
    let dry_foliage = if !pal.has_custom_dry_foliage {
        cms.sample_dry_foliage_linear(pal.temperature, pal.humidity)
    } else {
        pal.dry_foliage_linear()
    };
    (
        BiomeColorSet {
            grass,
            foliage,
            dry_foliage,
            water: pal.water_linear(),
        },
        pal.has_custom_grass,
        pal.has_custom_foliage,
        pal.has_custom_dry_foliage,
    )
}

/// Result structure containing flat aligned arrays for direct Blender mesh attribute injection.
#[derive(Debug, Clone)]
pub struct MeshBiomeAttributesResult {
    /// `mtk_biome_tint_data`: `(base_tint_weight, overlay_tint_weight, tint_weight, tint_type)`
    pub packed_tint_data: Vec<[f32; 4]>,
    /// `mtk_biome_tint_color`: `(r, g, b, a)` Linear RGBA
    pub tint_colors: Vec<[f32; 4]>,
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

    let default_cms = super::colormap::get_default_colormaps();
    let cms = colormaps.unwrap_or(&default_cms);
    let default_pal = get_biome_palette("plains");
    let sampled_grass = cms.sample_grass_linear(custom.temperature, custom.humidity);
    let sampled_foliage = cms.sample_foliage_linear(custom.temperature, custom.humidity);
    let sampled_dry = cms.sample_dry_foliage_linear(custom.temperature, custom.humidity);

    let grass_col = custom.grass_color.unwrap_or(sampled_grass);
    let foliage_col = custom.foliage_color.unwrap_or(sampled_foliage);
    let dry_foliage_col = custom.dry_foliage_color.unwrap_or(sampled_dry);
    let water_col = custom
        .water_color
        .unwrap_or_else(|| default_pal.water_linear());

    let has_cg = custom.has_custom_grass;
    let has_cf = custom.has_custom_foliage;
    let has_cdf = custom.has_custom_dry_foliage;

    let compute_face = |key: &String| -> ([f32; 4], [f32; 4]) {
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
            TINT_TYPE_GRASS => grass_col,
            TINT_TYPE_FOLIAGE => foliage_col,
            TINT_TYPE_DRY_FOLIAGE => dry_foliage_col,
            TINT_TYPE_WATER => water_col,
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

    // Resolve the global biome color channels (the resolved Linear RGBA is authoritative).
    let (grass_col, foliage_col, dry_foliage_col, water_col, has_cg, has_cf, has_cdf) =
        if let Some(mb) = multi_biomes.filter(|mb| !mb.is_empty()) {
            let weights: Vec<(&str, f32)> = mb.iter().map(|(n, w)| (n.as_str(), *w)).collect();
            (
                blend_biome_colors(&weights, "grass"),
                blend_biome_colors(&weights, "foliage"),
                blend_biome_colors(&weights, "dry_foliage"),
                blend_biome_colors(&weights, "water"),
                false,
                false,
                false,
            )
        } else {
            let pal = get_biome_palette(biome_name);
            let default_cms = super::colormap::get_default_colormaps();
            let cms = colormaps.unwrap_or(&default_cms);
            let grass = if !pal.has_custom_grass {
                cms.sample_grass_linear(pal.temperature, pal.humidity)
            } else {
                pal.grass_linear()
            };
            let foliage = if !pal.has_custom_foliage {
                cms.sample_foliage_linear(pal.temperature, pal.humidity)
            } else {
                pal.foliage_linear()
            };
            let dry_foliage = if !pal.has_custom_dry_foliage {
                cms.sample_dry_foliage_linear(pal.temperature, pal.humidity)
            } else {
                pal.dry_foliage_linear()
            };
            (
                grass,
                foliage,
                dry_foliage,
                pal.water_linear(),
                pal.has_custom_grass,
                pal.has_custom_foliage,
                pal.has_custom_dry_foliage,
            )
        };

    let compute_face = |key: &String| -> ([f32; 4], [f32; 4]) {
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
            TINT_TYPE_GRASS => grass_col,
            TINT_TYPE_FOLIAGE => foliage_col,
            TINT_TYPE_DRY_FOLIAGE => dry_foliage_col,
            TINT_TYPE_WATER => water_col,
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
        let grass_col = pal.grass_linear();
        let foliage_col = pal.foliage_linear();
        let dry_foliage_col = pal.dry_foliage_linear();
        let water_col = pal.water_linear();

        let tint_info = resolver.get_tint_info(key, None, None);
        let tw = tint_info.tint_weight;
        let base_w = tint_info.base_tint_weight;
        let overlay_w = tint_info.overlay_tint_weight;
        let tt = tint_info.tint_type;
        let is_hc = tint_info.is_hardcoded;

        let has_custom = match tt {
            TINT_TYPE_GRASS => pal.has_custom_grass,
            TINT_TYPE_FOLIAGE => pal.has_custom_foliage,
            TINT_TYPE_DRY_FOLIAGE => pal.has_custom_dry_foliage,
            _ => false,
        };

        let tint_type_val = if is_hc || has_custom {
            TINT_TYPE_HARDCODED as f32
        } else {
            tt as f32
        };

        let packed_data = [base_w, overlay_w, tw, tint_type_val];

        let final_col = match tt {
            TINT_TYPE_GRASS => grass_col,
            TINT_TYPE_FOLIAGE => foliage_col,
            TINT_TYPE_DRY_FOLIAGE => dry_foliage_col,
            TINT_TYPE_WATER => water_col,
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
