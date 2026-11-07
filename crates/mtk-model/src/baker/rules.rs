//! Block opacity, cube classification, and heuristics for model baking.

pub const KNOWN_NON_CUBES: &[&str] = &[
    "glass_pane",
    "pane",
    "fence",
    "door",
    "trapdoor",
    "bars",
    "chain",
    "lantern",
    "stairs",
    "slab",
    "chest",
    "banner",
    "bed",
    "carpet",
    "pot",
    "sign",
    "hanging_sign",
    "head",
    "skull",
    "rod",
    "hook",
    "lever",
    "rail",
    "torch",
    "candle",
    "flower",
    "plant",
    "sapling",
    "vine",
    "bush",
    "wire",
    "repeater",
    "comparator",
    "cauldron",
    "hopper",
    "bell",
    "anvil",
    "stand",
    "frame",
    "portal",
    "conduit",
    "grindstone",
    "stonecutter",
    "scaffolding",
    "dripstone",
    "amethyst",
    "sensor",
    "shrieker",
    "path",
    "farmland",
];

pub const NON_OPAQUE_SUBSTRINGS: &[&str] = &[
    "glass",
    "leaves",
    "ice",
    "water",
    "air",
    "path",
    "farmland",
    "pane",
    "fence",
    "door",
    "trapdoor",
    "bars",
    "chain",
    "lantern",
    "stairs",
    "slab",
    "chest",
    "banner",
    "bed",
    "carpet",
    "pot",
    "sign",
    "hanging_sign",
    "head",
    "skull",
    "rod",
    "hook",
    "lever",
    "rail",
    "torch",
    "candle",
    "flower",
    "plant",
    "sapling",
    "vine",
    "bush",
    "wire",
    "repeater",
    "comparator",
    "cauldron",
    "hopper",
    "bell",
    "anvil",
    "stand",
    "frame",
    "portal",
    "conduit",
    "grindstone",
    "cutter",
    "piston",
    "spawner",
    "vault",
    "grate",
];

/// Determines if a block is a known non-cube shape based on its canonical name.
#[inline]
pub fn is_known_non_cube(short_name: &str, is_double_slab: bool) -> bool {
    let is_bed = short_name == "bed" || short_name.ends_with("_bed");
    !is_double_slab
        && short_name != "bedrock"
        && (is_bed
            || KNOWN_NON_CUBES.iter().any(|&w| {
                if w == "bed" {
                    false
                } else {
                    short_name.contains(w)
                }
            }))
}

/// Determines if a block is opaque based on its canonical name.
#[inline]
pub fn is_opaque_block(short_name: &str) -> bool {
    let is_bed = short_name == "bed" || short_name.ends_with("_bed");
    short_name == "bedrock"
        || (!is_bed
            && !NON_OPAQUE_SUBSTRINGS.iter().any(|&w| {
                if w == "bed" {
                    false
                } else {
                    short_name.contains(w)
                }
            }))
}
