//! Hardcoded Minecraft Block Tints, Canonical Block Tint Registry, and Classifier.

use super::palettes::hex_to_linear_rgba;
use mtk_core::Direction;

pub const TINT_TYPE_NONE: u8 = 0;
pub const TINT_TYPE_GRASS: u8 = 1;
pub const TINT_TYPE_FOLIAGE: u8 = 2;
pub const TINT_TYPE_WATER: u8 = 3;
pub const TINT_TYPE_HARDCODED: u8 = 4;
pub const TINT_TYPE_DRY_FOLIAGE: u8 = 5;

/// Static table of vanilla blocks with fixed, non-colormap biome colors.
pub(crate) static HARDCODED_BLOCK_TINTS: &[(&str, &str)] = &[
    ("birch_leaves", "#80A755"),
    ("spruce_leaves", "#619961"),
    ("lily_pad", "#208030"),
    ("attached_melon_stem", "#E0C71C"),
    ("attached_pumpkin_stem", "#E0C71C"),
    ("melon_stem", "#E0C71C"),
    ("pumpkin_stem", "#E0C71C"),
    ("redstone_dust_line", "#4B0000"),
    ("redstone_dust_line0", "#4B0000"),
    ("redstone_dust_line1", "#4B0000"),
    ("redstone_dust_dot", "#4B0000"),
    ("redstone_wire", "#4B0000"),
    ("redstone_wire_line0", "#4B0000"),
    ("redstone_wire_line1", "#4B0000"),
    ("redstone_wire_dot", "#4B0000"),
];

/// Precomputed canonical hex strings for redstone wire power levels 0 to 15.
pub(crate) static REDSTONE_WIRE_TINT_HEX: [&str; 16] = [
    "#4B0000", "#580200", "#640500", "#700700", "#7C0A00", "#880C00", "#940F00", "#A01100",
    "#AC1400", "#B81600", "#C31900", "#CF1B00", "#DB1E00", "#E72000", "#F32300", "#FF2600",
];

/// Returns the sRGB float color `[r, g, b, 1.0]` for a redstone wire signal power level (0..15).
pub(crate) fn get_redstone_wire_srgb(power: u8) -> [f32; 4] {
    let p = power.min(15) as f32;
    let t = p / 15.0;
    let r = 0.3 + 0.7 * t;
    let g = if power == 0 { 0.0 } else { 0.15 * t };
    [r, g, 0.0, 1.0]
}

/// Returns the Linear RGBA float color for Blender shaders for redstone wire signal power level (0..15).
pub(crate) fn get_redstone_wire_color(power: u8) -> [f32; 4] {
    let srgb = get_redstone_wire_srgb(power);
    [
        super::palettes::srgb_to_linear(srgb[0]),
        super::palettes::srgb_to_linear(srgb[1]),
        super::palettes::srgb_to_linear(srgb[2]),
        1.0,
    ]
}

/// Returns the static hex color string for a redstone wire signal power level (0..15).
pub(crate) fn get_redstone_wire_hex(power: u8) -> &'static str {
    REDSTONE_WIRE_TINT_HEX[power.min(15) as usize]
}

/// Canonical swamp dual grass colors in linear RGBA.
pub(crate) static SWAMP_GRASS_LOW_HEX: &str = "#4C763C";
pub(crate) static SWAMP_GRASS_HIGH_HEX: &str = "#6A7039";

/// Computes the vanilla swamp grass color using Perlin noise at world coordinates `(x, z)`.
///
/// Follows Minecraft vanilla swamp variation: Perlin noise sampled at frequency 0.0225.
/// When noise < -0.1, uses `#4C763C`, otherwise uses `#6A7039`.
pub fn get_swamp_grass_color(x: f32, z: f32) -> [f32; 4] {
    let n = mtk_core::perlin_noise_3d(x * 0.0225, 0.0, z * 0.0225, 2345);
    if n < -0.1 {
        hex_to_linear_rgba(SWAMP_GRASS_LOW_HEX)
    } else {
        hex_to_linear_rgba(SWAMP_GRASS_HIGH_HEX)
    }
}

/// Canonical Minecraft Block Colors Registry.
/// Maps block stem to layer definitions: (category, default_weight).
pub(crate) static BLOCK_TINT_REGISTRY: &[(&str, &[(&str, f32)])] = &[
    // Grass & Flora
    ("grass_block", &[("grass", 1.0)]),
    ("short_grass", &[("grass", 1.0)]),
    ("grass", &[("grass", 1.0)]),
    ("tall_grass", &[("grass", 1.0)]),
    ("fern", &[("grass", 1.0)]),
    ("large_fern", &[("grass", 1.0)]),
    ("potted_fern", &[("grass", 1.0)]),
    ("bush", &[("grass", 1.0)]),
    ("sugar_cane", &[("grass", 1.0)]),
    // Multi-layer Flora (Layer 0 = Petals/Blank, Layer 1 = Stem/Grass)
    ("pink_petals", &[("none", 0.0), ("grass", 1.0)]),
    ("wildflowers", &[("none", 0.0), ("grass", 1.0)]),
    ("bamboo", &[("grass", 1.0)]),
    // Foliage
    ("oak_leaves", &[("foliage", 1.0)]),
    ("jungle_leaves", &[("foliage", 1.0)]),
    ("acacia_leaves", &[("foliage", 1.0)]),
    ("dark_oak_leaves", &[("foliage", 1.0)]),
    ("vine", &[("foliage", 1.0)]),
    ("mangrove_leaves", &[("foliage", 1.0)]),
    // Dry Foliage
    ("leaf_litter", &[("dry_foliage", 1.0)]),
    ("pale_hanging_moss", &[("dry_foliage", 1.0)]),
    ("pale_hanging_moss_tip", &[("dry_foliage", 1.0)]),
    ("pale_oak_leaves", &[("none", 1.0)]),
    // Water & Fluid
    ("water", &[("water", 1.0)]),
    ("flowing_water", &[("water", 1.0)]),
    ("water_still", &[("water", 1.0)]),
    ("water_flow", &[("water", 1.0)]),
    ("water_cauldron", &[("water", 1.0)]),
    ("bubble_column", &[("water", 1.0)]),
    // Hardcoded tints
    ("spruce_leaves", &[("hardcoded", 1.0)]),
    ("birch_leaves", &[("hardcoded", 1.0)]),
    ("lily_pad", &[("hardcoded", 1.0)]),
    ("attached_melon_stem", &[("hardcoded", 1.0)]),
    ("attached_pumpkin_stem", &[("hardcoded", 1.0)]),
    ("melon_stem", &[("hardcoded", 1.0)]),
    ("pumpkin_stem", &[("hardcoded", 1.0)]),
    ("redstone_wire", &[("hardcoded", 1.0)]),
];

/// Explicit list of vanilla blocks and textures that must NEVER receive biome tint.
/// Dead bush is dry dead wood and must retain its natural brown texture.
/// Azalea, cherry, and pale oak leaves have custom pre-colored textures in vanilla.
/// Dirt, seagrass, and kelp are also never tinted.
pub static EXPLICIT_NONE_BLOCKS: &[&str] = &[
    "dead_bush",
    "potted_dead_bush",
    "firefly_bush",
    "azalea_leaves",
    "flowering_azalea_leaves",
    "potted_azalea_bush_plant",
    "potted_flowering_azalea_bush_plant",
    "potted_azalea_bush_side",
    "potted_azalea_bush_top",
    "cherry_leaves",
    "pale_oak_leaves",
    "yellow_poplar_leaves",
    "red_poplar_leaves",
    "orange_poplar_leaves",
    "poplar_leaves",
    "leaves_notint",
    "seagrass",
    "tall_seagrass",
    "kelp",
    "kelp_plant",
    "dirt",
    "coarse_dirt",
    "rooted_dirt",
    "redstone_dust_overlay",
];

/// Known vanilla texture stems with a grass biome colour provider.
pub(crate) static KNOWN_GRASS_STEMS: &[&str] = &[
    "grass_block_top",
    "grass_block_side_overlay",
    "grass_side_overlay",
    "short_grass",
    "grass",
    "tall_grass_top",
    "tall_grass_bottom",
    "tall_grass",
    "fern",
    "large_fern_top",
    "large_fern_bottom",
    "large_fern",
    "sugar_cane",
    "potted_fern",
    "bush",
    "pink_petals_stem",
    "wildflowers_stem",
    "bamboo_large_leaves",
    "bamboo_small_leaves",
    "bamboo_stage0",
];

/// Known vanilla texture stems with a foliage biome colour provider.
pub static KNOWN_FOLIAGE_STEMS: &[&str] = &[
    "oak_leaves",
    "jungle_leaves",
    "acacia_leaves",
    "dark_oak_leaves",
    "vine",
    "mangrove_leaves",
];

/// Known vanilla texture stems with a dry foliage biome colour provider.
pub static KNOWN_DRY_FOLIAGE_STEMS: &[&str] = &[
    "leaf_litter",
    "pale_hanging_moss",
    "pale_hanging_moss_tip",
    "short_dry_grass",
    "tall_dry_grass",
    "dry_grass",
];

/// Known vanilla texture stems with a water biome colour provider.
pub static KNOWN_WATER_STEMS: &[&str] = &[
    "water",
    "water_still",
    "water_flow",
    "water_overlay",
    "water_cauldron",
    "bubble_column",
];

/// Returns true if the stem belongs to an explicit non-tinted block or texture.
pub fn is_explicit_none_tint(stem: &str) -> bool {
    let clean = stem.trim().to_ascii_lowercase();
    let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
    let s = unname.strip_prefix("block/").unwrap_or(unname);

    if EXPLICIT_NONE_BLOCKS
        .iter()
        .any(|&b| s == b || s.starts_with(b))
    {
        return true;
    }
    s.contains("dead_bush") || s.contains("firefly_bush")
}

/// Returns true if the leaf or block represents non-tinted foliage:
/// - Explicitly non-tinted vanilla trees: azalea, cherry, pale oak
/// - Poplar and un-tinted variants
/// - Snow-covered / frosty leaves: snow, frost, winter, ice, icy
/// - Pre-colored autumn / seasonal leaves: red, orange, yellow, golden, dead, autumn, purple, blue, pink, white, brown, amber, maple, zelkova
/// - Flowering / blossom leaves: flower, blossom
/// - Fruit-bearing leaves: apple, berry, cherries, fruit
/// - Fantasy / crystal dimension leaves: crystal, dusk, lunar, starlit, bluebright, comet, shimmering, glacia, etc.
/// - Nether / End leaves: nether, warped, crimson, dragon_tree, shadow_berry, tenanea, lacugrove
pub fn is_explicit_non_tinted_leaf(s_stem: &str, b_norm: Option<&str>) -> bool {
    let check = |s: &str| {
        s.contains("cherry")
            || s.contains("azalea")
            || s.contains("pale_oak")
            || s.contains("poplar")
            || s.contains("notint")
            || s.contains("snow")
            || s.contains("frost")
            || s.contains("winter")
            || s.contains("ice")
            || s.contains("icy")
            || s.contains("red_")
            || s.contains("orange_")
            || s.contains("yellow_")
            || s.contains("golden_")
            || s.contains("dead_")
            || s.contains("autumn")
            || s.contains("purple_")
            || s.contains("blue_")
            || s.contains("pink_")
            || s.contains("white_")
            || s.contains("brown_")
            || s.contains("amber_")
            || s.contains("flower")
            || s.contains("blossom")
            || s.contains("apple_")
            || s.contains("berry")
            || s.contains("cherries")
            || s.contains("fruit")
            || s.contains("crystal")
            || s.contains("dusk")
            || s.contains("lunar")
            || s.contains("starlit")
            || s.contains("bluebright")
            || s.contains("comet")
            || s.contains("shimmering")
            || s.contains("dragon_tree")
            || s.contains("shadow_berry")
            || s.contains("tenanea")
            || s.contains("lacugrove")
            || s.contains("glacia")
            || s.contains("maple")
            || s.contains("zelkova")
            || s.contains("nether")
            || s.contains("warped")
            || s.contains("crimson")
    };
    check(s_stem) || b_norm.is_some_and(check)
}

/// Returns true if the stem represents dry foliage (leaf litter, dry grass, etc.).
pub fn is_dry_foliage(stem: &str) -> bool {
    let clean = stem.trim().to_ascii_lowercase();
    let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
    let s = unname.strip_prefix("block/").unwrap_or(unname);

    s.contains("leaf_litter")
        || s.contains("dry_foliage")
        || s.contains("dry_grass")
        || s.contains("short_dry_grass")
        || s.contains("tall_dry_grass")
        || s.contains("pale_hanging_moss")
        || s.starts_with("dry_")
}

/// Returns true if the stem represents water.
pub fn is_water(stem: &str) -> bool {
    let clean = stem.trim().to_ascii_lowercase();
    let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
    let s = unname.strip_prefix("block/").unwrap_or(unname);

    s == "water"
        || s.contains("water_still")
        || s.contains("water_flow")
        || s.contains("water_cauldron")
        || s.contains("bubble_column")
}

/// Retrieve the hardcoded hex string for a block or texture stem, if any.
pub(crate) fn get_hardcoded_tint_hex(name: &str) -> Option<&'static str> {
    let clean = name.trim().to_ascii_lowercase();
    let unnamespaced = clean.strip_prefix("minecraft:").unwrap_or(&clean);
    let stem = unnamespaced.strip_prefix("block/").unwrap_or(unnamespaced);

    for &(k, hex) in HARDCODED_BLOCK_TINTS {
        if k == stem {
            return Some(hex);
        }
    }
    None
}

/// Classify a texture stem and/or block name into an authoritative tint category:
/// "grass", "foliage", "dry_foliage", "water", "hardcoded", or "none".
pub(crate) fn classify_tint_category(
    clean_stem: &str,
    block_name: Option<&str>,
    tint_index: Option<i32>,
) -> &'static str {
    // 0. Negative tint_index is strictly untinted in Minecraft specification
    if let Some(ti) = tint_index {
        if ti < 0 {
            return "none";
        }
    }

    let stem = clean_stem.trim().to_ascii_lowercase();
    let unname_stem = stem.strip_prefix("minecraft:").unwrap_or(&stem);
    let s_stem = unname_stem.strip_prefix("block/").unwrap_or(unname_stem);

    let b_norm = block_name.map(|b| {
        let clean = b.trim().to_ascii_lowercase();
        let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
        let unblock = unname.strip_prefix("block/").unwrap_or(unname);
        let unmodels = unblock.strip_prefix("models/block/").unwrap_or(unblock);
        let s = unmodels.strip_prefix("models/").unwrap_or(unmodels);
        if let Some((base, _)) = s.split_once('[') {
            base.to_string()
        } else {
            s.to_string()
        }
    });

    if s_stem.is_empty() && b_norm.is_none() {
        return "none";
    }

    // 1. Explicitly untinted check
    if is_explicit_none_tint(s_stem) || b_norm.as_deref().is_some_and(is_explicit_none_tint) {
        return "none";
    }

    // 2. Block-level registry lookup
    let target_b = b_norm.as_deref().or(Some(s_stem));
    if let Some(b) = target_b {
        for &(reg_block, layers) in BLOCK_TINT_REGISTRY {
            if b == reg_block {
                if let Some(ti) = tint_index {
                    let idx = ti as usize;
                    if idx < layers.len() {
                        return layers[idx].0;
                    } else if !layers.is_empty() {
                        return layers.last().unwrap().0;
                    }
                } else {
                    for &(cat, _) in layers {
                        if cat != "none" {
                            return cat;
                        }
                    }
                }
            }
        }
    }

    // 3. Hardcoded block/texture tints
    if get_hardcoded_tint_hex(s_stem).is_some()
        || b_norm.as_deref().and_then(get_hardcoded_tint_hex).is_some()
    {
        return "hardcoded";
    }

    // 4. Canonical known stems
    if KNOWN_DRY_FOLIAGE_STEMS.contains(&s_stem) {
        return "dry_foliage";
    }
    if KNOWN_GRASS_STEMS.contains(&s_stem) {
        return "grass";
    }
    if KNOWN_FOLIAGE_STEMS.contains(&s_stem) {
        return "foliage";
    }
    if KNOWN_WATER_STEMS.contains(&s_stem) {
        return "water";
    }

    // 5. Heuristic fallback for leaves
    if s_stem.contains("leaves") || b_norm.as_deref().is_some_and(|b| b.contains("leaves")) {
        if s_stem.contains("spruce") || b_norm.as_deref().is_some_and(|b| b.contains("spruce")) {
            return "hardcoded";
        }
        if s_stem.contains("birch") || b_norm.as_deref().is_some_and(|b| b.contains("birch")) {
            return "hardcoded";
        }
        if is_explicit_non_tinted_leaf(s_stem, b_norm.as_deref()) {
            return "none";
        }
        return "foliage";
    }

    // 6. Suffix and keyword heuristics
    if s_stem.ends_with("_grass")
        || s_stem.ends_with("_fern")
        || s_stem.ends_with("_vine")
        || s_stem == "bush"
        || s_stem == "potted_bush"
    {
        return "grass";
    }
    if s_stem.contains("vine") {
        return "foliage";
    }
    if is_dry_foliage(s_stem) || b_norm.as_deref().is_some_and(is_dry_foliage) {
        return "dry_foliage";
    }
    if is_water(s_stem) || b_norm.as_deref().is_some_and(is_water) {
        return "water";
    }

    "none"
}

/// Precompute the six per-direction unit-cube tint indices in a single
/// classification pass (indexed by `Direction::to_index`).
pub fn get_unit_cube_tint_indices(clean_block: &str) -> [i16; 6] {
    let clean = clean_block.trim().to_ascii_lowercase();
    let unname = clean.strip_prefix("minecraft:").unwrap_or(&clean);
    let s = unname.strip_prefix("block/").unwrap_or(unname);
    let base = s.split_once('[').map(|(b, _)| b).unwrap_or(s);

    if base == "grass_block" || base == "grass" {
        let mut out = [-1i16; 6];
        out[Direction::Up.to_index()] = 0;
        out
    } else if classify_tint_category("", Some(base), Some(0)) != "none" {
        [0i16; 6]
    } else {
        [-1i16; 6]
    }
}

#[cfg(test)]
#[path = "hardcoded_tests.rs"]
mod tests;
