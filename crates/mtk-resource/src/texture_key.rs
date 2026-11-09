//! Canonical texture key variant derivation and alias resolution.

use crate::identifier::ResourceLocation;
use std::collections::HashSet;

/// Utilities for resolving and generating texture key aliases across namespaces and domains.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextureKeyVariants;

impl TextureKeyVariants {
    /// Generates all canonical aliases and variations for a given texture identifier key.
    ///
    /// Handles player skins (`minecraft:skin/`, `skin/`), procedural banners (`banner:`, `minecraft:banner/`),
    /// and standard block/item assets. All variants are returned in priority order with deduplication.
    pub fn for_key(key: &str) -> Vec<String> {
        let mut variants = Vec::with_capacity(16);
        let mut seen = HashSet::new();

        let mut add = |s: String| {
            if seen.insert(s.clone()) {
                variants.push(s);
            }
        };

        // 1. Direct key
        add(key.to_string());

        // 2. Banner texture aliases
        let banner_spec = key
            .strip_prefix("minecraft:banner/")
            .or_else(|| key.strip_prefix("minecraft:banner:"))
            .or_else(|| key.strip_prefix("banner:"))
            .or_else(|| key.strip_prefix("banner/"))
            .or_else(|| key.strip_prefix("minecraft:entity/banner/"))
            .or_else(|| key.strip_prefix("entity/banner/"));

        if let Some(spec) = banner_spec {
            add(format!("banner:{spec}"));
            add(format!("minecraft:banner/{spec}"));
            add(format!("banner/{spec}"));
            add(format!("minecraft:entity/banner/{spec}"));
            add(format!("entity/banner/{spec}"));
            // Composite banner pattern specs (e.g. "white+stripe_center:red") must not
            // generate bare block names or bare tokens which would create invalid ResourceLocations.
            if !spec.contains(':') && !spec.contains('+') {
                add(format!("minecraft:{spec}_banner"));
                add(format!("{spec}_banner"));
                add(spec.to_string());
            }
        }

        // 3. Player skin texture aliases
        let skin_spec = key
            .strip_prefix("minecraft:skin/")
            .or_else(|| key.strip_prefix("skin/"))
            .or_else(|| key.strip_prefix("minecraft:player/"))
            .or_else(|| key.strip_prefix("player/"))
            .or_else(|| key.strip_prefix("entity/player/"));

        if let Some(spec) = skin_spec {
            add(format!("minecraft:skin/{spec}"));
            add(format!("skin/{spec}"));
            add(format!("minecraft:player/{spec}"));
            add(format!("player/{spec}"));
            add(format!("entity/player/{spec}"));
            add(spec.to_string());

            let base = spec
                .strip_prefix("uuid_")
                .or_else(|| spec.strip_prefix("player_"))
                .unwrap_or(spec);
            if base != spec {
                add(format!("minecraft:skin/{base}"));
                add(format!("skin/{base}"));
                add(base.to_string());
            }
        } else if banner_spec.is_none() {
            // If the key is a raw skin identifier without prefix (e.g. "player_Alex" or hex)
            if key.starts_with("player_")
                || key.starts_with("uuid_")
                || key.starts_with("cdn_")
                || (key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()))
            {
                add(format!("minecraft:skin/{key}"));
                add(format!("skin/{key}"));
                add(format!("minecraft:player/{key}"));
                add(format!("player/{key}"));
                add(format!("entity/player/{key}"));
            }
        }

        // 3.5. Painting texture aliases
        let painting_spec = key
            .strip_prefix("minecraft:painting/")
            .or_else(|| key.strip_prefix("painting/"))
            .or_else(|| key.strip_prefix("minecraft:textures/painting/"))
            .or_else(|| key.strip_prefix("textures/painting/"));

        if let Some(spec) = painting_spec {
            add(format!("minecraft:painting/{spec}"));
            add(format!("painting/{spec}"));
            add(format!("minecraft:textures/painting/{spec}"));
            add(format!("textures/painting/{spec}"));
            add(format!("minecraft:{spec}"));
            add(spec.to_string());
        }

        // 4. Standard namespace aliases
        if let Some(stripped) = key.strip_prefix("minecraft:") {
            add(stripped.to_string());
        } else if !key.contains(':') {
            add(format!("minecraft:{key}"));
        }

        variants
    }

    /// Converts all key variants into valid `ResourceLocation`s, deduplicating the results.
    ///
    /// Validates that namespaces only contain valid identifier characters `[a-z0-9_.-]`,
    /// discarding any malformed aliases.
    pub fn resource_locations_for_key(key: &str) -> Vec<ResourceLocation> {
        let mut locs = Vec::new();
        let mut seen = HashSet::new();

        for variant in Self::for_key(key) {
            if let Ok(loc) = ResourceLocation::parse_texture_path(&variant) {
                if is_valid_namespace(&loc.namespace) && seen.insert(loc.clone()) {
                    locs.push(loc);
                }
            }
            if let Ok(loc) = ResourceLocation::parse(&variant) {
                if is_valid_namespace(&loc.namespace) && seen.insert(loc.clone()) {
                    locs.push(loc);
                }
            }
        }

        locs
    }
}

#[inline]
fn is_valid_namespace(ns: &str) -> bool {
    !ns.is_empty()
        && ns.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' || c == '-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skin_key_variants() {
        let variants = TextureKeyVariants::for_key("player_Alex");
        assert!(variants.contains(&"minecraft:skin/player_Alex".to_string()));
        assert!(variants.contains(&"skin/player_Alex".to_string()));
        assert!(variants.contains(&"player_Alex".to_string()));

        let locs = TextureKeyVariants::resource_locations_for_key("minecraft:skin/uuid_1234");
        assert!(locs.iter().any(|l| l.path == "skin/uuid_1234"));
        assert!(locs.iter().any(|l| l.path == "skin/1234"));
    }

    #[test]
    fn test_banner_key_variants_composite_pattern() {
        let variants = TextureKeyVariants::for_key("banner:white+stripe_center:red");
        assert!(variants.contains(&"minecraft:banner/white+stripe_center:red".to_string()));
        assert!(variants.contains(&"banner/white+stripe_center:red".to_string()));
        // Ensure composite pattern does not produce invalid bare block tokens
        assert!(!variants.contains(&"white+stripe_center:red".to_string()));
        assert!(!variants.contains(&"minecraft:white+stripe_center:red_banner".to_string()));

        let locs = TextureKeyVariants::resource_locations_for_key("banner:white+stripe_center:red");
        // Ensure all produced ResourceLocations have valid namespaces (no bogus "white+stripe_center" namespace)
        for loc in &locs {
            assert!(
                loc.namespace == "minecraft" || loc.namespace == "banner",
                "Unexpected namespace '{}' in location {:?}",
                loc.namespace,
                loc
            );
        }
    }

    #[test]
    fn test_painting_key_variants() {
        let variants = TextureKeyVariants::for_key("minecraft:painting/kebab");
        assert!(variants.contains(&"minecraft:painting/kebab".to_string()));
        assert!(variants.contains(&"painting/kebab".to_string()));
        assert!(variants.contains(&"minecraft:kebab".to_string()));
        assert!(variants.contains(&"kebab".to_string()));

        let locs = TextureKeyVariants::resource_locations_for_key("minecraft:painting/water");
        assert!(locs
            .iter()
            .any(|l| l.namespace == "minecraft" && l.path == "painting/water"));
        assert!(locs
            .iter()
            .any(|l| l.namespace == "minecraft" && l.path == "water"));
    }

    #[test]
    fn test_player_alex_variants_and_locations() {
        let variants = TextureKeyVariants::for_key("player_Alex");
        assert!(variants.contains(&"minecraft:skin/player_Alex".to_string()));
        assert!(variants.contains(&"skin/player_Alex".to_string()));

        let locs = TextureKeyVariants::resource_locations_for_key("player_Alex");
        assert!(locs
            .iter()
            .any(|l| l.namespace == "minecraft" && l.path == "skin/player_Alex"));
    }

    #[test]
    fn test_64_char_hash_variants_and_locations() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let variants = TextureKeyVariants::for_key(hash);
        assert!(variants.contains(&format!("minecraft:skin/{hash}")));
        assert!(variants.contains(&format!("skin/{hash}")));

        let locs = TextureKeyVariants::resource_locations_for_key(hash);
        assert!(locs
            .iter()
            .any(|l| l.namespace == "minecraft" && l.path == format!("skin/{hash}")));
    }

    #[test]
    fn test_standard_block_stone_variants_and_locations() {
        let variants = TextureKeyVariants::for_key("minecraft:block/stone");
        assert!(variants.contains(&"minecraft:block/stone".to_string()));
        assert!(variants.contains(&"block/stone".to_string()));
        // Regular blocks should never contain banner or skin aliases
        assert!(!variants.iter().any(|v| v.contains("skin/")));
        assert!(!variants.iter().any(|v| v.contains("banner")));

        let locs = TextureKeyVariants::resource_locations_for_key("minecraft:block/stone");
        assert!(locs
            .iter()
            .any(|l| l.namespace == "minecraft" && l.path == "block/stone"));
    }
}
