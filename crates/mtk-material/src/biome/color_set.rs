//! Biome color sets and custom user settings resolution.

use super::colormap::VanillaColormaps;
use super::palettes::{blend_biome_colors, get_biome_palette};

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
