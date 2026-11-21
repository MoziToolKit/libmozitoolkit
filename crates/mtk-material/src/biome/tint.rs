//! Biome tint calculation for individual faces.
//!
//! Computes per-face Biome Tint data `[base_weight, overlay_weight, tint_weight, tint_type]`
//! and resolved linear colors `[r, g, b, a]`.

use std::sync::LazyLock;

use super::colormap::VanillaColormaps;
use super::hardcoded::*;
use super::palettes::default_biome_palette;
use super::resolver::BiomeResolver;

static DEFAULT_BIOME_RESOLVER: LazyLock<BiomeResolver> = LazyLock::new(BiomeResolver::default);

/// Computes biome tint data and linear color for a face.
pub fn compute_face_tint(
    texture_key: &str,
    block_name: Option<&str>,
    tint_index: i16,
    biome_resolver: Option<&BiomeResolver>,
    colormaps: Option<&VanillaColormaps>,
) -> ([f32; 4], [f32; 4]) {
    // In Minecraft rendering, any face with negative tint index is strictly untinted,
    // UNLESS it has an overlay companion whose overlay part must be tinted (such as grass_block_side).
    let is_grass_side =
        texture_key.ends_with("grass_block_side") || texture_key.ends_with("grass_side");
    if tint_index < 0 && !is_grass_side {
        return ([1.0, 0.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0]);
    }

    let resolver = biome_resolver.unwrap_or(&DEFAULT_BIOME_RESOLVER);

    // Check if texture has an overlay companion (e.g. grass_block_side has grass_block_side_overlay)
    let has_overlay = resolver.get_overlay_texture(texture_key).is_some();

    if tint_index < 0 && !(has_overlay && is_grass_side) {
        return ([1.0, 0.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0]);
    }

    // Hot path: the default palette is a static entry, no lookup/allocation needed.
    let default_pal = default_biome_palette();
    let t_idx = Some(tint_index as i32);

    let info = resolver.get_tint_info(texture_key, block_name, t_idx);
    let tw = info.tint_weight;
    let bw = info.base_tint_weight;
    let ow = info.overlay_tint_weight;
    let tt = info.tint_type;
    let is_hc = info.is_hardcoded;

    let packed_data = [bw, ow, tw, tt as f32];
    let final_col = match tt {
        TINT_TYPE_GRASS => {
            if let Some(cms) = colormaps {
                cms.sample_grass_linear(default_pal.temperature, default_pal.humidity)
            } else {
                default_pal.grass_linear()
            }
        }
        TINT_TYPE_FOLIAGE => {
            if let Some(cms) = colormaps {
                cms.sample_foliage_linear(default_pal.temperature, default_pal.humidity)
            } else {
                default_pal.foliage_linear()
            }
        }
        TINT_TYPE_DRY_FOLIAGE => {
            if let Some(cms) = colormaps {
                cms.sample_dry_foliage_linear(default_pal.temperature, default_pal.humidity)
            } else {
                default_pal.dry_foliage_linear()
            }
        }
        TINT_TYPE_WATER => default_pal.water_linear(),
        TINT_TYPE_HARDCODED => info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0]),
        _ => {
            if is_hc {
                info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0])
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }
    };
    (packed_data, final_col)
}
