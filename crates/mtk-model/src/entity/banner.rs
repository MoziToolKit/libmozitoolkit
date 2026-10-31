//! Banner entity definitions, pattern layers, short-code resolution, and 16 vanilla dye palettes.

use serde::{Deserialize, Serialize};

/// 16 Canonical Vanilla Dye colors.
pub const DYE_NAMES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// Resolves a dye index (0..15) into its canonical English name.
#[inline]
pub fn dye_id_to_name(id: u8) -> &'static str {
    DYE_NAMES.get(id as usize).copied().unwrap_or("white")
}

/// Resolves a dye color name or string integer to a 0..15 index.
pub fn name_to_dye_id(name: &str) -> Option<u8> {
    if let Ok(id) = name.parse::<u8>() {
        if id < 16 {
            return Some(id);
        }
    }
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);
    let stem = clean.strip_suffix("_dye").unwrap_or(clean);
    DYE_NAMES.iter().position(|&d| d == stem).map(|i| i as u8)
}

/// Extracts the base dye color name from a banner blockstate name
/// (e.g. `minecraft:red_banner[rotation=4]` -> `"red"`, `minecraft:blue_wall_banner[facing=north]` -> `"blue"`).
pub fn extract_banner_color_from_state(state_str: &str) -> Option<&'static str> {
    let clean = state_str.split('[').next().unwrap_or(state_str);
    let clean = clean.strip_prefix("minecraft:").unwrap_or(clean);
    let stem = if let Some(s) = clean.strip_suffix("_wall_banner") {
        s
    } else {
        clean.strip_suffix("_banner")?
    };
    DYE_NAMES.iter().find(|&&d| d == stem).copied()
}

/// Canonical sRGB colors for vanilla banner dye rendering ([R, G, B] in 0..255).
pub const DYE_RGB: [[u8; 3]; 16] = [
    [249, 255, 254], // 0: White
    [249, 128, 29],  // 1: Orange
    [199, 78, 189],  // 2: Magenta
    [58, 179, 218],  // 3: Light Blue
    [254, 216, 61],  // 4: Yellow
    [128, 199, 31],  // 5: Lime
    [243, 139, 170], // 6: Pink
    [71, 79, 82],    // 7: Gray
    [157, 157, 151], // 8: Light Gray
    [22, 156, 156],  // 9: Cyan
    [137, 50, 184],  // 10: Purple
    [60, 68, 170],   // 11: Blue
    [131, 84, 50],   // 12: Brown
    [94, 124, 22],   // 13: Green
    [176, 46, 38],   // 14: Red
    [29, 29, 33],    // 15: Black
];

/// Resolves sRGB color tuple for a dye index.
#[inline]
pub fn dye_rgb(id: u8) -> [u8; 3] {
    DYE_RGB.get(id as usize).copied().unwrap_or(DYE_RGB[0])
}

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

/// Normalizes legacy 2-character pattern short codes or full identifiers into standard pattern stems.
pub fn normalize_banner_pattern(code_or_name: &str) -> &'static str {
    let clean = code_or_name
        .strip_prefix("minecraft:")
        .unwrap_or(code_or_name);
    let stem = clean.strip_prefix("banner/").unwrap_or(clean);
    let stem = stem.strip_prefix("pattern_").unwrap_or(stem);

    match stem {
        "b" | "base" | "banner_base" => "base",
        "bs" | "stripe_bottom" => "stripe_bottom",
        "ts" | "stripe_top" => "stripe_top",
        "ls" | "stripe_left" => "stripe_left",
        "rs" | "stripe_right" => "stripe_right",
        "cs" | "stripe_center" => "stripe_center",
        "ms" | "stripe_middle" => "stripe_middle",
        "drs" | "stripe_downright" => "stripe_downright",
        "dls" | "stripe_downleft" => "stripe_downleft",
        "ss" | "small_stripes" => "small_stripes",
        "cr" | "cross" => "cross",
        "sc" | "straight_cross" => "straight_cross",
        "ld" | "diagonal_left" => "diagonal_left",
        "rud" | "diagonal_right" => "diagonal_right",
        "lud" | "diagonal_up_left" => "diagonal_up_left",
        "rd" | "diagonal_up_right" => "diagonal_up_right",
        "vh" | "half_vertical" => "half_vertical",
        "vhr" | "half_vertical_right" => "half_vertical_right",
        "hh" | "half_horizontal" => "half_horizontal",
        "hhb" | "half_horizontal_bottom" => "half_horizontal_bottom",
        "bl" | "square_bottom_left" => "square_bottom_left",
        "br" | "square_bottom_right" => "square_bottom_right",
        "tl" | "square_top_left" => "square_top_left",
        "tr" | "square_top_right" => "square_top_right",
        "bt" | "triangle_bottom" => "triangle_bottom",
        "tt" | "triangle_top" => "triangle_top",
        "bts" | "triangles_bottom" => "triangles_bottom",
        "tts" | "triangles_top" => "triangles_top",
        "mc" | "circle" => "circle",
        "mr" | "rhombus" => "rhombus",
        "bo" | "border" => "border",
        "cbo" | "curly_border" => "curly_border",
        "bri" | "bricks" => "bricks",
        "gra" | "gradient" => "gradient",
        "gru" | "gradient_up" => "gradient_up",
        "cre" | "creeper" => "creeper",
        "sku" | "skull" => "skull",
        "flo" | "flower" => "flower",
        "moj" | "mojang" => "mojang",
        "glb" | "globe" => "globe",
        "pig" | "piglin" => "piglin",
        "flw" | "flow" => "flow",
        "gus" | "guster" => "guster",
        _ => "base",
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
