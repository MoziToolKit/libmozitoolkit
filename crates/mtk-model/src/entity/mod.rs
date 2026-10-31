//! High-level typed BlockEntity representation decoupled from BlockState strings.

pub mod banner;
pub mod pot;
pub mod skull;

pub use banner::{
    banner_texture_identifier, dye_id_to_name, dye_rgb, name_to_dye_id, normalize_banner_pattern,
    BannerPatternLayer, DYE_NAMES, DYE_RGB,
};
pub use pot::{normalize_sherd_pattern, sherd_texture, sherd_texture_ref};
pub use skull::{
    decode_base64, extract_skin_hash, extract_skin_url_from_base64, PlayerProfileData, SkullKind,
};

use serde::{Deserialize, Serialize};

#[inline]
fn default_banner_base_color() -> String {
    "white".to_string()
}

/// High-level typed BlockEntity representation decoupled from BlockState strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "id")]
pub enum BlockEntityData {
    /// Decorated Pot with up to 4 sherd patterns.
    #[serde(rename = "minecraft:decorated_pot", alias = "decorated_pot")]
    DecoratedPot {
        /// Sherd patterns on [back, left, right, front].
        /// `None` indicates default undecorated brick side.
        sherds: [Option<String>; 4],
    },
    /// Banner with base color and pattern layers.
    #[serde(rename = "minecraft:banner", alias = "banner")]
    Banner {
        #[serde(default = "default_banner_base_color")]
        base_color: String,
        #[serde(default)]
        patterns: Vec<BannerPatternLayer>,
    },
    /// Skull or Player Head with species, rotation, and optional profile.
    #[serde(rename = "minecraft:skull", alias = "skull")]
    Skull {
        kind: SkullKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<PlayerProfileData>,
    },
    /// Raw generic NBT payload for unsupported or custom block entities.
    #[serde(rename = "raw")]
    Raw { data: Vec<u8> },
}

impl BlockEntityData {
    /// Normalizes a Minecraft Pottery Sherd item ID or pattern name into a standard pattern key.
    #[inline]
    pub fn normalize_sherd_pattern(item_id: &str) -> Option<&'static str> {
        pot::normalize_sherd_pattern(item_id)
    }

    /// Resolves the static texture identifier for a specific sherd side without heap allocation.
    #[inline]
    pub fn sherd_texture_ref(pattern: Option<&str>) -> &'static str {
        pot::sherd_texture_ref(pattern)
    }

    /// Resolves the texture identifier for a specific sherd side.
    #[inline]
    pub fn sherd_texture(pattern: Option<&str>) -> String {
        pot::sherd_texture(pattern)
    }

    /// Constructs a canonical `DecoratedPot` data struct, normalizing each sherd into a clean pattern stem.
    pub fn new_decorated_pot(sherds: [Option<&str>; 4]) -> Self {
        Self::DecoratedPot {
            sherds: [
                sherds[0]
                    .and_then(Self::normalize_sherd_pattern)
                    .map(str::to_string),
                sherds[1]
                    .and_then(Self::normalize_sherd_pattern)
                    .map(str::to_string),
                sherds[2]
                    .and_then(Self::normalize_sherd_pattern)
                    .map(str::to_string),
                sherds[3]
                    .and_then(Self::normalize_sherd_pattern)
                    .map(str::to_string),
            ],
        }
    }

    /// Constructs a canonical `Banner` data struct.
    pub fn new_banner(
        base_color: impl Into<String>,
        patterns: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::Banner {
            base_color: base_color.into(),
            patterns: patterns
                .into_iter()
                .map(|(p, c)| BannerPatternLayer::new(p, c))
                .collect(),
        }
    }

    /// Constructs a canonical `Skull` data struct.
    pub fn new_skull(
        kind: SkullKind,
        rotation: Option<u8>,
        profile: Option<PlayerProfileData>,
    ) -> Self {
        Self::Skull {
            kind,
            rotation,
            profile,
        }
    }

    /// Computes the canonical dynamic texture identifier for this block entity (if applicable).
    pub fn texture_identifier(&self) -> Option<String> {
        match self {
            Self::Banner {
                base_color,
                patterns,
            } => Some(banner::banner_texture_identifier(base_color, patterns)),
            Self::Skull { profile, kind, .. } => {
                if let Some(prof) = profile {
                    Some(prof.texture_identifier())
                } else {
                    Some(kind.default_texture_path().to_string())
                }
            }
            _ => None,
        }
    }

    /// Serializes to a clean, canonical Visual NBT JSON string suitable for point clouds,
    /// `.mtkscene` files, and persistent state storage.
    pub fn to_visual_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserializes from a Visual NBT JSON string.
    pub fn from_visual_json(json_str: &str) -> Option<Self> {
        serde_json::from_str(json_str).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_visual_nbt_json_roundtrip() {
        let pot = BlockEntityData::new_decorated_pot([
            Some("minecraft:angler_pottery_sherd"),
            Some("brick"),
            Some("miner_pottery_pattern"),
            Some("blade"),
        ]);

        let json = pot.to_visual_json();
        assert!(json.contains("\"minecraft:decorated_pot\""));
        assert!(json.contains("\"angler\""));
        assert!(json.contains("\"miner\""));
        assert!(json.contains("\"blade\""));
        assert!(!json.contains("brick"));

        let parsed = BlockEntityData::from_visual_json(&json).expect("Must parse visual json");
        assert_eq!(pot, parsed);
    }

    #[test]
    fn test_banner_and_skull_roundtrip() {
        let banner =
            BlockEntityData::new_banner("white", vec![("stripe_top", "blue"), ("flower", "red")]);
        let b_json = banner.to_visual_json();
        assert!(b_json.contains("\"minecraft:banner\""));
        assert!(b_json.contains("\"stripe_top\""));
        let b_parsed = BlockEntityData::from_visual_json(&b_json).unwrap();
        assert_eq!(banner, b_parsed);

        let skull = BlockEntityData::new_skull(
            SkullKind::Player,
            Some(4),
            Some(PlayerProfileData::new(
                Some("Steve".to_string()),
                None,
                Some("http://textures.minecraft.net/texture/abc123def".to_string()),
                None,
            )),
        );
        let s_json = skull.to_visual_json();
        assert!(s_json.contains("\"minecraft:skull\""));
        assert!(s_json.contains("\"player\""));
        assert!(s_json.contains("\"Steve\""));
        let s_parsed = BlockEntityData::from_visual_json(&s_json).unwrap();
        assert_eq!(skull, s_parsed);
    }

    #[test]
    fn test_plain_banner_json() {
        let json = r#"{"id":"minecraft:banner","base_color":"red"}"#;
        let parsed = BlockEntityData::from_visual_json(json);
        assert!(
            parsed.is_some(),
            "Must parse plain banner without patterns field"
        );
    }
}
