use std::collections::HashMap;

/// Precomputed sample offsets and weights for 2D horizontal kernel radius `R=2` (5x5 uniform box filter matching Vanilla).
pub(crate) const BIOME_KERNEL_R2: &[(i32, i32, f32)] = &[
    (-2, -2, 1.0),
    (-2, -1, 1.0),
    (-2, 0, 1.0),
    (-2, 1, 1.0),
    (-2, 2, 1.0),
    (-1, -2, 1.0),
    (-1, -1, 1.0),
    (-1, 0, 1.0),
    (-1, 1, 1.0),
    (-1, 2, 1.0),
    (0, -2, 1.0),
    (0, -1, 1.0),
    (0, 0, 1.0),
    (0, 1, 1.0),
    (0, 2, 1.0),
    (1, -2, 1.0),
    (1, -1, 1.0),
    (1, 0, 1.0),
    (1, 1, 1.0),
    (1, 2, 1.0),
    (2, -2, 1.0),
    (2, -1, 1.0),
    (2, 0, 1.0),
    (2, 1, 1.0),
    (2, 2, 1.0),
];

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Precomputed smoothed biome colormap UV and linear colors for a single 1D column (x, z).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SmoothedBiomeColumn {
    pub colormap_uv: [f32; 2],
    pub grass_color: [f32; 4],
    pub foliage_color: [f32; 4],
    pub dry_foliage_color: [f32; 4],
    pub water_color: [f32; 4],
    pub has_custom_grass: bool,
    pub has_custom_foliage: bool,
    pub has_custom_dry_foliage: bool,
}

impl Default for SmoothedBiomeColumn {
    fn default() -> Self {
        let pal = mtk_material::get_biome_palette("plains");
        Self {
            colormap_uv: pal.colormap_uv(),
            grass_color: pal.grass_linear(),
            foliage_color: pal.foliage_linear(),
            dry_foliage_color: pal.dry_foliage_linear(),
            water_color: pal.water_linear(),
            has_custom_grass: pal.has_custom_grass,
            has_custom_foliage: pal.has_custom_foliage,
            has_custom_dry_foliage: pal.has_custom_dry_foliage,
        }
    }
}

/// Computes unsmoothed single-point biome colors and colormap UVs at `(x, z)` (preserving swamp Perlin noise).
pub fn get_point_column_biome(biome: &str, x: i32, z: i32) -> SmoothedBiomeColumn {
    let pal = mtk_material::get_biome_palette(biome);
    let is_swamp = pal.id == "swamp" || pal.id == "mangrove_swamp";
    let grass_color = if is_swamp {
        mtk_material::get_swamp_grass_color(x as f32, z as f32)
    } else {
        pal.grass_linear()
    };
    SmoothedBiomeColumn {
        colormap_uv: pal.colormap_uv(),
        grass_color,
        foliage_color: pal.foliage_linear(),
        dry_foliage_color: pal.dry_foliage_linear(),
        water_color: pal.water_linear(),
        has_custom_grass: pal.has_custom_grass,
        has_custom_foliage: pal.has_custom_foliage,
        has_custom_dry_foliage: pal.has_custom_dry_foliage,
    }
}

/// Computes smooth biome blending over `(x ± 2, z ± 2)` horizontal neighborhood for all color channels.
pub fn get_smoothed_column_biome<F, S>(
    mut get_biome: F,
    x: i32,
    y: i32,
    z: i32,
) -> SmoothedBiomeColumn
where
    F: FnMut(i32, i32, i32) -> S,
    S: AsRef<str>,
{
    let mut total_w = 0.0f32;
    let mut sum_u = 0.0f32;
    let mut sum_v = 0.0f32;
    let mut sum_grass = [0.0f32; 4];
    let mut sum_foliage = [0.0f32; 4];
    let mut sum_dry = [0.0f32; 4];
    let mut sum_water = [0.0f32; 4];
    let mut any_custom_grass = false;
    let mut any_custom_foliage = false;
    let mut any_custom_dry_foliage = false;

    let mut cache = HashMap::<String, (&'static mtk_material::BiomePalette, [f32; 2])>::new();

    for &(dx, dz, weight) in BIOME_KERNEL_R2 {
        let bx = x + dx;
        let bz = z + dz;
        let b_raw = get_biome(bx, y, bz);
        let b_name = b_raw.as_ref();

        let (pal, uv) = if let Some(&(pal, uv)) = cache.get(b_name) {
            (pal, uv)
        } else {
            let pal = mtk_material::get_biome_palette(b_name);
            let uv = mtk_material::get_colormap_uv(pal.temperature, pal.humidity);
            cache.insert(b_name.to_string(), (pal, uv));
            (pal, uv)
        };

        if pal.has_custom_grass {
            any_custom_grass = true;
        }
        if pal.has_custom_foliage {
            any_custom_foliage = true;
        }
        if pal.has_custom_dry_foliage {
            any_custom_dry_foliage = true;
        }

        total_w += weight;
        sum_u += uv[0] * weight;
        sum_v += uv[1] * weight;

        let is_swamp = pal.id == "swamp" || pal.id == "mangrove_swamp";
        let gc = if is_swamp {
            mtk_material::get_swamp_grass_color(bx as f32, bz as f32)
        } else {
            pal.grass_linear()
        };

        sum_grass[0] += gc[0] * weight;
        sum_grass[1] += gc[1] * weight;
        sum_grass[2] += gc[2] * weight;
        sum_grass[3] += gc[3] * weight;

        let fc = pal.foliage_linear();
        sum_foliage[0] += fc[0] * weight;
        sum_foliage[1] += fc[1] * weight;
        sum_foliage[2] += fc[2] * weight;
        sum_foliage[3] += fc[3] * weight;

        let dc = pal.dry_foliage_linear();
        sum_dry[0] += dc[0] * weight;
        sum_dry[1] += dc[1] * weight;
        sum_dry[2] += dc[2] * weight;
        sum_dry[3] += dc[3] * weight;

        let wc = pal.water_linear();
        sum_water[0] += wc[0] * weight;
        sum_water[1] += wc[1] * weight;
        sum_water[2] += wc[2] * weight;
        sum_water[3] += wc[3] * weight;
    }

    let inv_w = if total_w > 0.0 { 1.0 / total_w } else { 1.0 };
    SmoothedBiomeColumn {
        colormap_uv: [sum_u * inv_w, sum_v * inv_w],
        grass_color: [
            sum_grass[0] * inv_w,
            sum_grass[1] * inv_w,
            sum_grass[2] * inv_w,
            sum_grass[3] * inv_w,
        ],
        foliage_color: [
            sum_foliage[0] * inv_w,
            sum_foliage[1] * inv_w,
            sum_foliage[2] * inv_w,
            sum_foliage[3] * inv_w,
        ],
        dry_foliage_color: [
            sum_dry[0] * inv_w,
            sum_dry[1] * inv_w,
            sum_dry[2] * inv_w,
            sum_dry[3] * inv_w,
        ],
        water_color: [
            sum_water[0] * inv_w,
            sum_water[1] * inv_w,
            sum_water[2] * inv_w,
            sum_water[3] * inv_w,
        ],
        has_custom_grass: any_custom_grass,
        has_custom_foliage: any_custom_foliage,
        has_custom_dry_foliage: any_custom_dry_foliage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothed_biome() {
        let col = get_smoothed_column_biome(|_x, _y, _z| "minecraft:plains".to_string(), 0, 64, 0);
        assert!(col.colormap_uv[0] > 0.0);
        assert!(col.water_color[3] > 0.0);
    }
}
