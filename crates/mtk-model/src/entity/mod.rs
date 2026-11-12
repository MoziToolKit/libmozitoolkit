//! High-level typed BlockEntity representation decoupled from BlockState strings.

pub mod baker;
pub mod banner;
pub(crate) mod copper_statue;
pub mod instance;
pub mod painting;
pub(crate) mod pot;
pub mod render;
pub mod skull;

pub use baker::EntityBaker;
pub use banner::BannerPatternLayer;
pub use instance::{
    find_supported_visual_entity, is_supported_visual_entity, supported_visual_entity_ids,
    ArmorStandData, EntityBuildCapability, EntityInstance, EntityKindData, EntityValidationError,
    ItemFrameData, PaintingData, SupportedVisualEntity, ENTITY_ID_ARMOR_STAND,
    ENTITY_ID_GLOW_ITEM_FRAME, ENTITY_ID_ITEM_FRAME, ENTITY_ID_PAINTING, SUPPORTED_VISUAL_ENTITIES,
};
pub use render::{
    build_entity_render_plan, entity_render_key, host_transform_to_mc, EntityMaterialRoute,
    EntityRenderConfig, EntityRenderNode, EntityRenderPlan, EntityRenderTransform,
    EntityRenderWarning, ENTITY_RENDER_SCHEMA_VERSION,
};
pub use skull::{PlayerProfileData, SkullKind};

use serde::{Deserialize, Serialize};

#[inline]
fn default_banner_base_color() -> String {
    "white".to_string()
}

#[inline]
fn default_sign_color() -> String {
    "black".to_string()
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
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    /// Sign or Hanging Sign text and styling.
    #[serde(rename = "minecraft:sign", alias = "sign")]
    Sign {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        front_text: Vec<String>,
        #[serde(default = "default_sign_color")]
        front_color: String,
        #[serde(default)]
        front_glowing: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back_text: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back_color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back_glowing: Option<bool>,
        #[serde(default)]
        is_waxed: bool,
    },
    /// Generic or custom block entity with an explicit type ID and raw NBT payload.
    #[serde(rename = "generic")]
    Generic {
        custom_id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        nbt_bytes: Vec<u8>,
    },
    /// Raw generic NBT payload for unsupported or custom block entities without parsed ID.
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

    /// Constructs a canonical `Sign` data struct.
    pub fn new_sign(
        front_text: Vec<String>,
        front_color: Option<impl Into<String>>,
        front_glowing: bool,
    ) -> Self {
        Self::Sign {
            front_text,
            front_color: front_color
                .map(Into::into)
                .unwrap_or_else(default_sign_color),
            front_glowing,
            back_text: None,
            back_color: None,
            back_glowing: None,
            is_waxed: false,
        }
    }

    /// Constructs a generic custom block entity with an explicit identifier.
    pub fn new_generic(custom_id: impl Into<String>, nbt_bytes: Vec<u8>) -> Self {
        Self::Generic {
            custom_id: custom_id.into(),
            nbt_bytes,
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

    /// Normalizes sherds of a decorated pot to canonical pattern stems, turning invalid or brick patterns into `None`.
    pub fn normalize_sherds(&mut self) {
        if let Self::DecoratedPot { sherds } = self {
            for sherd in sherds.iter_mut() {
                *sherd = sherd
                    .as_deref()
                    .and_then(Self::normalize_sherd_pattern)
                    .map(str::to_string);
            }
        }
    }

    /// Serializes to a clean, canonical Visual NBT JSON string suitable for point clouds,
    /// `.mtkscene` files, and persistent state storage.
    pub fn to_visual_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserializes from a Visual NBT JSON string and normalizes sherds into canonical representation.
    pub fn from_visual_json(json_str: &str) -> Option<Self> {
        let mut data: Self = serde_json::from_str(json_str).ok()?;
        data.normalize_sherds();
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_core::Direction;

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

    #[test]
    fn test_block_entities_roundtrip() {
        // 1. Sign
        let sign = BlockEntityData::new_sign(
            vec!["Welcome to".to_string(), "MoziToolKit".to_string()],
            Some("yellow"),
            true,
        );
        let sign_json = sign.to_visual_json();
        assert!(sign_json.contains("\"minecraft:sign\""));
        assert!(sign_json.contains("\"yellow\""));
        assert!(sign_json.contains("\"front_glowing\":true"));
        let sign_parsed = BlockEntityData::from_visual_json(&sign_json).expect("Parse sign");
        assert_eq!(sign, sign_parsed);

        // 2. Generic Custom
        let generic = BlockEntityData::new_generic("custom:copper_statue", vec![1, 2, 3, 4]);
        let g_json = generic.to_visual_json();
        assert!(g_json.contains("\"generic\""));
        assert!(g_json.contains("\"custom:copper_statue\""));
        let g_parsed = BlockEntityData::from_visual_json(&g_json).expect("Parse generic");
        assert_eq!(generic, g_parsed);
    }

    #[test]
    fn test_entity_instance_roundtrip() {
        let painting = EntityInstance::new_painting(
            Some("12345678-1234-1234-1234-123456789abc".to_string()),
            glam::DVec3::new(10.5, 64.0, -20.5),
            Direction::South,
            glam::Vec3::new(0.0, std::f32::consts::PI, 0.0),
            "kebab",
            1,
            1,
            Some([10, 64, -20]),
        );
        let json = serde_json::to_string(&painting).expect("Serialize entity instance");
        assert!(json.contains("\"minecraft:painting\""));
        assert!(json.contains("\"kebab\""));
        assert!(json.contains("\"12345678-1234-1234-1234-123456789abc\""));
        let parsed: EntityInstance =
            serde_json::from_str(&json).expect("Deserialize entity instance");
        assert_eq!(painting, parsed);

        let frame = EntityInstance::new_item_frame(
            Some("87654321-4321-4321-4321-cba987654321".to_string()),
            glam::DVec3::new(5.0, 65.0, 3.0),
            Direction::West,
            glam::Vec3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0),
            "minecraft:diamond_sword",
            1,
            2,
            true,
            false,
            None,
        );
        let frame_json = serde_json::to_string(&frame).expect("Serialize item frame");
        assert!(frame_json.contains("\"minecraft:diamond_sword\""));
        let frame_parsed: EntityInstance = serde_json::from_str(&frame_json).expect("Parse frame");
        assert_eq!(frame, frame_parsed);
    }

    #[test]
    fn test_decorated_pot_normalization_alignment() {
        // Deserializing unnormalized sherd names or empty/brick strings normalizes to canonical stems and null
        let raw_json = r#"{"id":"minecraft:decorated_pot","sherds":["minecraft:angler_pottery_sherd","","minecraft:brick","archer_pottery_pattern"]}"#;
        let pot = BlockEntityData::from_visual_json(raw_json).expect("Must parse");
        assert_eq!(
            pot,
            BlockEntityData::DecoratedPot {
                sherds: [
                    Some("angler".to_string()),
                    None,
                    None,
                    Some("archer".to_string()),
                ],
            }
        );

        let canonical_json = pot.to_visual_json();
        assert_eq!(
            canonical_json,
            r#"{"id":"minecraft:decorated_pot","sherds":["angler",null,null,"archer"]}"#
        );
    }

    #[test]
    fn test_entity_category_decoupling() {
        let pot = BlockEntityData::new_decorated_pot([Some("angler"), None, None, None]);
        assert_eq!(pot.texture_identifier(), None);

        let banner = BlockEntityData::new_banner("red", Vec::<(&str, &str)>::new());
        assert!(banner.texture_identifier().is_some());

        let skull = BlockEntityData::new_skull(SkullKind::Skeleton, None, None);
        assert!(skull.texture_identifier().is_some());

        let sign = BlockEntityData::new_sign(vec!["hello".to_string()], None::<&str>, false);
        assert_eq!(sign.texture_identifier(), None);
    }
}
