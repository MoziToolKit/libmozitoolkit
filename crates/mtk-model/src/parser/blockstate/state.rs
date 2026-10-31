use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::ModelError;

/// Parsed Minecraft BlockState representation.
/// Separates namespace, block identifier, and sorted key-value properties.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockState {
    /// Namespace (defaults to "minecraft" if omitted).
    pub namespace: String,
    /// Block identifier name (e.g. "observer", "stone", "oak_stairs").
    pub name: String,
    /// Sorted properties map (e.g. {"facing": "north", "powered": "false"}).
    pub properties: BTreeMap<String, String>,
}

impl BlockState {
    /// Parses a BlockState string into a structured `BlockState`.
    ///
    /// # Examples
    /// - `"stone"` -> `minecraft:stone`
    /// - `"minecraft:observer[facing=north,powered=false]"`
    pub fn parse(s: &str) -> Result<Self, ModelError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ModelError::InvalidBlockStateSyntax(
                "Empty string".to_string(),
            ));
        }

        let (base_part, prop_part) = if let Some(open_bracket) = trimmed.find('[') {
            if !trimmed.ends_with(']') {
                return Err(ModelError::InvalidBlockStateSyntax(format!(
                    "Missing closing bracket in '{}'",
                    trimmed
                )));
            }
            let base = &trimmed[..open_bracket];
            let props = &trimmed[open_bracket + 1..trimmed.len() - 1];
            (base, Some(props))
        } else {
            (trimmed, None)
        };

        // Parse namespace and name
        let (namespace, name) = if let Some(colon) = base_part.find(':') {
            let ns = &base_part[..colon];
            let n = &base_part[colon + 1..];
            (ns.to_string(), n.to_string())
        } else {
            ("minecraft".to_string(), base_part.to_string())
        };

        let mut properties = BTreeMap::new();
        if let Some(props_str) = prop_part {
            let props_trimmed = props_str.trim();
            if !props_trimmed.is_empty() {
                for item in props_trimmed.split(',') {
                    let pair = item.trim();
                    if pair.is_empty() {
                        continue;
                    }
                    if let Some(eq_idx) = pair.find('=') {
                        let k = pair[..eq_idx].trim().to_ascii_lowercase();
                        let v = pair[eq_idx + 1..].trim().to_ascii_lowercase();
                        properties.insert(k, v);
                    } else {
                        return Err(ModelError::InvalidBlockStateSyntax(format!(
                            "Invalid property item '{}' in '{}'",
                            pair, trimmed
                        )));
                    }
                }
            }
        }

        Ok(Self {
            namespace,
            name,
            properties,
        })
    }

    /// Full qualified identifier `namespace:name` without properties.
    #[inline]
    pub fn block_id(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }

    /// Returns canonical representation where properties are deterministically sorted.
    pub fn to_canonical_string(&self) -> String {
        if self.properties.is_empty() {
            format!("{}:{}", self.namespace, self.name)
        } else {
            let props_str: Vec<String> = self
                .properties
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!("{}:{}[{}]", self.namespace, self.name, props_str.join(","))
        }
    }

    /// Checks if a subset of required properties matches this blockstate.
    pub fn matches_properties<'a, I>(&self, required_props: I) -> bool
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        for (k, v) in required_props {
            match self.properties.get(k) {
                Some(curr_v) if curr_v == v => continue,
                _ => return false,
            }
        }
        true
    }
}

impl FromStr for BlockState {
    type Err = ModelError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_canonical_string())
    }
}

/// Canonicalises a blockstate identifier into the server-authoritative representation:
/// - Leading/trailing whitespace is trimmed.
/// - Void air, cave air, structure void, bubble column (with or without properties) -> "minecraft:air"
/// - Empty string -> "minecraft:air"
#[inline]
pub fn normalize_canonical_blockstate(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "minecraft:air";
    }
    // The Java SSOT normalizes the *block type* (`state.is(Blocks.BUBBLE_COLUMN)`),
    // which ignores properties. Strip an optional `[...]` suffix before matching so
    // `minecraft:bubble_column[drag=true]` canonicalises to air exactly like the server.
    let base = trimmed
        .find('[')
        .map_or(trimmed, |property_start| &trimmed[..property_start]);
    match base {
        "minecraft:air"
        | "air"
        | "minecraft:cave_air"
        | "cave_air"
        | "minecraft:void_air"
        | "void_air"
        | "minecraft:structure_void"
        | "structure_void"
        | "minecraft:bubble_column"
        | "bubble_column" => "minecraft:air",
        _ => trimmed,
    }
}

/// Returns true if the given block name or blockstate string is considered air-like.
#[inline]
pub fn is_air_like(raw: &str) -> bool {
    normalize_canonical_blockstate(raw) == "minecraft:air"
}
