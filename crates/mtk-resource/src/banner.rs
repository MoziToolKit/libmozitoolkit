//! Vanilla banner patterns, short-code normalization, and 16 vanilla dye palettes.

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

/// 16 Canonical Vanilla Dye colors with names and RGB ([name, [R, G, B]]).
pub const VANILLA_BANNER_DYES: [(&str, [u8; 3]); 16] = [
    ("white", [249, 255, 254]),
    ("orange", [249, 128, 29]),
    ("magenta", [199, 78, 189]),
    ("light_blue", [58, 179, 218]),
    ("yellow", [254, 216, 61]),
    ("lime", [128, 199, 31]),
    ("pink", [243, 139, 170]),
    ("gray", [71, 79, 82]),
    ("light_gray", [157, 157, 151]),
    ("cyan", [22, 156, 156]),
    ("purple", [137, 50, 184]),
    ("blue", [60, 68, 170]),
    ("brown", [131, 84, 50]),
    ("green", [94, 124, 22]),
    ("red", [176, 46, 38]),
    ("black", [29, 29, 33]),
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

/// Resolves sRGB color tuple for a dye index.
#[inline]
pub fn dye_rgb(id: u8) -> [u8; 3] {
    DYE_RGB.get(id as usize).copied().unwrap_or(DYE_RGB[0])
}

/// Resolves a dye name or integer index string to canonical sRGB [R, G, B].
pub fn dye_color_to_rgb(dye_name_or_id: &str) -> [u8; 3] {
    if let Some(id) = name_to_dye_id(dye_name_or_id) {
        return dye_rgb(id);
    }
    DYE_RGB[0]
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

/// Normalizes legacy 2-character pattern short codes or full identifiers into standard pattern stems.
pub fn normalize_banner_pattern(code_or_name: &str) -> &'static str {
    let clean = code_or_name
        .strip_prefix("minecraft:")
        .unwrap_or(code_or_name);
    let stem = clean.strip_prefix("entity/banner/").unwrap_or(clean);
    let stem = stem.strip_prefix("banner/").unwrap_or(stem);
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
