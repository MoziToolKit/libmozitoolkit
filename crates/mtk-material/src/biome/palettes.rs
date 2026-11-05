//! Canonical Minecraft 26.2 Biome Palettes, Colormap UV Mapping, and Color Conversions.

/// Convert standard sRGB color component (0.0 .. 1.0) to Linear RGB.
#[inline]
pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Parse a hex color string (e.g. "#91BD59" or "91BD59") into sRGB float components `[r, g, b, a]`.
pub(crate) fn hex_to_srgb(hex: &str) -> [f32; 4] {
    let clean = hex.trim().trim_start_matches('#');
    let bytes = clean.as_bytes();
    if bytes.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255) as f32 / 255.0;
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(255) as f32 / 255.0;
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255) as f32 / 255.0;
        [r, g, b, 1.0]
    } else if bytes.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255) as f32 / 255.0;
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(255) as f32 / 255.0;
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255) as f32 / 255.0;
        let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255) as f32 / 255.0;
        [r, g, b, a]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

/// Parse a hex color string into Linear RGBA float components for Blender shaders.
pub fn hex_to_linear_rgba(hex: &str) -> [f32; 4] {
    let srgb = hex_to_srgb(hex);
    [
        srgb_to_linear(srgb[0]),
        srgb_to_linear(srgb[1]),
        srgb_to_linear(srgb[2]),
        srgb[3],
    ]
}

/// Compute standard Minecraft triangular Colormap UV coordinates from temperature and humidity.
#[inline]
pub fn get_colormap_uv(temperature: f32, humidity: f32) -> [f32; 2] {
    let t = temperature.clamp(0.0, 1.0);
    let h = humidity.clamp(0.0, 1.0) * t;
    [1.0 - t, h]
}

/// Canonical metadata and color definition for a single Minecraft biome.
#[derive(Debug, Clone, PartialEq)]
pub struct BiomePalette {
    pub id: &'static str,
    pub name: &'static str,
    pub grass_hex: &'static str,
    pub foliage_hex: &'static str,
    pub dry_foliage_hex: &'static str,
    pub water_hex: &'static str,
    pub temperature: f32,
    pub humidity: f32,
    pub has_custom_grass: bool,
    pub has_custom_foliage: bool,
    pub has_custom_dry_foliage: bool,
}

use std::sync::LazyLock;

static CANONICAL_LINEAR_COLORS: LazyLock<Vec<([f32; 4], [f32; 4], [f32; 4], [f32; 4])>> =
    LazyLock::new(|| {
        CANONICAL_BIOMES
            .iter()
            .map(|b| {
                (
                    hex_to_linear_rgba(b.grass_hex),
                    hex_to_linear_rgba(b.foliage_hex),
                    hex_to_linear_rgba(b.dry_foliage_hex),
                    hex_to_linear_rgba(b.water_hex),
                )
            })
            .collect()
    });

impl BiomePalette {
    #[inline]
    fn precomputed_index(&self) -> Option<usize> {
        let base = CANONICAL_BIOMES.as_ptr() as usize;
        let cur = self as *const BiomePalette as usize;
        let stride = std::mem::size_of::<BiomePalette>();
        #[allow(clippy::manual_slice_size_calculation)]
        let total_bytes = CANONICAL_BIOMES.len() * stride;
        if cur >= base && cur < base + total_bytes && (cur - base) % stride == 0 {
            Some((cur - base) / stride)
        } else {
            None
        }
    }

    #[inline]
    pub fn grass_linear(&self) -> [f32; 4] {
        if let Some(idx) = self.precomputed_index() {
            CANONICAL_LINEAR_COLORS[idx].0
        } else {
            hex_to_linear_rgba(self.grass_hex)
        }
    }

    #[inline]
    pub fn foliage_linear(&self) -> [f32; 4] {
        if let Some(idx) = self.precomputed_index() {
            CANONICAL_LINEAR_COLORS[idx].1
        } else {
            hex_to_linear_rgba(self.foliage_hex)
        }
    }

    #[inline]
    pub fn dry_foliage_linear(&self) -> [f32; 4] {
        if let Some(idx) = self.precomputed_index() {
            CANONICAL_LINEAR_COLORS[idx].2
        } else {
            hex_to_linear_rgba(self.dry_foliage_hex)
        }
    }

    #[inline]
    pub fn water_linear(&self) -> [f32; 4] {
        if let Some(idx) = self.precomputed_index() {
            CANONICAL_LINEAR_COLORS[idx].3
        } else {
            hex_to_linear_rgba(self.water_hex)
        }
    }

    #[inline]
    pub fn colormap_uv(&self) -> [f32; 2] {
        get_colormap_uv(self.temperature, self.humidity)
    }
}

pub use super::canonical_data::CANONICAL_BIOMES;

/// Index of the canonical `plains` palette inside [`CANONICAL_BIOMES`].
const PLAINS_BIOME_INDEX: usize = 40;

/// Returns the canonical `plains` palette, used as the default tint color source.
///
/// Prefer this over `get_biome_palette("plains")` on hot paths: it performs no
/// string normalization and no allocation.
#[inline]
pub fn default_biome_palette() -> &'static BiomePalette {
    debug_assert_eq!(CANONICAL_BIOMES[PLAINS_BIOME_INDEX].id, "plains");
    &CANONICAL_BIOMES[PLAINS_BIOME_INDEX]
}

/// Look up a canonical BiomePalette by name or ID (case-insensitive).
///
/// Allocation-free: comparisons are ASCII case-insensitive instead of
/// lowercasing every canonical name on each call.
pub fn get_biome_palette(name_or_id: &str) -> &'static BiomePalette {
    let clean = name_or_id.trim();
    let unnamespaced = match clean.split_once(':') {
        Some((ns, rest)) if ns.eq_ignore_ascii_case("minecraft") => rest,
        _ => clean,
    };

    for b in CANONICAL_BIOMES {
        if b.id.eq_ignore_ascii_case(unnamespaced) || b.name.eq_ignore_ascii_case(unnamespaced) {
            return b;
        }
    }
    default_biome_palette()
}

/// Blend multiple biome colors weighted together.
pub(crate) fn blend_biome_colors(biome_weights: &[(&str, f32)], channel: &str) -> [f32; 4] {
    let mut total_w = 0.0f32;
    let mut sum_r = 0.0f32;
    let mut sum_g = 0.0f32;
    let mut sum_b = 0.0f32;
    let mut sum_a = 0.0f32;

    for &(b_name, w) in biome_weights {
        if w <= 0.0 {
            continue;
        }
        let pal = get_biome_palette(b_name);
        let col = match channel {
            "grass" => pal.grass_linear(),
            "foliage" => pal.foliage_linear(),
            "dry_foliage" => pal.dry_foliage_linear(),
            "water" => pal.water_linear(),
            _ => [1.0, 1.0, 1.0, 1.0],
        };
        total_w += w;
        sum_r += col[0] * w;
        sum_g += col[1] * w;
        sum_b += col[2] * w;
        sum_a += col[3] * w;
    }

    if total_w > 0.0 {
        let inv = 1.0 / total_w;
        [sum_r * inv, sum_g * inv, sum_b * inv, sum_a * inv]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

/// Sample normalized sRGB float components `[r, g, b]` from 2D colormap image bytes.
///
/// Strictly aligns with Minecraft's `net.minecraft.world.level.ColorMapColorUtil.get`:
/// ```text
/// rain = rain * temp;
/// int x = (int)((1.0 - temp) * 255.0);
/// int y = (int)((1.0 - rain) * 255.0);
/// int index = (y << 8) | x;
/// ```
#[inline]
pub(crate) fn sample_colormap_pixel(
    image_pixels: &[u8],
    width: u32,
    height: u32,
    temperature: f32,
    downfall: f32,
    channels: usize,
) -> [f32; 3] {
    let [r, g, b] =
        sample_colormap_pixel_u8(image_pixels, width, height, temperature, downfall, channels);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// Sample raw sRGB bytes `[r, g, b]` from a colormap buffer using authoritative Minecraft truncation.
#[inline]
pub(crate) fn sample_colormap_pixel_u8(
    image_pixels: &[u8],
    width: u32,
    height: u32,
    temperature: f32,
    downfall: f32,
    channels: usize,
) -> [u8; 3] {
    if width == 0 || height == 0 || image_pixels.is_empty() || channels < 3 {
        return [255, 255, 255];
    }
    let temp = (temperature as f64).clamp(0.0, 1.0);
    let hum = (downfall as f64).clamp(0.0, 1.0);
    let rain = hum * temp;

    // Strict alignment with Java (int)((1.0 - temp) * 255.0) truncation
    let x = (((1.0 - temp) * (width as f64 - 1.0)) as usize).min(width as usize - 1);
    let y = (((1.0 - rain) * (height as f64 - 1.0)) as usize).min(height as usize - 1);

    let idx = (y * width as usize + x) * channels;
    if idx + 2 < image_pixels.len() {
        [
            image_pixels[idx],
            image_pixels[idx + 1],
            image_pixels[idx + 2],
        ]
    } else {
        [255, 255, 255]
    }
}

/// Sample normalized sRGB floats `[r, g, b]` from a colormap buffer directly using normalized UV coordinates.
#[inline]
pub(crate) fn sample_colormap_pixel_uv(
    image_pixels: &[u8],
    width: u32,
    height: u32,
    u: f32,
    v: f32,
    channels: usize,
) -> [f32; 3] {
    let [r, g, b] = sample_colormap_pixel_uv_u8(image_pixels, width, height, u, v, channels);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// Sample raw sRGB bytes `[r, g, b]` from a colormap buffer directly using normalized UV coordinates.
#[inline]
pub(crate) fn sample_colormap_pixel_uv_u8(
    image_pixels: &[u8],
    width: u32,
    height: u32,
    u: f32,
    v: f32,
    channels: usize,
) -> [u8; 3] {
    if width == 0 || height == 0 || image_pixels.is_empty() || channels < 3 {
        return [255, 255, 255];
    }
    let u_clamped = (u as f64).clamp(0.0, 1.0);
    let v_clamped = (v as f64).clamp(0.0, 1.0);

    let x = ((u_clamped * (width as f64 - 1.0)) as usize).min(width as usize - 1);
    let y = (((1.0 - v_clamped) * (height as f64 - 1.0)) as usize).min(height as usize - 1);

    let idx = (y * width as usize + x) * channels;
    if idx + 2 < image_pixels.len() {
        [
            image_pixels[idx],
            image_pixels[idx + 1],
            image_pixels[idx + 2],
        ]
    } else {
        [255, 255, 255]
    }
}

/// Sample colormap color and format directly as uppercase hex string (e.g. `"#91BD59"`).
#[inline]
pub(crate) fn sample_colormap_hex(
    image_pixels: &[u8],
    width: u32,
    height: u32,
    temperature: f32,
    downfall: f32,
    channels: usize,
) -> String {
    let [r, g, b] =
        sample_colormap_pixel_u8(image_pixels, width, height, temperature, downfall, channels);
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod palette_lookup_tests {
    use super::*;

    #[test]
    fn plains_index_matches_canonical_table() {
        assert_eq!(CANONICAL_BIOMES[PLAINS_BIOME_INDEX].id, "plains");
        assert_eq!(default_biome_palette().id, "plains");
    }

    #[test]
    fn lookup_is_case_and_namespace_insensitive() {
        for expected in ["plains", "minecraft:plains", "Minecraft:PLAINS", " Plains "] {
            assert_eq!(get_biome_palette(expected).id, "plains", "{expected}");
        }
        // Display names (with spaces and capitals) still resolve.
        assert_eq!(get_biome_palette("Bamboo Jungle").id, "bamboo_jungle");
        assert_eq!(get_biome_palette("bamboo_jungle").id, "bamboo_jungle");
        // Unknown ids fall back to plains.
        assert_eq!(get_biome_palette("definitely_not_a_biome").id, "plains");
        assert_eq!(get_biome_palette("").id, "plains");
    }

    #[test]
    fn every_canonical_biome_resolves_to_itself() {
        for b in CANONICAL_BIOMES {
            assert_eq!(get_biome_palette(b.id).id, b.id);
            // Display names are not unique (`sparce_jungle` / `sparse_jungle` share
            // "Sparse Jungle"), so only require the resolved display name to match.
            assert_eq!(
                get_biome_palette(b.name).name.to_ascii_lowercase(),
                b.name.to_ascii_lowercase()
            );
            assert_eq!(get_biome_palette(&format!("minecraft:{}", b.id)).id, b.id);
        }
    }
}
