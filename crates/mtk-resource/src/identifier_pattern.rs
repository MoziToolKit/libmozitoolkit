//! Minecraft's `IdentifierPattern`: two optional regexes, matched with `Pattern.asPredicate()`.
//!
//! Used by both data formats that filter resource locations: atlas `filter` sources
//! (`SourceFilter`) and `pack.mcmeta`'s `filter.block` (`ResourceFilterSection`). Matching is a
//! substring search, exactly like Java's `Pattern::asPredicate`, and an absent field matches
//! everything.

use crate::identifier::ResourceLocation;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One field of an [`IdentifierPattern`]: a regex kept alongside its source text.
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

/// Which resource locations a filter selects: an absent field matches everything.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IdentifierPattern {
    #[serde(default)]
    pub namespace: Option<FilterPattern>,
    #[serde(default)]
    pub path: Option<FilterPattern>,
}

impl IdentifierPattern {
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

/// `pack.mcmeta`'s `filter` section: locations this pack hides.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ResourceFilterSection {
    #[serde(default)]
    pub block: Vec<IdentifierPattern>,
}

impl ResourceFilterSection {
    /// True when any pattern in the section selects `location`.
    pub fn is_filtered(&self, location: &ResourceLocation) -> bool {
        self.block.iter().any(|pattern| pattern.matches(location))
    }

    /// True when this section would hide anything at all.
    pub fn is_active(&self) -> bool {
        !self.block.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(namespace: &str, path: &str) -> ResourceLocation {
        ResourceLocation::new(namespace, path)
    }

    #[test]
    fn an_absent_field_matches_everything() {
        assert!(IdentifierPattern::default().matches(&location("minecraft", "block/stone")));
    }

    #[test]
    fn matching_is_a_substring_search_like_java() {
        let pattern: IdentifierPattern =
            serde_json::from_str(r#"{"path":"block/stone"}"#).expect("pattern");
        assert!(pattern.matches(&location("minecraft", "block/stone")));
        assert!(pattern.matches(&location("minecraft", "block/stone_bricks")));
        assert!(!pattern.matches(&location("minecraft", "block/dirt")));
    }

    #[test]
    fn the_filter_section_is_a_list_of_patterns() {
        let section: ResourceFilterSection =
            serde_json::from_str(r#"{"block":[{"path":"block/"},{"namespace":"mymod"}]}"#)
                .expect("section");
        assert!(section.is_filtered(&location("minecraft", "block/stone")));
        assert!(section.is_filtered(&location("mymod", "anything")));
        assert!(!section.is_filtered(&location("minecraft", "item/stick")));
    }
}
