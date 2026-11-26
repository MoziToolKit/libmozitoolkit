use crate::identifier::ResourceLocation;
use crate::identifier_pattern::IdentifierPattern;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unstitch region slice definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnstitchRegion {
    pub sprite: ResourceLocation,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// All data-driven Sprite Source types supported by vanilla Minecraft 1.19.3+.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AtlasSource {
    #[serde(rename = "minecraft:directory", alias = "directory")]
    Directory { source: String, prefix: String },

    #[serde(rename = "minecraft:single", alias = "single")]
    Single {
        resource: ResourceLocation,
        #[serde(default)]
        sprite: Option<ResourceLocation>,
    },

    /// Removes sprites collected *so far* (`SpriteSourceList.list` runs sources in order).
    ///
    /// The JSON shape is `{"type": "...filter", "pattern": {"namespace"?, "path"?}}` — the pattern
    /// is nested, exactly like Minecraft's `SourceFilter.pattern`.
    #[serde(rename = "minecraft:filter", alias = "filter")]
    Filter { pattern: IdentifierPattern },

    #[serde(rename = "minecraft:unstitch", alias = "unstitch")]
    Unstitch {
        resource: ResourceLocation,
        #[serde(default = "default_divisor")]
        divisor_x: f64,
        #[serde(default = "default_divisor")]
        divisor_y: f64,
        regions: Vec<UnstitchRegion>,
    },

    #[serde(
        rename = "minecraft:paletted_permutations",
        alias = "paletted_permutations"
    )]
    PalettedPermutations {
        palette_key: ResourceLocation,
        permutations: HashMap<String, ResourceLocation>,
        textures: Vec<ResourceLocation>,
        #[serde(default = "default_separator")]
        separator: String,
    },
}

fn default_divisor() -> f64 {
    1.0
}

fn default_separator() -> String {
    "_".to_string()
}

/// Top-level container for atlas JSON configuration (`assets/<namespace>/atlases/*.json`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AtlasDefinition {
    pub sources: Vec<AtlasSource>,
}

impl AtlasDefinition {
    pub fn parse_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}
