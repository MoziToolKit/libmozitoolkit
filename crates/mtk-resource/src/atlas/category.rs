use crate::identifier::ResourceLocation;
use serde::{Deserialize, Serialize};

/// Standardized Atlas categories corresponding to vanilla Minecraft atlases.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AtlasCategory {
    #[serde(rename = "blocks")]
    Blocks,
    #[serde(rename = "items")]
    Items,
    #[serde(rename = "particles")]
    Particles,
    #[serde(rename = "chests")]
    Chests,
    #[serde(rename = "shulker_boxes")]
    ShulkerBoxes,
    #[serde(rename = "banner_patterns")]
    BannerPatterns,
    #[serde(rename = "shield_patterns")]
    ShieldPatterns,
    #[serde(rename = "armor_trims")]
    ArmorTrims,
    #[serde(rename = "decorated_pot")]
    DecoratedPot,
    #[serde(rename = "paintings")]
    Paintings,
    #[serde(rename = "celestials")]
    Celestials,
    #[serde(rename = "gui")]
    Gui,
    #[serde(rename = "map_decorations")]
    MapDecorations,
    #[serde(rename = "entities")]
    Entities,
    #[serde(rename = "misc")]
    Misc,
    #[serde(untagged)]
    Custom(ResourceLocation),
}

impl AtlasCategory {
    pub const ALL_STANDARD: [AtlasCategory; 13] = [
        AtlasCategory::Blocks,
        AtlasCategory::Items,
        AtlasCategory::Particles,
        AtlasCategory::Chests,
        AtlasCategory::ShulkerBoxes,
        AtlasCategory::ShieldPatterns,
        AtlasCategory::ArmorTrims,
        AtlasCategory::DecoratedPot,
        AtlasCategory::Paintings,
        AtlasCategory::Celestials,
        AtlasCategory::Gui,
        AtlasCategory::MapDecorations,
        AtlasCategory::Entities,
    ];

    /// Canonical name string (e.g. `"blocks"`, `"items"`).
    pub fn as_str(&self) -> &str {
        match self {
            AtlasCategory::Blocks => "blocks",
            AtlasCategory::Items => "items",
            AtlasCategory::Particles => "particles",
            AtlasCategory::Chests => "chests",
            AtlasCategory::ShulkerBoxes => "shulker_boxes",
            AtlasCategory::BannerPatterns => "banner_patterns",
            AtlasCategory::ShieldPatterns => "shield_patterns",
            AtlasCategory::ArmorTrims => "armor_trims",
            AtlasCategory::DecoratedPot => "decorated_pot",
            AtlasCategory::Paintings => "paintings",
            AtlasCategory::Celestials => "celestials",
            AtlasCategory::Gui => "gui",
            AtlasCategory::MapDecorations => "map_decorations",
            AtlasCategory::Entities => "entities",
            AtlasCategory::Misc => "misc",
            AtlasCategory::Custom(loc) => &loc.path,
        }
    }

    /// Associated ResourceLocation for the atlas definition (e.g. `"minecraft:blocks"`).
    pub fn atlas_location(&self) -> ResourceLocation {
        match self {
            AtlasCategory::Blocks => ResourceLocation::vanilla("blocks"),
            AtlasCategory::Items => ResourceLocation::vanilla("items"),
            AtlasCategory::Particles => ResourceLocation::vanilla("particles"),
            AtlasCategory::Chests => ResourceLocation::vanilla("chests"),
            AtlasCategory::ShulkerBoxes => ResourceLocation::vanilla("shulker_boxes"),
            AtlasCategory::BannerPatterns => ResourceLocation::vanilla("banner_patterns"),
            AtlasCategory::ShieldPatterns => ResourceLocation::vanilla("shield_patterns"),
            AtlasCategory::ArmorTrims => ResourceLocation::vanilla("armor_trims"),
            AtlasCategory::DecoratedPot => ResourceLocation::vanilla("decorated_pot"),
            AtlasCategory::Paintings => ResourceLocation::vanilla("paintings"),
            AtlasCategory::Celestials => ResourceLocation::vanilla("celestials"),
            AtlasCategory::Gui => ResourceLocation::vanilla("gui"),
            AtlasCategory::MapDecorations => ResourceLocation::vanilla("map_decorations"),
            AtlasCategory::Entities => ResourceLocation::vanilla("entities"),
            AtlasCategory::Misc => ResourceLocation::vanilla("misc"),
            AtlasCategory::Custom(loc) => loc.clone(),
        }
    }

    /// Parse category name string into AtlasCategory.
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "blocks" | "block" => AtlasCategory::Blocks,
            "items" | "item" => AtlasCategory::Items,
            "particles" | "particle" => AtlasCategory::Particles,
            "chests" | "chest" => AtlasCategory::Chests,
            "shulker_boxes" | "shulker" => AtlasCategory::ShulkerBoxes,
            "banner_patterns" | "banners" => AtlasCategory::BannerPatterns,
            "shield_patterns" | "shields" => AtlasCategory::ShieldPatterns,
            "armor_trims" | "trims" => AtlasCategory::ArmorTrims,
            "decorated_pot" | "pot" => AtlasCategory::DecoratedPot,
            "paintings" | "painting" => AtlasCategory::Paintings,
            "celestials" | "environment" => AtlasCategory::Celestials,
            "gui" => AtlasCategory::Gui,
            "map_decorations" | "map" => AtlasCategory::MapDecorations,
            "entities" | "entity" => AtlasCategory::Entities,
            other => {
                if let Ok(loc) = ResourceLocation::parse(other) {
                    AtlasCategory::Custom(loc)
                } else {
                    AtlasCategory::Misc
                }
            }
        }
    }

    /// Classify any texture path string into its authoritative category.
    pub fn classify_texture_path(path: &str) -> Self {
        let clean = path
            .strip_prefix("assets/minecraft/textures/")
            .or_else(|| path.strip_prefix("textures/"))
            .unwrap_or(path)
            .trim_start_matches('/');

        if clean.starts_with("block/") {
            AtlasCategory::Blocks
        } else if clean.starts_with("item/") {
            AtlasCategory::Items
        } else if clean.starts_with("particle/") {
            AtlasCategory::Particles
        } else if clean.starts_with("painting/") {
            AtlasCategory::Paintings
        } else if clean.starts_with("trims/") {
            AtlasCategory::ArmorTrims
        } else if clean.starts_with("entity/chest/") {
            AtlasCategory::Chests
        } else if clean.starts_with("entity/shulker/") {
            AtlasCategory::ShulkerBoxes
        } else if clean.starts_with("entity/shield/") {
            AtlasCategory::ShieldPatterns
        } else if clean.starts_with("entity/banner/") {
            AtlasCategory::BannerPatterns
        } else if clean.starts_with("entity/decorated_pot/") {
            AtlasCategory::DecoratedPot
        } else if clean.starts_with("environment/") {
            AtlasCategory::Celestials
        } else if clean.starts_with("gui/") {
            AtlasCategory::Gui
        } else if clean.starts_with("map/") {
            AtlasCategory::MapDecorations
        } else if clean.starts_with("entity/") || clean.starts_with("models/armor/") {
            AtlasCategory::Entities
        } else {
            AtlasCategory::Misc
        }
    }
}
