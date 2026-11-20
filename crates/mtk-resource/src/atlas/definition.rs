use crate::identifier::ResourceLocation;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

/// One field of an [`AtlasFilterPattern`]: a regex kept alongside its source text.
///
/// Minecraft compiles these with `java.util.regex.Pattern` and matches with
/// `Pattern.asPredicate()` (= `matcher.find()`, a substring search), so `block/.*_ore` matches
/// `block/diamond_ore` and an unanchored `stone` matches `block/redstone_block`.
#[derive(Debug, Clone)]
pub struct FilterPattern {
    source: String,
    regex: regex::Regex,
}

impl FilterPattern {
    /// Compiles a pattern the way Minecraft does, so an invalid regex is a data error.
    pub fn new(source: impl Into<String>) -> Result<Self, regex::Error> {
        let source = source.into();
        let regex = regex::Regex::new(&source)?;
        Ok(Self { source, regex })
    }

    /// The original pattern text.
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Minecraft's `Pattern.asPredicate()`: true when the regex matches anywhere in `value`.
    pub fn is_match(&self, value: &str) -> bool {
        self.regex.is_match(value)
    }
}

/// Two patterns are equal when their source text is; the compiled form is derived.
impl PartialEq for FilterPattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Serialize for FilterPattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for FilterPattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Self::new(source).map_err(serde::de::Error::custom)
    }
}

/// `filter` pattern for atlas sources — Minecraft's `IdentifierPattern`.
///
/// The two fields are matched independently: an absent field matches everything.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AtlasFilterPattern {
    #[serde(default)]
    pub namespace: Option<FilterPattern>,
    #[serde(default)]
    pub path: Option<FilterPattern>,
}

impl AtlasFilterPattern {
    /// True when this pattern selects `location` (namespace and path must both match).
    pub fn matches(&self, location: &ResourceLocation) -> bool {
        let namespace_matches = self
            .namespace
            .as_ref()
            .is_none_or(|pattern| pattern.is_match(&location.namespace));
        let path_matches = self
            .path
            .as_ref()
            .is_none_or(|pattern| pattern.is_match(&location.path));
        namespace_matches && path_matches
    }
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
    Filter { pattern: AtlasFilterPattern },

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
    },
}

fn default_divisor() -> f64 {
    1.0
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
