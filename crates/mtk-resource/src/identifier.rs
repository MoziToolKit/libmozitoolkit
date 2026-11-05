use crate::error::ResourceError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

pub(crate) const DEFAULT_NAMESPACE: &str = "minecraft";

/// A standardized Minecraft Resource Location (Identifier), such as `minecraft:block/stone`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceLocation {
    pub namespace: String,
    pub path: String,
}

impl ResourceLocation {
    /// Create a new ResourceLocation with an explicit namespace and path.
    pub fn new(namespace: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            path: path.into(),
        }
    }

    /// Create a new ResourceLocation in the `minecraft` namespace.
    pub fn vanilla(path: impl Into<String>) -> Self {
        Self::new(DEFAULT_NAMESPACE, path)
    }

    /// Parse a resource string such as `"minecraft:block/stone"` or `"block/stone"`.
    /// Missing namespace defaults to `"minecraft"`.
    pub fn parse(s: &str) -> Result<Self, ResourceError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ResourceError::InvalidLocation(
                "Empty resource string".to_string(),
            ));
        }

        if let Some((ns, path)) = s.split_once(':') {
            if ns.is_empty() {
                return Err(ResourceError::InvalidLocation(format!(
                    "Empty namespace in '{}'",
                    s
                )));
            }
            if path.is_empty() {
                return Err(ResourceError::InvalidLocation(format!(
                    "Empty path in '{}'",
                    s
                )));
            }
            Ok(Self::new(ns, path))
        } else {
            Ok(Self::new(DEFAULT_NAMESPACE, s))
        }
    }

    /// Parse any texture path format (raw asset path, texture relative path, or canonical location)
    /// into a canonical ResourceLocation.
    ///
    /// Examples:
    /// - `"block/stone"` -> `minecraft:block/stone`
    /// - `"minecraft:block/stone"` -> `minecraft:block/stone`
    /// - `"textures/block/dirt.png"` -> `minecraft:block/dirt`
    /// - `"assets/minecraft/textures/block/stone.png"` -> `minecraft:block/stone`
    /// - `"create:textures/block/cogwheel.png"` -> `create:block/cogwheel`
    /// - `"assets/create/textures/block/cogwheel.png"` -> `create:block/cogwheel`
    pub fn parse_texture_path(input: &str) -> Result<Self, ResourceError> {
        let input = input.trim().replace('\\', "/");
        if input.is_empty() {
            return Err(ResourceError::InvalidLocation(
                "Empty texture path".to_string(),
            ));
        }

        // 1. Check if starts with "assets/<namespace>/textures/<path>"
        if let Some(loc) = Self::from_asset_path(&input, "textures", "png") {
            return Ok(loc);
        }
        if let Some(loc) = Self::from_asset_path(&input, "textures", "") {
            return Ok(loc);
        }

        // 2. Check if has namespace prefix, e.g. "namespace:path"
        if let Some((ns, path)) = input.split_once(':') {
            let clean_path = path
                .strip_prefix("textures/")
                .unwrap_or(path)
                .strip_suffix(".png")
                .unwrap_or(path);
            return Ok(Self::new(ns, clean_path));
        }

        // 3. Raw path without namespace, e.g. "textures/block/stone.png" or "block/stone"
        let clean_path = input
            .strip_prefix("textures/")
            .unwrap_or(&input)
            .strip_suffix(".png")
            .unwrap_or(&input);

        Ok(Self::new(DEFAULT_NAMESPACE, clean_path))
    }

    /// Canonical string format: `"namespace:path"`.
    pub fn as_string(&self) -> String {
        format!("{}:{}", self.namespace, self.path)
    }

    /// Convert to a physical file path within standard Minecraft `assets/` structure.
    /// E.g. `location.to_asset_path("textures", "png")` -> `"assets/minecraft/textures/block/stone.png"`
    pub fn to_asset_path(&self, category_dir: &str, extension: &str) -> String {
        let ext = extension.strip_prefix('.').unwrap_or(extension);
        if ext.is_empty() {
            format!("assets/{}/{}/{}", self.namespace, category_dir, self.path)
        } else {
            format!(
                "assets/{}/{}/{}.{}",
                self.namespace, category_dir, self.path, ext
            )
        }
    }

    /// Try to construct a ResourceLocation from a physical relative asset path.
    /// E.g. `"assets/minecraft/textures/block/stone.png"` with prefix `"textures"` and ext `"png"`
    /// -> `Some(ResourceLocation("minecraft", "block/stone"))`
    pub fn from_asset_path(asset_path: &str, category_dir: &str, extension: &str) -> Option<Self> {
        let path = asset_path.replace('\\', "/");
        let remainder = path.strip_prefix("assets/")?;
        let (ns, rest) = remainder.split_once('/')?;

        let cat_prefix = format!("{}/", category_dir);
        if !rest.starts_with(&cat_prefix) {
            return None;
        }
        let inner_path = &rest[cat_prefix.len()..];

        let ext_suffix = if extension.starts_with('.') {
            extension.to_string()
        } else if !extension.is_empty() {
            format!(".{}", extension)
        } else {
            String::new()
        };

        let trimmed_path = if !ext_suffix.is_empty() && inner_path.ends_with(&ext_suffix) {
            &inner_path[..inner_path.len() - ext_suffix.len()]
        } else {
            inner_path
        };

        Some(Self::new(ns, trimmed_path))
    }

    /// Return a new ResourceLocation with a PBR companion suffix (e.g. `_n` or `_s`).
    pub fn with_suffix(&self, suffix: &str) -> Self {
        Self::new(&self.namespace, format!("{}{}", self.path, suffix))
    }

    /// Return the `.mcmeta` asset location.
    pub fn to_mcmeta_asset_path(&self, category_dir: &str, extension: &str) -> String {
        format!("{}.mcmeta", self.to_asset_path(category_dir, extension))
    }

    /// Get the short file stem without directory prefixes.
    pub fn short_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Generate all prioritized candidate asset file paths for resolving a model identifier.
    ///
    /// Examples:
    /// - `"minecraft:block/stone"` -> `["assets/minecraft/models/block/stone.json", "assets/minecraft/models/stone.json"]`
    /// - `"stone"` -> `["assets/minecraft/models/block/stone.json", "assets/minecraft/models/stone.json", "assets/minecraft/models/item/stone.json"]`
    /// - `"item/diamond_sword"` -> `["assets/minecraft/models/item/diamond_sword.json", "assets/minecraft/models/diamond_sword.json"]`
    pub fn model_candidate_asset_paths(input: &str) -> Vec<String> {
        let trimmed = input.trim().replace('\\', "/");
        let (ns, raw_path) = if let Some((ns, p)) = trimmed.split_once(':') {
            (ns, p)
        } else {
            (DEFAULT_NAMESPACE, trimmed.as_str())
        };

        let clean_path = raw_path
            .strip_prefix("models/")
            .unwrap_or(raw_path)
            .strip_suffix(".json")
            .unwrap_or(raw_path);

        if clean_path.starts_with("block/") || clean_path.starts_with("item/") {
            vec![
                format!("assets/{}/models/{}.json", ns, clean_path),
                format!(
                    "assets/{}/models/{}.json",
                    ns,
                    clean_path
                        .split_once('/')
                        .map(|(_, p)| p)
                        .unwrap_or(clean_path)
                ),
            ]
        } else {
            vec![
                format!("assets/{}/models/block/{}.json", ns, clean_path),
                format!("assets/{}/models/{}.json", ns, clean_path),
                format!("assets/{}/models/item/{}.json", ns, clean_path),
            ]
        }
    }
}

impl fmt::Display for ResourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl fmt::Debug for ResourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl FromStr for ResourceLocation {
    type Err = ResourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for ResourceLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for ResourceLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
