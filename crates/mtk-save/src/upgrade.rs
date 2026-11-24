//! DataVersion-driven upgrade rules for legacy Minecraft block states and biomes (1.13+).
//!
//! Translates historical block names, block properties, numeric biome IDs, and biome names
//! to modern canonical identifiers so modern resource packs and `mtk-material` palettes resolve
//! them uniformly without exposing version differences above `mtk-save`.

/// Maps a pre-1.18 numeric biome ID (`0..=255`) to its modern canonical biome identifier.
pub(crate) fn biome_name_from_legacy_id(id: i32) -> &'static str {
    match id & 0xFF {
        0 => "minecraft:ocean",
        1 => "minecraft:plains",
        2 | 17 | 130 => "minecraft:desert",
        3 | 20 => "minecraft:windswept_hills",
        4 | 18 => "minecraft:forest",
        5 | 19 | 133 => "minecraft:taiga",
        6 | 134 => "minecraft:swamp",
        7 => "minecraft:river",
        8 => "minecraft:nether_wastes",
        9 => "minecraft:the_end",
        10 => "minecraft:frozen_ocean",
        11 => "minecraft:frozen_river",
        12 | 13 | 129 => {
            if (id & 0xFF) == 129 {
                "minecraft:sunflower_plains"
            } else {
                "minecraft:snowy_plains"
            }
        }
        14 | 15 => "minecraft:mushroom_fields",
        16 => "minecraft:beach",
        21 | 22 | 149 => "minecraft:jungle",
        23 | 151 => "minecraft:sparse_jungle",
        24 => "minecraft:deep_ocean",
        25 => "minecraft:stony_shore",
        26 => "minecraft:snowy_beach",
        27 | 28 => "minecraft:birch_forest",
        29 | 157 => "minecraft:dark_forest",
        30 | 31 | 158 => "minecraft:snowy_taiga",
        32 | 33 => "minecraft:old_growth_pine_taiga",
        34 => "minecraft:windswept_forest",
        35 => "minecraft:savanna",
        36 => "minecraft:savanna_plateau",
        37 | 39 | 167 => "minecraft:badlands",
        38 | 166 => "minecraft:wooded_badlands",
        40 => "minecraft:small_end_islands",
        41 => "minecraft:end_midlands",
        42 => "minecraft:end_highlands",
        43 => "minecraft:end_barrens",
        44 | 47 => "minecraft:warm_ocean",
        45 => "minecraft:lukewarm_ocean",
        46 => "minecraft:cold_ocean",
        48 => "minecraft:deep_lukewarm_ocean",
        49 => "minecraft:deep_cold_ocean",
        50 => "minecraft:deep_frozen_ocean",
        127 => "minecraft:the_void",
        131 | 162 => "minecraft:windswept_gravelly_hills",
        132 => "minecraft:flower_forest",
        140 => "minecraft:ice_spikes",
        155 | 156 => "minecraft:old_growth_birch_forest",
        160 | 161 => "minecraft:old_growth_spruce_taiga",
        163 | 164 => "minecraft:windswept_savanna",
        165 => "minecraft:eroded_badlands",
        168 | 169 => "minecraft:bamboo_jungle",
        170 => "minecraft:soul_sand_valley",
        171 => "minecraft:crimson_forest",
        172 => "minecraft:warped_forest",
        173 => "minecraft:basalt_deltas",
        174 => "minecraft:dripstone_caves",
        175 => "minecraft:lush_caves",
        177 => "minecraft:meadow",
        178 => "minecraft:grove",
        179 => "minecraft:snowy_slopes",
        180 => "minecraft:frozen_peaks",
        181 => "minecraft:jagged_peaks",
        182 => "minecraft:stony_peaks",
        _ => "minecraft:plains",
    }
}

/// Ensures `id` has an explicit namespace (`minecraft:` if none is present).
fn ensure_namespaced(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.contains(':') {
        trimmed.to_string()
    } else {
        format!("minecraft:{trimmed}")
    }
}

/// Upgrades a biome identifier according to the chunk's `DataVersion`.
pub(crate) fn upgrade_biome_name(name: &str, data_version: i32) -> String {
    let mut current = ensure_namespaced(name);
    if data_version <= 0 {
        return current;
    }

    if data_version < 2552 && current == "minecraft:nether" {
        current = "minecraft:nether_wastes".to_string();
    }

    if data_version < 2838 {
        let renamed = match current.as_str() {
            "minecraft:badlands_plateau" => Some("minecraft:badlands"),
            "minecraft:bamboo_jungle_hills" => Some("minecraft:bamboo_jungle"),
            "minecraft:birch_forest_hills" => Some("minecraft:birch_forest"),
            "minecraft:dark_forest_hills" => Some("minecraft:dark_forest"),
            "minecraft:desert_hills" | "minecraft:desert_lakes" => Some("minecraft:desert"),
            "minecraft:giant_spruce_taiga_hills" | "minecraft:giant_spruce_taiga" => {
                Some("minecraft:old_growth_spruce_taiga")
            }
            "minecraft:giant_tree_taiga_hills" | "minecraft:giant_tree_taiga" => {
                Some("minecraft:old_growth_pine_taiga")
            }
            "minecraft:gravelly_mountains" | "minecraft:modified_gravelly_mountains" => {
                Some("minecraft:windswept_gravelly_hills")
            }
            "minecraft:jungle_edge" | "minecraft:modified_jungle_edge" => {
                Some("minecraft:sparse_jungle")
            }
            "minecraft:jungle_hills" | "minecraft:modified_jungle" => Some("minecraft:jungle"),
            "minecraft:modified_badlands_plateau" => Some("minecraft:badlands"),
            "minecraft:modified_wooded_badlands_plateau" => Some("minecraft:wooded_badlands"),
            "minecraft:mountain_edge" | "minecraft:mountains" => Some("minecraft:windswept_hills"),
            "minecraft:mushroom_field_shore" => Some("minecraft:mushroom_fields"),
            "minecraft:shattered_savanna" | "minecraft:shattered_savanna_plateau" => {
                Some("minecraft:windswept_savanna")
            }
            "minecraft:snowy_mountains" | "minecraft:snowy_tundra" => {
                Some("minecraft:snowy_plains")
            }
            "minecraft:snowy_taiga_hills" | "minecraft:snowy_taiga_mountains" => {
                Some("minecraft:snowy_taiga")
            }
            "minecraft:stone_shore" => Some("minecraft:stony_shore"),
            "minecraft:swamp_hills" => Some("minecraft:swamp"),
            "minecraft:taiga_hills" | "minecraft:taiga_mountains" => Some("minecraft:taiga"),
            "minecraft:tall_birch_forest" | "minecraft:tall_birch_hills" => {
                Some("minecraft:old_growth_birch_forest")
            }
            "minecraft:wooded_badlands_plateau" => Some("minecraft:wooded_badlands"),
            "minecraft:wooded_hills" => Some("minecraft:forest"),
            "minecraft:wooded_mountains" => Some("minecraft:windswept_forest"),
            "minecraft:lofty_peaks" => Some("minecraft:jagged_peaks"),
            "minecraft:snowcapped_peaks" => Some("minecraft:frozen_peaks"),
            _ => None,
        };
        if let Some(target) = renamed {
            current = target.to_string();
        }
    }

    if data_version < 2843 && current == "minecraft:deep_warm_ocean" {
        current = "minecraft:warm_ocean".to_string();
    }

    current
}

/// Upgrades a block name and its mutable `(key, value)` properties according to `data_version`.
pub(crate) fn upgrade_block_state(
    name: &str,
    props: &mut Vec<(String, String)>,
    data_version: i32,
) -> String {
    let norm = mtk_model::normalize_canonical_blockstate(name);
    if norm == "minecraft:air" {
        return "minecraft:air".to_string();
    }
    let mut current = ensure_namespaced(norm);
    if data_version <= 0 {
        return current;
    }

    if data_version < 1474 && current == "minecraft:purple_shulker_box" {
        current = "minecraft:shulker_box".to_string();
    }

    if data_version < 1475 {
        match current.as_str() {
            "minecraft:flowing_water" => current = "minecraft:water".to_string(),
            "minecraft:flowing_lava" => current = "minecraft:lava".to_string(),
            _ => {}
        }
    }

    if data_version < 1480 {
        if let Some(target) = rename_coral(current.as_str()) {
            current = target.to_string();
        }
    }

    if data_version < 1484 {
        match current.as_str() {
            "minecraft:sea_grass" => current = "minecraft:seagrass".to_string(),
            "minecraft:tall_sea_grass" => current = "minecraft:tall_seagrass".to_string(),
            _ => {}
        }
    }

    if data_version < 1487 {
        match current.as_str() {
            "minecraft:prismarine_bricks_slab" => {
                current = "minecraft:prismarine_brick_slab".to_string();
            }
            "minecraft:prismarine_bricks_stairs" => {
                current = "minecraft:prismarine_brick_stairs".to_string();
            }
            _ => {}
        }
    }

    if data_version < 1488 {
        match current.as_str() {
            "minecraft:kelp_top" => current = "minecraft:kelp".to_string(),
            "minecraft:kelp" => current = "minecraft:kelp_plant".to_string(),
            _ => {}
        }
    }

    if data_version < 1490 && current == "minecraft:melon_block" {
        current = "minecraft:melon".to_string();
    }

    if data_version < 1510 {
        if let Some(target) = rename_entity_renaming_block(current.as_str()) {
            current = target.to_string();
        }
    }

    if data_version < 1515 {
        if let Some(target) = match current.as_str() {
            "minecraft:tube_coral_fan" => Some("minecraft:tube_coral_wall_fan"),
            "minecraft:brain_coral_fan" => Some("minecraft:brain_coral_wall_fan"),
            "minecraft:bubble_coral_fan" => Some("minecraft:bubble_coral_wall_fan"),
            "minecraft:fire_coral_fan" => Some("minecraft:fire_coral_wall_fan"),
            "minecraft:horn_coral_fan" => Some("minecraft:horn_coral_wall_fan"),
            _ => None,
        } {
            current = target.to_string();
        }
    }

    if data_version < 1802 {
        match current.as_str() {
            "minecraft:stone_slab" => current = "minecraft:smooth_stone_slab".to_string(),
            "minecraft:sign" => current = "minecraft:oak_sign".to_string(),
            "minecraft:wall_sign" => current = "minecraft:oak_wall_sign".to_string(),
            _ => {}
        }
    }

    if data_version < 2209 && current == "minecraft:bee_hive" {
        current = "minecraft:beehive".to_string();
    }

    if data_version < 2503 && is_classic_wall_block(current.as_str()) {
        for (k, v) in props.iter_mut() {
            if matches!(k.as_str(), "north" | "south" | "east" | "west") {
                *v = if v == "true" {
                    "low".to_string()
                } else {
                    "none".to_string()
                };
            }
        }
    }

    if data_version < 2508 {
        match current.as_str() {
            "minecraft:warped_fungi" => current = "minecraft:warped_fungus".to_string(),
            "minecraft:crimson_fungi" => current = "minecraft:crimson_fungus".to_string(),
            _ => {}
        }
    }

    if data_version < 2528 {
        match current.as_str() {
            "minecraft:soul_fire_torch" => current = "minecraft:soul_torch".to_string(),
            "minecraft:soul_fire_wall_torch" => current = "minecraft:soul_wall_torch".to_string(),
            "minecraft:soul_fire_lantern" => current = "minecraft:soul_lantern".to_string(),
            _ => {}
        }
    }

    if data_version < 2679 && current == "minecraft:cauldron" {
        let level = props
            .iter()
            .find(|(k, _)| k == "level")
            .map(|(_, v)| v.as_str())
            .unwrap_or("0");
        if level == "0" {
            props.retain(|(k, _)| k != "level");
        } else {
            current = "minecraft:water_cauldron".to_string();
        }
    }

    if data_version < 2680 && current == "minecraft:grass_path" {
        current = "minecraft:dirt_path".to_string();
    }

    if data_version < 2690 {
        if let Some(target) = rename_copper_oxidized_terms(current.as_str()) {
            current = target.to_string();
        }
    }

    if data_version < 2691 {
        if let Some(target) = match current.as_str() {
            "minecraft:waxed_copper" => Some("minecraft:waxed_copper_block"),
            "minecraft:oxidized_copper_block" => Some("minecraft:oxidized_copper"),
            "minecraft:weathered_copper_block" => Some("minecraft:weathered_copper"),
            "minecraft:exposed_copper_block" => Some("minecraft:exposed_copper"),
            _ => None,
        } {
            current = target.to_string();
        }
    }

    if data_version < 2696 {
        if let Some(target) = rename_grimstone_to_deepslate(current.as_str()) {
            current = target.to_string();
        }
    }

    if data_version < 2700 {
        match current.as_str() {
            "minecraft:cave_vines_head" => current = "minecraft:cave_vines".to_string(),
            "minecraft:cave_vines_body" => current = "minecraft:cave_vines_plant".to_string(),
            _ => {}
        }
    }

    if data_version < 2717 && current == "minecraft:azalea_leaves_flowers" {
        current = "minecraft:flowering_azalea_leaves".to_string();
    }

    if data_version < 3692 && current == "minecraft:grass" {
        current = "minecraft:short_grass".to_string();
    }

    if data_version < 4541 && current == "minecraft:chain" {
        current = "minecraft:iron_chain".to_string();
    }

    current
}

fn rename_coral(id: &str) -> Option<&'static str> {
    Some(match id {
        "minecraft:blue_coral" => "minecraft:tube_coral_block",
        "minecraft:pink_coral" => "minecraft:brain_coral_block",
        "minecraft:purple_coral" => "minecraft:bubble_coral_block",
        "minecraft:red_coral" => "minecraft:fire_coral_block",
        "minecraft:yellow_coral" => "minecraft:horn_coral_block",
        "minecraft:blue_coral_plant" => "minecraft:tube_coral",
        "minecraft:pink_coral_plant" => "minecraft:brain_coral",
        "minecraft:purple_coral_plant" => "minecraft:bubble_coral",
        "minecraft:red_coral_plant" => "minecraft:fire_coral",
        "minecraft:yellow_coral_plant" => "minecraft:horn_coral",
        "minecraft:blue_coral_fan" => "minecraft:tube_coral_fan",
        "minecraft:pink_coral_fan" => "minecraft:brain_coral_fan",
        "minecraft:purple_coral_fan" => "minecraft:bubble_coral_fan",
        "minecraft:red_coral_fan" => "minecraft:fire_coral_fan",
        "minecraft:yellow_coral_fan" => "minecraft:horn_coral_fan",
        "minecraft:blue_dead_coral" => "minecraft:dead_tube_coral",
        "minecraft:pink_dead_coral" => "minecraft:dead_brain_coral",
        "minecraft:purple_dead_coral" => "minecraft:dead_bubble_coral",
        "minecraft:red_dead_coral" => "minecraft:dead_fire_coral",
        "minecraft:yellow_dead_coral" => "minecraft:dead_horn_coral",
        _ => return None,
    })
}

fn rename_entity_renaming_block(id: &str) -> Option<&'static str> {
    Some(match id {
        "minecraft:portal" => "minecraft:nether_portal",
        "minecraft:oak_bark" => "minecraft:oak_wood",
        "minecraft:spruce_bark" => "minecraft:spruce_wood",
        "minecraft:birch_bark" => "minecraft:birch_wood",
        "minecraft:jungle_bark" => "minecraft:jungle_wood",
        "minecraft:acacia_bark" => "minecraft:acacia_wood",
        "minecraft:dark_oak_bark" => "minecraft:dark_oak_wood",
        "minecraft:stripped_oak_bark" => "minecraft:stripped_oak_wood",
        "minecraft:stripped_spruce_bark" => "minecraft:stripped_spruce_wood",
        "minecraft:stripped_birch_bark" => "minecraft:stripped_birch_wood",
        "minecraft:stripped_jungle_bark" => "minecraft:stripped_jungle_wood",
        "minecraft:stripped_acacia_bark" => "minecraft:stripped_acacia_wood",
        "minecraft:stripped_dark_oak_bark" => "minecraft:stripped_dark_oak_wood",
        "minecraft:mob_spawner" => "minecraft:spawner",
        _ => return None,
    })
}

fn is_classic_wall_block(id: &str) -> bool {
    matches!(
        id,
        "minecraft:andesite_wall"
            | "minecraft:brick_wall"
            | "minecraft:cobblestone_wall"
            | "minecraft:diorite_wall"
            | "minecraft:end_stone_brick_wall"
            | "minecraft:granite_wall"
            | "minecraft:mossy_cobblestone_wall"
            | "minecraft:mossy_stone_brick_wall"
            | "minecraft:nether_brick_wall"
            | "minecraft:prismarine_wall"
            | "minecraft:red_nether_brick_wall"
            | "minecraft:red_sandstone_wall"
            | "minecraft:sandstone_wall"
            | "minecraft:stone_brick_wall"
    )
}

fn rename_copper_oxidized_terms(id: &str) -> Option<&'static str> {
    Some(match id {
        "minecraft:weathered_copper_block" => "minecraft:oxidized_copper_block",
        "minecraft:semi_weathered_copper_block" => "minecraft:weathered_copper_block",
        "minecraft:lightly_weathered_copper_block" => "minecraft:exposed_copper_block",
        "minecraft:weathered_cut_copper" => "minecraft:oxidized_cut_copper",
        "minecraft:semi_weathered_cut_copper" => "minecraft:weathered_cut_copper",
        "minecraft:lightly_weathered_cut_copper" => "minecraft:exposed_cut_copper",
        "minecraft:weathered_cut_copper_stairs" => "minecraft:oxidized_cut_copper_stairs",
        "minecraft:semi_weathered_cut_copper_stairs" => "minecraft:weathered_cut_copper_stairs",
        "minecraft:lightly_weathered_cut_copper_stairs" => "minecraft:exposed_cut_copper_stairs",
        "minecraft:weathered_cut_copper_slab" => "minecraft:oxidized_cut_copper_slab",
        "minecraft:semi_weathered_cut_copper_slab" => "minecraft:weathered_cut_copper_slab",
        "minecraft:lightly_weathered_cut_copper_slab" => "minecraft:exposed_cut_copper_slab",
        "minecraft:waxed_semi_weathered_copper" => "minecraft:waxed_weathered_copper",
        "minecraft:waxed_lightly_weathered_copper" => "minecraft:waxed_exposed_copper",
        "minecraft:waxed_semi_weathered_cut_copper" => "minecraft:waxed_weathered_cut_copper",
        "minecraft:waxed_lightly_weathered_cut_copper" => "minecraft:waxed_exposed_cut_copper",
        "minecraft:waxed_semi_weathered_cut_copper_stairs" => {
            "minecraft:waxed_weathered_cut_copper_stairs"
        }
        "minecraft:waxed_lightly_weathered_cut_copper_stairs" => {
            "minecraft:waxed_exposed_cut_copper_stairs"
        }
        "minecraft:waxed_semi_weathered_cut_copper_slab" => {
            "minecraft:waxed_weathered_cut_copper_slab"
        }
        "minecraft:waxed_lightly_weathered_cut_copper_slab" => {
            "minecraft:waxed_exposed_cut_copper_slab"
        }
        _ => return None,
    })
}

fn rename_grimstone_to_deepslate(id: &str) -> Option<&'static str> {
    Some(match id {
        "minecraft:grimstone" => "minecraft:deepslate",
        "minecraft:grimstone_slab" => "minecraft:cobbled_deepslate_slab",
        "minecraft:grimstone_stairs" => "minecraft:cobbled_deepslate_stairs",
        "minecraft:grimstone_wall" => "minecraft:cobbled_deepslate_wall",
        "minecraft:polished_grimstone" => "minecraft:polished_deepslate",
        "minecraft:polished_grimstone_slab" => "minecraft:polished_deepslate_slab",
        "minecraft:polished_grimstone_stairs" => "minecraft:polished_deepslate_stairs",
        "minecraft:polished_grimstone_wall" => "minecraft:polished_deepslate_wall",
        "minecraft:grimstone_tiles" => "minecraft:deepslate_tiles",
        "minecraft:grimstone_tile_slab" => "minecraft:deepslate_tile_slab",
        "minecraft:grimstone_tile_stairs" => "minecraft:deepslate_tile_stairs",
        "minecraft:grimstone_tile_wall" => "minecraft:deepslate_tile_wall",
        "minecraft:grimstone_bricks" => "minecraft:deepslate_bricks",
        "minecraft:grimstone_brick_slab" => "minecraft:deepslate_brick_slab",
        "minecraft:grimstone_brick_stairs" => "minecraft:deepslate_brick_stairs",
        "minecraft:grimstone_brick_wall" => "minecraft:deepslate_brick_wall",
        "minecraft:chiseled_grimstone" => "minecraft:chiseled_deepslate",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_biome_ids_map_to_modern_names() {
        assert_eq!(biome_name_from_legacy_id(0), "minecraft:ocean");
        assert_eq!(biome_name_from_legacy_id(1), "minecraft:plains");
        assert_eq!(biome_name_from_legacy_id(3), "minecraft:windswept_hills");
        assert_eq!(biome_name_from_legacy_id(8), "minecraft:nether_wastes");
        assert_eq!(biome_name_from_legacy_id(9), "minecraft:the_end");
        assert_eq!(biome_name_from_legacy_id(12), "minecraft:snowy_plains");
        assert_eq!(biome_name_from_legacy_id(47), "minecraft:warm_ocean");
        assert_eq!(biome_name_from_legacy_id(170), "minecraft:soul_sand_valley");
        assert_eq!(biome_name_from_legacy_id(171), "minecraft:crimson_forest");
        assert_eq!(biome_name_from_legacy_id(172), "minecraft:warped_forest");
        assert_eq!(biome_name_from_legacy_id(173), "minecraft:basalt_deltas");
        assert_eq!(biome_name_from_legacy_id(175), "minecraft:lush_caves");
        assert_eq!(biome_name_from_legacy_id(180), "minecraft:frozen_peaks");
        assert_eq!(biome_name_from_legacy_id(181), "minecraft:jagged_peaks");
        assert_eq!(biome_name_from_legacy_id(250), "minecraft:plains");
    }

    #[test]
    fn test_upgrade_biome_names_by_data_version() {
        assert_eq!(
            upgrade_biome_name("nether", 2528),
            "minecraft:nether_wastes"
        );
        assert_eq!(
            upgrade_biome_name("minecraft:snowy_tundra", 2724),
            "minecraft:snowy_plains"
        );
        assert_eq!(
            upgrade_biome_name("minecraft:deep_warm_ocean", 2724),
            "minecraft:warm_ocean"
        );
        assert_eq!(
            upgrade_biome_name("minecraft:deep_warm_ocean", 2843),
            "minecraft:deep_warm_ocean"
        );
    }

    #[test]
    fn test_upgrade_block_states_and_chained_renames() {
        let mut props = Vec::new();
        assert_eq!(
            upgrade_block_state("minecraft:grass", &mut props, 2724),
            "minecraft:short_grass"
        );
        assert_eq!(
            upgrade_block_state("minecraft:chain", &mut props, 2724),
            "minecraft:iron_chain"
        );
        assert_eq!(
            upgrade_block_state("minecraft:sign", &mut props, 1631),
            "minecraft:oak_sign"
        );
        // Chained coral fix: <1480 blue_coral_fan -> tube_coral_fan -> <1515 tube_coral_wall_fan
        assert_eq!(
            upgrade_block_state("minecraft:blue_coral_fan", &mut props, 1479),
            "minecraft:tube_coral_wall_fan"
        );
        // Chained copper fix: <2690 weathered_copper_block -> oxidized_copper_block -> <2691 oxidized_copper
        assert_eq!(
            upgrade_block_state("minecraft:weathered_copper_block", &mut props, 2685),
            "minecraft:oxidized_copper"
        );
    }

    #[test]
    fn test_wall_and_cauldron_upgrades() {
        let mut wall_props = vec![
            ("north".to_string(), "true".to_string()),
            ("south".to_string(), "false".to_string()),
            ("up".to_string(), "true".to_string()),
        ];
        assert_eq!(
            upgrade_block_state("minecraft:cobblestone_wall", &mut wall_props, 2230),
            "minecraft:cobblestone_wall"
        );
        assert_eq!(
            wall_props,
            vec![
                ("north".to_string(), "low".to_string()),
                ("south".to_string(), "none".to_string()),
                ("up".to_string(), "true".to_string()),
            ]
        );

        let mut empty_cauldron = vec![("level".to_string(), "0".to_string())];
        assert_eq!(
            upgrade_block_state("minecraft:cauldron", &mut empty_cauldron, 2586),
            "minecraft:cauldron"
        );
        assert!(empty_cauldron.is_empty());

        let mut full_cauldron = vec![("level".to_string(), "3".to_string())];
        assert_eq!(
            upgrade_block_state("minecraft:cauldron", &mut full_cauldron, 2586),
            "minecraft:water_cauldron"
        );
        assert_eq!(full_cauldron, vec![("level".to_string(), "3".to_string())]);
    }
}
