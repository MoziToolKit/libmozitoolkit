//! # Authoritative Colormap Sampling & Memory Cache
//!
//! Provides resident Minecraft colormaps (grass, foliage, dry_foliage) for fast
//! lookup, resolving linear RGBA tint colors in Rust without requiring Blender
//! shader node tree texture sampling.
//!
//! Embedded vanilla PNGs are the fallback. A precompiled asset cache may provide
//! resource-pack overrides; opening such a cache installs them as the active set.

use std::sync::{Arc, OnceLock};

use super::palettes::{
    sample_colormap_hex, sample_colormap_pixel, sample_colormap_pixel_uv, srgb_to_linear,
};

const GRASS_PNG: &[u8] = include_bytes!("../../assets/colormap/grass.png");
const FOLIAGE_PNG: &[u8] = include_bytes!("../../assets/colormap/foliage.png");
const DRY_FOLIAGE_PNG: &[u8] = include_bytes!("../../assets/colormap/dry_foliage.png");

/// Resident RGB colormap buffers with their source dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaColormaps {
    pub grass_pixels: Vec<u8>,
    pub foliage_pixels: Vec<u8>,
    pub dry_foliage_pixels: Vec<u8>,
    pub grass_width: usize,
    pub grass_height: usize,
    pub foliage_width: usize,
    pub foliage_height: usize,
    pub dry_foliage_width: usize,
    pub dry_foliage_height: usize,
}

impl VanillaColormaps {
    fn decode_png(bytes: &[u8], name: &str) -> Result<(Vec<u8>, usize, usize), String> {
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|e| format!("failed to decode colormap {name}: {e}"))?;
        let rgb = image.to_rgb8();
        let (width, height) = rgb.dimensions();
        if width == 0 || height == 0 {
            return Err(format!("colormap {name} has zero dimensions"));
        }
        Ok((rgb.into_raw(), width as usize, height as usize))
    }

    /// Decodes embedded canonical vanilla colormaps.
    pub fn load_embedded() -> Self {
        let (grass_pixels, grass_width, grass_height) = Self::decode_png(GRASS_PNG, "grass.png")
            .expect("embedded vanilla grass colormap must be a valid PNG");
        let (foliage_pixels, foliage_width, foliage_height) =
            Self::decode_png(FOLIAGE_PNG, "foliage.png")
                .expect("embedded vanilla foliage colormap must be a valid PNG");
        let (dry_foliage_pixels, dry_foliage_width, dry_foliage_height) =
            Self::decode_png(DRY_FOLIAGE_PNG, "dry_foliage.png")
                .expect("embedded vanilla dry foliage colormap must be a valid PNG");

        Self {
            grass_pixels,
            foliage_pixels,
            dry_foliage_pixels,
            grass_width,
            grass_height,
            foliage_width,
            foliage_height,
            dry_foliage_width,
            dry_foliage_height,
        }
    }

    /// Builds a colormap set from embedded vanilla data, replacing only the
    /// channels supplied by a resource-pack cache.
    pub fn with_overrides(
        grass: Option<&[u8]>,
        foliage: Option<&[u8]>,
        dry_foliage: Option<&[u8]>,
    ) -> Result<Self, String> {
        let mut colormaps = Self::load_embedded();

        if let Some(bytes) = grass {
            let (pixels, width, height) = Self::decode_png(bytes, "grass.png")?;
            colormaps.grass_pixels = pixels;
            colormaps.grass_width = width;
            colormaps.grass_height = height;
        }
        if let Some(bytes) = foliage {
            let (pixels, width, height) = Self::decode_png(bytes, "foliage.png")?;
            colormaps.foliage_pixels = pixels;
            colormaps.foliage_width = width;
            colormaps.foliage_height = height;
        }
        if let Some(bytes) = dry_foliage {
            let (pixels, width, height) = Self::decode_png(bytes, "dry_foliage.png")?;
            colormaps.dry_foliage_pixels = pixels;
            colormaps.dry_foliage_width = width;
            colormaps.dry_foliage_height = height;
        }

        Ok(colormaps)
    }

    /// Sample grass sRGB color [0.0..1.0] from temperature and downfall.
    #[inline]
    pub fn sample_grass_srgb(&self, temperature: f32, downfall: f32) -> [f32; 3] {
        sample_colormap_pixel(
            &self.grass_pixels,
            self.grass_width as u32,
            self.grass_height as u32,
            temperature,
            downfall,
            3,
        )
    }

    /// Sample foliage sRGB color [0.0..1.0] from temperature and downfall.
    #[inline]
    pub fn sample_foliage_srgb(&self, temperature: f32, downfall: f32) -> [f32; 3] {
        sample_colormap_pixel(
            &self.foliage_pixels,
            self.foliage_width as u32,
            self.foliage_height as u32,
            temperature,
            downfall,
            3,
        )
    }

    /// Sample dry foliage sRGB color [0.0..1.0] from temperature and downfall.
    #[inline]
    pub fn sample_dry_foliage_srgb(&self, temperature: f32, downfall: f32) -> [f32; 3] {
        sample_colormap_pixel(
            &self.dry_foliage_pixels,
            self.dry_foliage_width as u32,
            self.dry_foliage_height as u32,
            temperature,
            downfall,
            3,
        )
    }

    /// Sample grass color directly as Linear RGBA [r, g, b, 1.0].
    #[inline]
    pub fn sample_grass_linear(&self, temperature: f32, downfall: f32) -> [f32; 4] {
        let srgb = self.sample_grass_srgb(temperature, downfall);
        [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
            1.0,
        ]
    }

    /// Sample foliage color directly as Linear RGBA [r, g, b, 1.0].
    #[inline]
    pub fn sample_foliage_linear(&self, temperature: f32, downfall: f32) -> [f32; 4] {
        let srgb = self.sample_foliage_srgb(temperature, downfall);
        [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
            1.0,
        ]
    }

    /// Sample dry foliage color directly as Linear RGBA [r, g, b, 1.0].
    #[inline]
    pub fn sample_dry_foliage_linear(&self, temperature: f32, downfall: f32) -> [f32; 4] {
        let srgb = self.sample_dry_foliage_srgb(temperature, downfall);
        [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
            1.0,
        ]
    }

    /// Sample grass sRGB color [0.0..1.0] from colormap UV coordinates.
    #[inline]
    pub fn sample_grass_srgb_uv(&self, u: f32, v: f32) -> [f32; 3] {
        sample_colormap_pixel_uv(
            &self.grass_pixels,
            self.grass_width as u32,
            self.grass_height as u32,
            u,
            v,
            3,
        )
    }

    /// Sample foliage sRGB color [0.0..1.0] from colormap UV coordinates.
    #[inline]
    pub fn sample_foliage_srgb_uv(&self, u: f32, v: f32) -> [f32; 3] {
        sample_colormap_pixel_uv(
            &self.foliage_pixels,
            self.foliage_width as u32,
            self.foliage_height as u32,
            u,
            v,
            3,
        )
    }

    /// Sample dry foliage sRGB color [0.0..1.0] from colormap UV coordinates.
    #[inline]
    pub fn sample_dry_foliage_srgb_uv(&self, u: f32, v: f32) -> [f32; 3] {
        sample_colormap_pixel_uv(
            &self.dry_foliage_pixels,
            self.dry_foliage_width as u32,
            self.dry_foliage_height as u32,
            u,
            v,
            3,
        )
    }

    /// Sample grass color directly as Linear RGBA [r, g, b, 1.0] from colormap UV coordinates.
    #[inline]
    pub fn sample_grass_linear_uv(&self, u: f32, v: f32) -> [f32; 4] {
        let srgb = self.sample_grass_srgb_uv(u, v);
        [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
            1.0,
        ]
    }

    /// Sample foliage color directly as Linear RGBA [r, g, b, 1.0] from colormap UV coordinates.
    #[inline]
    pub fn sample_foliage_linear_uv(&self, u: f32, v: f32) -> [f32; 4] {
        let srgb = self.sample_foliage_srgb_uv(u, v);
        [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
            1.0,
        ]
    }

    /// Sample dry foliage color directly as Linear RGBA [r, g, b, 1.0] from colormap UV coordinates.
    #[inline]
    pub fn sample_dry_foliage_linear_uv(&self, u: f32, v: f32) -> [f32; 4] {
        let srgb = self.sample_dry_foliage_srgb_uv(u, v);
        [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
            1.0,
        ]
    }

    /// Sample grass color directly as uppercase hex string `#RRGGBB`.
    #[inline]
    pub fn sample_grass_hex(&self, temperature: f32, downfall: f32) -> String {
        sample_colormap_hex(
            &self.grass_pixels,
            self.grass_width as u32,
            self.grass_height as u32,
            temperature,
            downfall,
            3,
        )
    }

    /// Sample foliage color directly as uppercase hex string `#RRGGBB`.
    #[inline]
    pub fn sample_foliage_hex(&self, temperature: f32, downfall: f32) -> String {
        sample_colormap_hex(
            &self.foliage_pixels,
            self.foliage_width as u32,
            self.foliage_height as u32,
            temperature,
            downfall,
            3,
        )
    }

    /// Sample dry foliage color directly as uppercase hex string `#RRGGBB`.
    #[inline]
    pub fn sample_dry_foliage_hex(&self, temperature: f32, downfall: f32) -> String {
        sample_colormap_hex(
            &self.dry_foliage_pixels,
            self.dry_foliage_width as u32,
            self.dry_foliage_height as u32,
            temperature,
            downfall,
            3,
        )
    }
}

/// Embedded vanilla fallback, decoded once.
static EMBEDDED_COLORMAPS: OnceLock<Arc<VanillaColormaps>> = OnceLock::new();

/// Returns the embedded vanilla colormaps.
#[inline]
pub fn get_default_colormaps() -> Arc<VanillaColormaps> {
    Arc::clone(EMBEDDED_COLORMAPS.get_or_init(|| Arc::new(VanillaColormaps::load_embedded())))
}

/// Applies the vanilla `GrassColorModifier.DARK_FOREST` to a base sRGB grass color:
/// `((baseColor & 0xFEFEFE) + 0x28340A) >> 1` (where 0x28340A is 2634762)
/// Returns the resulting Linear RGBA.
pub fn apply_dark_forest_grass_modifier(base_srgb: [f32; 3]) -> [f32; 4] {
    let r = (base_srgb[0] * 255.0).round().clamp(0.0, 255.0) as u32;
    let g = (base_srgb[1] * 255.0).round().clamp(0.0, 255.0) as u32;
    let b = (base_srgb[2] * 255.0).round().clamp(0.0, 255.0) as u32;

    let base_color = (r << 16) | (g << 8) | b;
    let modified = ((base_color & 0xFEFEFE) + 0x28340A) >> 1;

    let mr = ((modified >> 16) & 0xFF) as f32 / 255.0;
    let mg = ((modified >> 8) & 0xFF) as f32 / 255.0;
    let mb = (modified & 0xFF) as f32 / 255.0;

    [
        srgb_to_linear(mr),
        srgb_to_linear(mg),
        srgb_to_linear(mb),
        1.0,
    ]
}

/// Computes the authoritative linear RGBA tint for a given temperature, humidity, and category.
/// If `is_dark_forest` is true, applies the dark forest modifier to grass tint.
pub fn compute_resolved_tint_color(
    tint_type: u8,
    temperature: f32,
    humidity: f32,
    is_dark_forest: bool,
) -> [f32; 4] {
    let cms = get_default_colormaps();
    match tint_type {
        crate::biome::TINT_TYPE_GRASS => {
            if is_dark_forest {
                let base = cms.sample_grass_srgb(temperature, humidity);
                apply_dark_forest_grass_modifier(base)
            } else {
                cms.sample_grass_linear(temperature, humidity)
            }
        }
        crate::biome::TINT_TYPE_FOLIAGE => cms.sample_foliage_linear(temperature, humidity),
        crate::biome::TINT_TYPE_DRY_FOLIAGE => cms.sample_dry_foliage_linear(temperature, humidity),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_png(rgb: [u8; 3]) -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
        let image = RgbImage::from_pixel(1, 1, Rgb(rgb));
        let mut buffer = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image)
            .write_to(&mut buffer, ImageFormat::Png)
            .expect("PNG encode");
        buffer.into_inner()
    }

    #[test]
    fn cached_colormap_override_is_used() {
        let red = solid_png([255, 0, 0]);
        let colormaps =
            VanillaColormaps::with_overrides(Some(&red), None, None).expect("custom colormap");
        assert_eq!((colormaps.grass_width, colormaps.grass_height), (1, 1));
        assert_eq!(
            colormaps.sample_grass_linear(0.5, 0.5),
            [1.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn invalid_colormap_is_rejected() {
        assert!(VanillaColormaps::with_overrides(Some(b"not a png"), None, None).is_err());
    }
}
