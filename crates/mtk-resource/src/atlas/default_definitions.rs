use std::collections::HashMap;

use super::category::AtlasCategory;
use super::definition::{AtlasDefinition, AtlasSource};
use crate::identifier::ResourceLocation;

impl AtlasCategory {
    /// Generates the standard default AtlasDefinition for this category if no atlases/*.json exists.
    pub fn default_definition(&self) -> AtlasDefinition {
        match self {
            AtlasCategory::Blocks => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "block".to_string(),
                        prefix: "block/".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "entity/conduit".to_string(),
                        prefix: "entity/conduit/".to_string(),
                    },
                    AtlasSource::Single {
                        resource: ResourceLocation::vanilla("entity/bell/bell_body"),
                        sprite: None,
                    },
                    AtlasSource::Single {
                        resource: ResourceLocation::vanilla(
                            "entity/enchantment/enchanting_table_book",
                        ),
                        sprite: None,
                    },
                    AtlasSource::Single {
                        resource: ResourceLocation::vanilla("entity/end_portal/end_portal"),
                        sprite: None,
                    },
                ],
            },
            AtlasCategory::Items => {
                let mut perms = HashMap::new();
                for (name, path) in [
                    ("amethyst", "minecraft:trims/color_palettes/amethyst"),
                    ("copper", "minecraft:trims/color_palettes/copper"),
                    (
                        "copper_darker",
                        "minecraft:trims/color_palettes/copper_darker",
                    ),
                    ("diamond", "minecraft:trims/color_palettes/diamond"),
                    (
                        "diamond_darker",
                        "minecraft:trims/color_palettes/diamond_darker",
                    ),
                    ("emerald", "minecraft:trims/color_palettes/emerald"),
                    ("gold", "minecraft:trims/color_palettes/gold"),
                    ("gold_darker", "minecraft:trims/color_palettes/gold_darker"),
                    ("iron", "minecraft:trims/color_palettes/iron"),
                    ("iron_darker", "minecraft:trims/color_palettes/iron_darker"),
                    ("lapis", "minecraft:trims/color_palettes/lapis"),
                    ("netherite", "minecraft:trims/color_palettes/netherite"),
                    (
                        "netherite_darker",
                        "minecraft:trims/color_palettes/netherite_darker",
                    ),
                    ("quartz", "minecraft:trims/color_palettes/quartz"),
                    ("redstone", "minecraft:trims/color_palettes/redstone"),
                    ("resin", "minecraft:trims/color_palettes/resin"),
                ] {
                    perms.insert(name.to_string(), ResourceLocation::parse(path).unwrap());
                }
                AtlasDefinition {
                    sources: vec![
                        AtlasSource::Directory {
                            source: "item".to_string(),
                            prefix: "item/".to_string(),
                        },
                        AtlasSource::PalettedPermutations {
                            palette_key: ResourceLocation::vanilla(
                                "trims/color_palettes/trim_palette",
                            ),
                            permutations: perms,
                            textures: vec![
                                ResourceLocation::vanilla("trims/items/helmet_trim"),
                                ResourceLocation::vanilla("trims/items/chestplate_trim"),
                                ResourceLocation::vanilla("trims/items/leggings_trim"),
                                ResourceLocation::vanilla("trims/items/boots_trim"),
                            ],
                        },
                    ],
                }
            }
            AtlasCategory::Particles => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "particle".to_string(),
                        prefix: "particle/".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "particle".to_string(),
                        prefix: "".to_string(),
                    },
                ],
            },
            AtlasCategory::Chests => AtlasDefinition {
                sources: vec![AtlasSource::Directory {
                    source: "entity/chest".to_string(),
                    prefix: "entity/chest/".to_string(),
                }],
            },
            AtlasCategory::ShulkerBoxes => AtlasDefinition {
                sources: vec![AtlasSource::Directory {
                    source: "entity/shulker".to_string(),
                    prefix: "entity/shulker/".to_string(),
                }],
            },
            AtlasCategory::BannerPatterns => AtlasDefinition {
                sources: vec![AtlasSource::Directory {
                    source: "entity/banner".to_string(),
                    prefix: "entity/banner/".to_string(),
                }],
            },
            AtlasCategory::ShieldPatterns => AtlasDefinition {
                sources: vec![AtlasSource::Directory {
                    source: "entity/shield".to_string(),
                    prefix: "entity/shield/".to_string(),
                }],
            },
            AtlasCategory::ArmorTrims => {
                let mut perms = HashMap::new();
                for (name, path) in [
                    ("amethyst", "minecraft:trims/color_palettes/amethyst"),
                    ("copper", "minecraft:trims/color_palettes/copper"),
                    (
                        "copper_darker",
                        "minecraft:trims/color_palettes/copper_darker",
                    ),
                    ("diamond", "minecraft:trims/color_palettes/diamond"),
                    (
                        "diamond_darker",
                        "minecraft:trims/color_palettes/diamond_darker",
                    ),
                    ("emerald", "minecraft:trims/color_palettes/emerald"),
                    ("gold", "minecraft:trims/color_palettes/gold"),
                    ("gold_darker", "minecraft:trims/color_palettes/gold_darker"),
                    ("iron", "minecraft:trims/color_palettes/iron"),
                    ("iron_darker", "minecraft:trims/color_palettes/iron_darker"),
                    ("lapis", "minecraft:trims/color_palettes/lapis"),
                    ("netherite", "minecraft:trims/color_palettes/netherite"),
                    (
                        "netherite_darker",
                        "minecraft:trims/color_palettes/netherite_darker",
                    ),
                    ("quartz", "minecraft:trims/color_palettes/quartz"),
                    ("redstone", "minecraft:trims/color_palettes/redstone"),
                    ("resin", "minecraft:trims/color_palettes/resin"),
                ] {
                    perms.insert(name.to_string(), ResourceLocation::parse(path).unwrap());
                }
                AtlasDefinition {
                    sources: vec![
                        AtlasSource::Directory {
                            source: "trims".to_string(),
                            prefix: "trims/".to_string(),
                        },
                        AtlasSource::PalettedPermutations {
                            palette_key: ResourceLocation::vanilla(
                                "trims/color_palettes/trim_palette",
                            ),
                            permutations: perms,
                            textures: vec![
                                ResourceLocation::vanilla("trims/entity/humanoid/sentry"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/sentry"),
                                ResourceLocation::vanilla("trims/entity/humanoid/dune"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/dune"),
                                ResourceLocation::vanilla("trims/entity/humanoid/coast"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/coast"),
                                ResourceLocation::vanilla("trims/entity/humanoid/wild"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/wild"),
                                ResourceLocation::vanilla("trims/entity/humanoid/ward"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/ward"),
                                ResourceLocation::vanilla("trims/entity/humanoid/eye"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/eye"),
                                ResourceLocation::vanilla("trims/entity/humanoid/vex"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/vex"),
                                ResourceLocation::vanilla("trims/entity/humanoid/tide"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/tide"),
                                ResourceLocation::vanilla("trims/entity/humanoid/snout"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/snout"),
                                ResourceLocation::vanilla("trims/entity/humanoid/rib"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/rib"),
                                ResourceLocation::vanilla("trims/entity/humanoid/spire"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/spire"),
                                ResourceLocation::vanilla("trims/entity/humanoid/wayfinder"),
                                ResourceLocation::vanilla(
                                    "trims/entity/humanoid_leggings/wayfinder",
                                ),
                                ResourceLocation::vanilla("trims/entity/humanoid/shaper"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/shaper"),
                                ResourceLocation::vanilla("trims/entity/humanoid/silence"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/silence"),
                                ResourceLocation::vanilla("trims/entity/humanoid/raiser"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/raiser"),
                                ResourceLocation::vanilla("trims/entity/humanoid/host"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/host"),
                                ResourceLocation::vanilla("trims/entity/humanoid/flow"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/flow"),
                                ResourceLocation::vanilla("trims/entity/humanoid/bolt"),
                                ResourceLocation::vanilla("trims/entity/humanoid_leggings/bolt"),
                            ],
                        },
                    ],
                }
            }
            AtlasCategory::DecoratedPot => AtlasDefinition {
                sources: vec![AtlasSource::Directory {
                    source: "entity/decorated_pot".to_string(),
                    prefix: "entity/decorated_pot/".to_string(),
                }],
            },
            AtlasCategory::Paintings => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "painting".to_string(),
                        prefix: "painting/".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "painting".to_string(),
                        prefix: "".to_string(),
                    },
                ],
            },
            AtlasCategory::Celestials => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "environment/celestial".to_string(),
                        prefix: "".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "environment".to_string(),
                        prefix: "environment/".to_string(),
                    },
                ],
            },
            AtlasCategory::Gui => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "gui/sprites".to_string(),
                        prefix: "".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "mob_effect".to_string(),
                        prefix: "mob_effect/".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "gui".to_string(),
                        prefix: "gui/".to_string(),
                    },
                ],
            },
            AtlasCategory::MapDecorations => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "map/decorations".to_string(),
                        prefix: "".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "map".to_string(),
                        prefix: "map/".to_string(),
                    },
                ],
            },
            AtlasCategory::Entities => AtlasDefinition {
                sources: vec![
                    AtlasSource::Directory {
                        source: "entity".to_string(),
                        prefix: "entity/".to_string(),
                    },
                    AtlasSource::Directory {
                        source: "models/armor".to_string(),
                        prefix: "models/armor/".to_string(),
                    },
                ],
            },
            AtlasCategory::Misc | AtlasCategory::Custom(_) => AtlasDefinition::default(),
        }
    }
}
