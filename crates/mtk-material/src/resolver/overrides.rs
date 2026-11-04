//! External, data-driven override rules for material and biome resolution.
//!
//! The object is loaded from a JSON compatibility profile (Python-owned for
//! fast iteration) and applied both at precompile time and when an asset cache
//! is opened.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One keyword -> candidate texture/model rule for legacy or modded material names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialFallbackRule {
    /// Case-insensitive substring matched against the raw or cleaned material name.
    pub keyword: String,
    /// Candidate atlas keys, tried in order.
    pub candidates: Vec<String>,
}

/// Optional data-driven overrides applied on top of built-in defaults.
///
/// `None` fields keep the built-in behavior. An explicit empty list/map
/// disables the corresponding fallback set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolverOverrides {
    #[serde(default)]
    pub strip_biome_suffixes: Option<Vec<String>>,
    #[serde(default)]
    pub material_fallbacks: Option<Vec<MaterialFallbackRule>>,
    #[serde(default)]
    pub tint_categories: HashMap<String, String>,
    #[serde(default)]
    pub hardcoded_colors: HashMap<String, [f32; 4]>,
}

impl ResolverOverrides {
    /// Returns configured suffix stripping rules, or the built-in exporter list.
    pub fn effective_biome_suffixes(&self) -> &[String] {
        match &self.strip_biome_suffixes {
            Some(list) => list,
            None => builtin_biome_suffixes(),
        }
    }

    /// Returns configured material fallbacks, or the built-in legacy rules.
    pub fn effective_material_fallbacks(&self) -> &[MaterialFallbackRule] {
        match &self.material_fallbacks {
            Some(list) => list,
            None => builtin_material_fallbacks(),
        }
    }
}

/// Historical DCC exporter suffixes (Mineways / jmc2obj and similar).
///
/// Ordered longest-first so a naive `ends_with` consumer can never truncate
/// `_dark_forest` down to `_forest`. Consumers that accept user-provided lists
/// must still prefer the longest match.
pub fn builtin_biome_suffixes() -> &'static [String] {
    static SUFFIXES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    SUFFIXES.get_or_init(|| {
        let mut suffixes: Vec<String> = [
            "-desert",
            "-forest",
            "-swamp",
            "-taiga",
            "-snow",
            "-ocean",
            "-jungle",
            "-badlands",
            "-savanna",
            "-dark_forest",
            "-birch_forest",
            "-plains",
            "-meadow",
            "-mangrove",
            "-cherry_grove",
            "-cold_ocean",
            "-warm_ocean",
            "_desert",
            "_forest",
            "_swamp",
            "_taiga",
            "_snow",
            "_ocean",
            "_jungle",
            "_badlands",
            "_savanna",
            "_dark_forest",
            "_birch_forest",
            "_plains",
            "_meadow",
            "_mangrove",
            "_cherry_grove",
            "_cold_ocean",
            "_warm_ocean",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        suffixes.sort_by_key(|suffix| std::cmp::Reverse(suffix.len()));
        suffixes
    })
}

/// Built-in fallbacks for legacy/modded materials (retired, now owned by Python layer).
pub fn builtin_material_fallbacks() -> &'static [MaterialFallbackRule] {
    &[]
}

/// Parses a JSON compatibility profile.
pub fn parse_overrides(json_str: &str) -> Result<ResolverOverrides, serde_json::Error> {
    serde_json::from_str(json_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_custom_rules() {
        let json = r#"{
            "strip_biome_suffixes": ["_custom"],
            "material_fallbacks": [{"keyword": "leaves", "candidates": ["block/oak_leaves"]}],
            "tint_categories": {"mod:block/grass": "grass"},
            "hardcoded_colors": {"mod:block/red": [1.0, 0.0, 0.0, 1.0]}
        }"#;
        let overrides = parse_overrides(json).expect("parse overrides");
        assert_eq!(overrides.effective_biome_suffixes(), ["_custom"]);
        assert_eq!(
            overrides.effective_material_fallbacks()[0].keyword,
            "leaves"
        );
        assert_eq!(overrides.tint_categories["mod:block/grass"], "grass");
    }

    #[test]
    fn empty_override_disables_builtin_rules() {
        let overrides =
            parse_overrides(r#"{"strip_biome_suffixes": [], "material_fallbacks": []}"#)
                .expect("parse overrides");
        assert!(overrides.effective_biome_suffixes().is_empty());
        assert!(overrides.effective_material_fallbacks().is_empty());
    }

    #[test]
    fn biome_resolver_applies_texture_overrides() {
        let overrides = parse_overrides(
            r#"{
                "tint_categories": {"mod:block/grass": "grass"},
                "hardcoded_colors": {"mod:block/red": [1.0, 0.0, 0.0, 1.0]}
            }"#,
        )
        .expect("parse overrides");
        let mut resolver = crate::biome::BiomeResolver::new();
        resolver.apply_overrides(&overrides);
        let tint = resolver.get_tint_info("mod:block/grass", None, Some(0));
        assert_eq!(tint.tint_category, "grass");
        let hard = resolver.get_tint_info("mod:block/red", None, Some(0));
        assert!(hard.is_hardcoded);
        assert_eq!(hard.hardcoded_color, Some([1.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn default_keeps_builtin_rules() {
        let overrides = ResolverOverrides::default();
        assert!(overrides
            .effective_biome_suffixes()
            .contains(&"_forest".to_string()));
        assert!(overrides.effective_material_fallbacks().is_empty());
    }

    #[test]
    fn builtin_biome_suffixes_are_longest_first() {
        let suffixes = builtin_biome_suffixes();
        assert!(
            suffixes.windows(2).all(|w| w[0].len() >= w[1].len()),
            "suffix list must be ordered longest-first"
        );
        for required in ["_dark_forest", "-dark_forest", "_cherry_grove"] {
            assert!(
                suffixes.iter().any(|suffix| suffix == required),
                "missing built-in suffix {required}"
            );
        }
        let mut unique = suffixes.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), suffixes.len(), "duplicate built-in suffix");
    }
}
