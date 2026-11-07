//! # Minecraft Block Names & Category Catalogs
//!
//! Provides static arrays and classification predicates for vanilla block categories.

/// Canonical vanilla technical / invisible blocks that never emit static solid polygon models.
pub fn is_non_mesh_placeholder_block(name: &str) -> bool {
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);
    let base = clean.find('[').map_or(clean, |idx| &clean[..idx]);
    let lower = base.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "air" | "cave_air" | "void_air" | "structure_void" | "light" | "barrier" | "bubble_column"
    )
}

/// Known block names for air categories.
pub const AIR_NAMES: &[&str] = &[
    "air",
    "minecraft:air",
    "cave_air",
    "minecraft:cave_air",
    "void_air",
    "minecraft:void_air",
    "structure_void",
    "minecraft:structure_void",
];

/// Known block names for fluid categories.
pub const FLUID_NAMES: &[&str] = &[
    "water",
    "minecraft:water",
    "flowing_water",
    "minecraft:flowing_water",
    "lava",
    "minecraft:lava",
    "flowing_lava",
    "minecraft:flowing_lava",
];

/// Known block names for leaves categories.
pub const LEAVES_NAMES: &[&str] = &[
    "oak_leaves",
    "minecraft:oak_leaves",
    "spruce_leaves",
    "minecraft:spruce_leaves",
    "birch_leaves",
    "minecraft:birch_leaves",
    "jungle_leaves",
    "minecraft:jungle_leaves",
    "acacia_leaves",
    "minecraft:acacia_leaves",
    "dark_oak_leaves",
    "minecraft:dark_oak_leaves",
    "mangrove_leaves",
    "minecraft:mangrove_leaves",
    "cherry_leaves",
    "minecraft:cherry_leaves",
    "azalea_leaves",
    "minecraft:azalea_leaves",
    "flowering_azalea_leaves",
    "minecraft:flowering_azalea_leaves",
    "pale_oak_leaves",
    "minecraft:pale_oak_leaves",
];

/// Known block names for glass categories.
pub const GLASS_NAMES: &[&str] = &[
    "glass",
    "minecraft:glass",
    "tinted_glass",
    "minecraft:tinted_glass",
    "white_stained_glass",
    "minecraft:white_stained_glass",
    "orange_stained_glass",
    "minecraft:orange_stained_glass",
    "magenta_stained_glass",
    "minecraft:magenta_stained_glass",
    "light_blue_stained_glass",
    "minecraft:light_blue_stained_glass",
    "yellow_stained_glass",
    "minecraft:yellow_stained_glass",
    "lime_stained_glass",
    "minecraft:lime_stained_glass",
    "pink_stained_glass",
    "minecraft:pink_stained_glass",
    "gray_stained_glass",
    "minecraft:gray_stained_glass",
    "light_gray_stained_glass",
    "minecraft:light_gray_stained_glass",
    "cyan_stained_glass",
    "minecraft:cyan_stained_glass",
    "purple_stained_glass",
    "minecraft:purple_stained_glass",
    "blue_stained_glass",
    "minecraft:blue_stained_glass",
    "brown_stained_glass",
    "minecraft:brown_stained_glass",
    "green_stained_glass",
    "minecraft:green_stained_glass",
    "red_stained_glass",
    "minecraft:red_stained_glass",
    "black_stained_glass",
    "minecraft:black_stained_glass",
    "ice",
    "minecraft:ice",
    "packed_ice",
    "minecraft:packed_ice",
    "blue_ice",
    "minecraft:blue_ice",
    "frosted_ice",
    "minecraft:frosted_ice",
    "slime_block",
    "minecraft:slime_block",
    "honey_block",
    "minecraft:honey_block",
    "powder_snow",
    "minecraft:powder_snow",
];

/// Known block names for non-occluding categories.
pub const NON_OCCLUDING_NAMES: &[&str] = &[
    "torch",
    "wall_torch",
    "soul_torch",
    "soul_wall_torch",
    "redstone_torch",
    "redstone_wall_torch",
    "lantern",
    "soul_lantern",
    "short_grass",
    "tall_grass",
    "fern",
    "large_fern",
    "dandelion",
    "poppy",
    "blue_orchid",
    "allium",
    "azure_bluet",
    "red_tulip",
    "orange_tulip",
    "white_tulip",
    "pink_tulip",
    "oxeye_daisy",
    "cornflower",
    "lily_of_the_valley",
    "wither_rose",
    "sunflower",
    "lilac",
    "rose_bush",
    "peony",
    "dead_bush",
    "sapling",
    "wheat",
    "carrots",
    "potatoes",
    "beetroots",
    "sweet_berry_bush",
    "ladder",
    "lever",
    "tripwire_hook",
    "tripwire",
    "vine",
    "scaffolding",
    "barrier",
    "light",
    "structure_void",
    "bubble_column",
    "kelp",
    "kelp_plant",
    "seagrass",
    "tall_seagrass",
    "sea_pickle",
    "sugar_cane",
    "bamboo",
    "bamboo_sapling",
    "cactus",
    "nether_wart",
    "crimson_roots",
    "warped_roots",
    "hanging_roots",
    "nether_sprouts",
    "spore_blossom",
    "small_dripleaf",
    "big_dripleaf",
    "big_dripleaf_stem",
    "lily_pad",
    "pink_petals",
    "wildflowers",
    "leaf_litter",
    "torchflower",
    "torchflower_crop",
    "pitcher_plant",
    "pitcher_crop",
    "cave_vines",
    "cave_vines_plant",
    "twisting_vines",
    "twisting_vines_plant",
    "weeping_vines",
    "weeping_vines_plant",
    "glow_lichen",
    "sculk_vein",
    "frogspawn",
    "turtle_egg",
    "sniffer_egg",
    "cobweb",
    "copper_statue",
    "copper_golem_statue",
];

/// Check if block identifier is non-occluding vegetation, decoration, or plant.
pub fn is_non_occluding_block(name_low: &str) -> bool {
    if NON_OCCLUDING_NAMES.contains(&name_low) {
        return true;
    }
    if name_low.contains("statue") {
        return true;
    }
    if name_low.ends_with("_flower")
        || name_low.ends_with("_sapling")
        || name_low.ends_with("_torch")
        || name_low.ends_with("_lantern")
        || name_low.ends_with("_plant")
        || name_low.ends_with("_bush")
        || (name_low.ends_with("_roots") && name_low != "mangrove_roots")
        || name_low.ends_with("_vines")
        || name_low.ends_with("_sprouts")
        || name_low.ends_with("_petals")
        || name_low.ends_with("_lichen")
        || name_low.ends_with("_crop")
        || name_low.ends_with("_egg")
    {
        return true;
    }
    if name_low.contains("grass") && !name_low.contains("grass_block") {
        return true;
    }
    if name_low.contains("fern")
        || name_low.contains("dripleaf")
        || name_low.contains("kelp")
        || name_low.contains("seagrass")
        || name_low.contains("litter")
        || name_low == "sugar_cane"
        || (name_low.starts_with("bamboo")
            && !name_low.contains("block")
            && !name_low.contains("planks"))
        || name_low == "lily_pad"
        || name_low == "spore_blossom"
        || name_low == "sea_pickle"
        || name_low == "cobweb"
        || (name_low.contains("coral") && !name_low.contains("block"))
    {
        return true;
    }
    false
}

/// Suffixes identifying partial non-full blocks.
pub const PARTIAL_SHAPE_SUFFIXES: &[&str] = &[
    "_fence",
    "_fence_gate",
    "_wall",
    "_pane",
    "_bars",
    "_trapdoor",
    "_door",
    "_carpet",
    "_bed",
    "_sign",
    "_hanging_sign",
    "_head",
    "_skull",
    "_banner",
    "_candle",
    "_pot",
    "_rod",
    "_coral",
    "_fan",
    "_chain",
    "_path",
];

/// Exact names identifying partial non-full blocks.
pub const PARTIAL_SHAPE_EXACT_NAMES: &[&str] = &[
    "dirt_path",
    "farmland",
    "bed",
    "piston_head",
    "moving_piston",
    "iron_bars",
    "glass_pane",
    "chest",
    "trapped_chest",
    "ender_chest",
    "bell",
    "anvil",
    "chipped_anvil",
    "damaged_anvil",
    "cauldron",
    "water_cauldron",
    "lava_cauldron",
    "powder_snow_cauldron",
    "hopper",
    "brewing_stand",
    "flower_pot",
    "conduit",
    "beacon",
    "decorated_pot",
    "end_portal_frame",
    "end_portal",
    "end_gateway",
    "chain",
    "iron_chain",
    "copper_chain",
    "exposed_copper_chain",
    "weathered_copper_chain",
    "oxidized_copper_chain",
    "lever",
    "tripwire_hook",
    "tripwire",
    "repeater",
    "comparator",
    "daylight_detector",
    "lightning_rod",
    "end_rod",
    "dragon_egg",
    "scaffolding",
    "pointed_dripstone",
    "amethyst_cluster",
    "small_amethyst_bud",
    "medium_amethyst_bud",
    "large_amethyst_bud",
    "calibrated_sculk_sensor",
    "sculk_sensor",
    "sculk_shrieker",
    "sculk_vein",
    "snow",
    "ladder",
    "grindstone",
    "stonecutter",
    "lectern",
    "sniffer_egg",
];

/// Check if block identifier is a non-full or partial block.
pub(crate) fn is_non_full_or_partial_block(name_low: &str) -> bool {
    if PARTIAL_SHAPE_EXACT_NAMES.contains(&name_low) || is_non_occluding_block(name_low) {
        return true;
    }
    if PARTIAL_SHAPE_SUFFIXES
        .iter()
        .any(|&s| name_low.ends_with(s))
    {
        return true;
    }
    const KEYWORDS: &[&str] = &[
        "flower", "sapling", "torch", "lantern", "pane", "fence", "wall", "carpet", "trapdoor",
        "door",
    ];
    if KEYWORDS.iter().any(|&kw| name_low.contains(kw)) {
        return true;
    }
    false
}
