//! Banner entity definitions, pattern layers, short-code resolution, and 16 vanilla dye palettes.

use serde::{Deserialize, Serialize};

pub use mtk_resource::banner::{
    dye_color_to_rgb, dye_id_to_name, dye_rgb, extract_banner_color_from_state, name_to_dye_id,
    normalize_banner_pattern, DYE_NAMES, DYE_RGB,
};

/// A single pattern layer in a banner.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BannerPatternLayer {
    /// Normalized pattern identifier (e.g. "stripe_top", "flower", "mojang", "creeper").
    pub pattern: String,
    /// Dye color identifier (e.g. "red", "black", "light_gray") or index.
    pub color: String,
}

impl BannerPatternLayer {
    pub fn new(pattern: impl Into<String>, color: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            color: color.into(),
        }
    }

    /// Returns the dye ID (0..15).
    pub fn dye_id(&self) -> u8 {
        name_to_dye_id(&self.color).unwrap_or(0)
    }

    /// Resolves the static sprite path in the `banner_patterns` atlas category.
    pub fn sprite_path(&self) -> String {
        format!("entity/banner/{}", normalize_banner_pattern(&self.pattern))
    }
}

/// Computes the canonical dynamic texture identifier for a banner.
///
/// For plain banners (no patterns), resolves to `minecraft:banner/<base_color>`.
/// For patterned banners, resolves to `minecraft:banner/<base_color>+<pat>#<col>+...`.
pub fn banner_texture_identifier(base_color: &str, patterns: &[BannerPatternLayer]) -> String {
    let clean_base = base_color.strip_prefix("minecraft:").unwrap_or(base_color);
    let stem_base = clean_base.strip_suffix("_dye").unwrap_or(clean_base);
    let stem_base = stem_base.strip_suffix("_wall_banner").unwrap_or(stem_base);
    let stem_base = stem_base.strip_suffix("_banner").unwrap_or(stem_base);

    if patterns.is_empty() {
        return format!("minecraft:banner/{}", stem_base);
    }

    let mut id = format!("minecraft:banner/{}", stem_base);
    for p in patterns {
        let clean_pat = normalize_banner_pattern(&p.pattern);
        let clean_col = p.color.strip_prefix("minecraft:").unwrap_or(&p.color);
        let clean_col = clean_col.strip_suffix("_dye").unwrap_or(clean_col);
        id.push('+');
        id.push_str(clean_pat);
        id.push('#');
        id.push_str(clean_col);
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_banner_color_from_state() {
        assert_eq!(
            extract_banner_color_from_state("minecraft:red_banner[rotation=4]"),
            Some("red")
        );
        assert_eq!(
            extract_banner_color_from_state("minecraft:blue_wall_banner[facing=north]"),
            Some("blue")
        );
        assert_eq!(
            extract_banner_color_from_state("black_banner"),
            Some("black")
        );
        assert_eq!(
            extract_banner_color_from_state("light_gray_banner"),
            Some("light_gray")
        );
        assert_eq!(extract_banner_color_from_state("minecraft:stone"), None);
    }
}
