use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::super::hardcoded::TINT_TYPE_NONE;

/// Complete resolved tint metadata for a single texture or block face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TintInfo {
    pub tint_type: u8,
    pub tint_category: String,
    pub tint_weight: f32,
    pub base_tint_weight: f32,
    pub overlay_tint_weight: f32,
    pub has_overlay: bool,
    pub overlay_texture: Option<String>,
    pub is_hardcoded: bool,
    pub hardcoded_color: Option<[f32; 4]>,
    pub hardcoded_hex: Option<String>,
}

impl Default for TintInfo {
    fn default() -> Self {
        Self {
            tint_type: TINT_TYPE_NONE,
            tint_category: "none".to_string(),
            tint_weight: 0.0,
            base_tint_weight: 0.0,
            overlay_tint_weight: 0.0,
            has_overlay: false,
            overlay_texture: None,
            is_hardcoded: false,
            hardcoded_color: None,
            hardcoded_hex: None,
        }
    }
}

/// Resource-pack aware Biome Resolver.
/// Discovers model JSON `tintindex` metadata and `side` / `overlay` texture pairings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BiomeResolver {
    pub overlay_pairs: HashMap<String, String>,
    pub texture_tint_categories: HashMap<String, String>,
    pub texture_hardcoded_colors: HashMap<String, [f32; 4]>,
    #[serde(default)]
    pub models: HashMap<String, Value>,
    #[serde(default)]
    pub models_by_stem: HashMap<String, Value>,
}

impl Default for BiomeResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl BiomeResolver {
    pub fn new() -> Self {
        let mut resolver = Self {
            overlay_pairs: HashMap::new(),
            texture_tint_categories: HashMap::new(),
            texture_hardcoded_colors: HashMap::new(),
            models: HashMap::new(),
            models_by_stem: HashMap::new(),
        };
        resolver.seed_defaults();
        resolver
    }

    /// Merges external compatibility overrides into the resolver maps.
    pub fn apply_overrides(&mut self, overrides: &crate::ResolverOverrides) {
        for (key, category) in &overrides.tint_categories {
            self.texture_tint_categories
                .insert(key.clone(), category.clone());
        }
        for (key, color) in &overrides.hardcoded_colors {
            self.texture_hardcoded_colors.insert(key.clone(), *color);
        }
    }

    /// Serialize BiomeResolver mapping table to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize BiomeResolver mapping table from a JSON string.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Load BiomeResolver from a JSON file on disk.
    pub fn from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}
