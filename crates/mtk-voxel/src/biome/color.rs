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

/// One biome's resolved tint colors, resolved once per section sampling pass.
#[derive(Clone, Copy)]
struct ResolvedBiomeTint {
    palette: &'static mtk_material::BiomePalette,
    colormap_uv: [f32; 2],
    grass: [f32; 4],
    foliage: [f32; 4],
    dry_foliage: [f32; 4],
    water: [f32; 4],
    is_swamp: bool,
}

/// Reusable biome -> color resolver for one section's sampling pass.
///
/// A section contains a handful of distinct biomes but thousands of kernel samples, so resolving
/// `get_biome_palette` (a name scan) and the four linear-color accessors once per distinct name is
/// the difference between ~25 lookups per column and ~100. Entries are copied out, never borrowed,
/// so the mutable scratch never aliases the caller's storage borrow.
pub(crate) struct BiomeTintScratch {
    entries: Vec<(String, ResolvedBiomeTint)>,
}

impl BiomeTintScratch {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::with_capacity(8),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[inline]
    fn resolve(&mut self, biome: &str) -> ResolvedBiomeTint {
        if let Some((_, resolved)) = self.entries.iter().find(|(name, _)| name == biome) {
            return *resolved;
        }
        let palette = mtk_material::get_biome_palette(biome);
        let resolved = ResolvedBiomeTint {
            palette,
            colormap_uv: mtk_material::get_colormap_uv(palette.temperature, palette.humidity),
            grass: palette.grass_linear(),
            foliage: palette.foliage_linear(),
            dry_foliage: palette.dry_foliage_linear(),
            water: palette.water_linear(),
            is_swamp: palette.id == "swamp" || palette.id == "mangrove_swamp",
        };
        self.entries.push((biome.to_string(), resolved));
        resolved
    }
}

/// Running sums of the uniform 5x5 biome kernel.
///
/// Both the closure-based path and the pre-sampled grid path in `storage::world::padded` feed this
/// accumulator, so a kernel change cannot make the two disagree.
#[derive(Default)]
pub(crate) struct BiomeColumnSums {
    total_w: f32,
    sum_u: f32,
    sum_v: f32,
    sum_grass: [f32; 4],
    sum_foliage: [f32; 4],
    sum_dry: [f32; 4],
    sum_water: [f32; 4],
    any_custom_grass: bool,
    any_custom_foliage: bool,
    any_custom_dry_foliage: bool,
}

impl BiomeColumnSums {
    #[inline]
    pub(crate) fn accumulate(
        &mut self,
        scratch: &mut BiomeTintScratch,
        biome: &str,
        x: i32,
        z: i32,
        weight: f32,
    ) {
        let resolved = scratch.resolve(biome);
        let palette = resolved.palette;

        if palette.has_custom_grass {
            self.any_custom_grass = true;
        }
        if palette.has_custom_foliage {
            self.any_custom_foliage = true;
        }
        if palette.has_custom_dry_foliage {
            self.any_custom_dry_foliage = true;
        }

        self.total_w += weight;
        self.sum_u += resolved.colormap_uv[0] * weight;
        self.sum_v += resolved.colormap_uv[1] * weight;

        // Swamp grass keeps a position-dependent Perlin modulation on top of the biome color.
        let grass = if resolved.is_swamp {
            mtk_material::get_swamp_grass_color(x as f32, z as f32)
        } else {
            resolved.grass
        };
        accumulate_rgba(&mut self.sum_grass, grass, weight);
        accumulate_rgba(&mut self.sum_foliage, resolved.foliage, weight);
        accumulate_rgba(&mut self.sum_dry, resolved.dry_foliage, weight);
        accumulate_rgba(&mut self.sum_water, resolved.water, weight);
    }

    pub(crate) fn finish(self) -> SmoothedBiomeColumn {
        let inv_w = if self.total_w > 0.0 {
            1.0 / self.total_w
        } else {
            1.0
        };
        SmoothedBiomeColumn {
            colormap_uv: [self.sum_u * inv_w, self.sum_v * inv_w],
            grass_color: scale_rgba(self.sum_grass, inv_w),
            foliage_color: scale_rgba(self.sum_foliage, inv_w),
            dry_foliage_color: scale_rgba(self.sum_dry, inv_w),
            water_color: scale_rgba(self.sum_water, inv_w),
            has_custom_grass: self.any_custom_grass,
            has_custom_foliage: self.any_custom_foliage,
            has_custom_dry_foliage: self.any_custom_dry_foliage,
        }
    }
}

#[inline]
fn accumulate_rgba(sum: &mut [f32; 4], color: [f32; 4], weight: f32) {
    sum[0] += color[0] * weight;
    sum[1] += color[1] * weight;
    sum[2] += color[2] * weight;
    sum[3] += color[3] * weight;
}

#[inline]
fn scale_rgba(sum: [f32; 4], factor: f32) -> [f32; 4] {
    [
        sum[0] * factor,
        sum[1] * factor,
        sum[2] * factor,
        sum[3] * factor,
    ]
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
    let mut scratch = BiomeTintScratch::new();
    let mut sums = BiomeColumnSums::default();
    for &(dx, dz, weight) in BIOME_KERNEL_R2 {
        let bx = x + dx;
        let bz = z + dz;
        let biome = get_biome(bx, y, bz);
        sums.accumulate(&mut scratch, biome.as_ref(), bx, bz, weight);
    }
    sums.finish()
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

    /// `storage::world::padded` averages a pre-sampled 5x5 grid instead of calling the closure per
    /// column. That shortcut is only valid while the kernel stays a uniform box filter.
    #[test]
    fn kernel_is_a_uniform_5x5_box() {
        assert_eq!(BIOME_KERNEL_R2.len(), 25);
        let mut seen = std::collections::HashSet::new();
        for &(dx, dz, weight) in BIOME_KERNEL_R2 {
            assert!((-2..=2).contains(&dx) && (-2..=2).contains(&dz));
            assert_eq!(weight, 1.0);
            assert!(seen.insert((dx, dz)), "duplicate kernel offset {dx},{dz}");
        }
        assert_eq!(seen.len(), 25);
    }

    #[test]
    fn grid_path_matches_the_closure_path() {
        // A 20x20 grid of biome names around the section, sampled once, must produce exactly the
        // same column as calling the resolver 25 times.
        let names = [
            "minecraft:plains",
            "minecraft:desert",
            "minecraft:swamp",
            "minecraft:taiga",
        ];
        let at = |x: i32, z: i32| names[(((x * 31 + z * 17) & 0x7fff_ffff) % 4) as usize];
        let mut scratch = BiomeTintScratch::new();

        for (x, z) in [(0, 0), (-3, 7), (15, 15)] {
            let expected = get_smoothed_column_biome(|bx, _y, bz| at(bx, bz), x, 64, z);
            let mut sums = BiomeColumnSums::default();
            for &(dx, dz, weight) in BIOME_KERNEL_R2 {
                sums.accumulate(&mut scratch, at(x + dx, z + dz), x + dx, z + dz, weight);
            }
            assert_eq!(sums.finish(), expected, "mismatch at {x},{z}");
        }
    }
}
