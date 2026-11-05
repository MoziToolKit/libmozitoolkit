//! High-level typed BlockEntity representation decoupled from BlockState strings.

pub mod banner;
pub(crate) mod pot;
pub mod skull;

pub use banner::BannerPatternLayer;
pub use skull::{PlayerProfileData, SkullKind};

use serde::{Deserialize, Serialize};

#[inline]
fn default_banner_base_color() -> String {
    "white".to_string()
}

#[inline]
fn default_true() -> bool {
    true
}

#[inline]
fn default_item_count() -> u8 {
    1
}

#[inline]
fn default_painting_dimension() -> u8 {
    1
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
    /// Item Frame or Glow Item Frame (treated as block-attached entity).
    #[serde(rename = "minecraft:item_frame", alias = "item_frame")]
    ItemFrame {
        item_id: String,
        #[serde(default = "default_item_count")]
        count: u8,
        #[serde(default)]
        rotation: u8,
        #[serde(default)]
        is_glowing: bool,
        #[serde(default)]
        is_invisible: bool,
    },
    /// Hanging Painting (treated as block-attached entity).
    #[serde(rename = "minecraft:painting", alias = "painting")]
    Painting {
        variant: String,
        #[serde(default = "default_painting_dimension")]
        width: u8,
        #[serde(default = "default_painting_dimension")]
        height: u8,
    },
    /// End Crystal (treated as block-attached entity).
    #[serde(rename = "minecraft:end_crystal", alias = "end_crystal")]
    EndCrystal {
        #[serde(default = "default_true")]
        show_bottom: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        beam_target: Option<[i32; 3]>,
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

    /// Constructs a canonical `ItemFrame` data struct.
    pub fn new_item_frame(
        item_id: impl Into<String>,
        count: u8,
        rotation: u8,
        is_glowing: bool,
        is_invisible: bool,
    ) -> Self {
        Self::ItemFrame {
            item_id: item_id.into(),
            count: count.max(1),
            rotation: rotation & 7,
            is_glowing,
            is_invisible,
        }
    }

    /// Constructs a canonical `Painting` data struct.
    pub fn new_painting(variant: impl Into<String>, width: u8, height: u8) -> Self {
        Self::Painting {
            variant: variant.into(),
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// Constructs a canonical `EndCrystal` data struct.
    pub fn new_end_crystal(show_bottom: bool, beam_target: Option<[i32; 3]>) -> Self {
        Self::EndCrystal {
            show_bottom,
            beam_target,
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
            Self::Painting { variant, .. } => {
                let v = variant.strip_prefix("minecraft:").unwrap_or(variant);
                Some(format!("minecraft:painting/{}", v))
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

    /// Returns true if this entity is an intrinsic discrete voxel block entity
    /// tied to an integer grid block (e.g. DecoratedPot, Banner, Skull, Sign).
    #[inline]
    pub fn is_voxel_block_entity(&self) -> bool {
        matches!(
            self,
            Self::DecoratedPot { .. }
                | Self::Banner { .. }
                | Self::Skull { .. }
                | Self::Sign { .. }
                | Self::Generic { .. }
                | Self::Raw { .. }
        )
    }

    /// Returns true if this entity is a continuous-space entity (e.g. ItemFrame, Painting, EndCrystal)
    /// that is decoupled from discrete voxel blocks.
    #[inline]
    pub fn is_continuous_space_entity(&self) -> bool {
        matches!(
            self,
            Self::ItemFrame { .. } | Self::Painting { .. } | Self::EndCrystal { .. }
        )
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
    fn test_block_attached_entities_roundtrip() {
        // 1. Item Frame
        let item_frame =
            BlockEntityData::new_item_frame("minecraft:diamond_sword", 1, 3, true, false);
        let if_json = item_frame.to_visual_json();
        assert!(if_json.contains("\"minecraft:item_frame\""));
        assert!(if_json.contains("\"minecraft:diamond_sword\""));
        assert!(if_json.contains("\"is_glowing\":true"));
        let if_parsed = BlockEntityData::from_visual_json(&if_json).expect("Parse item frame");
        assert_eq!(item_frame, if_parsed);

        // 2. Painting
        let painting = BlockEntityData::new_painting("minecraft:kebab", 2, 2);
        let p_json = painting.to_visual_json();
        assert!(p_json.contains("\"minecraft:painting\""));
        assert!(p_json.contains("\"minecraft:kebab\""));
        assert_eq!(
            painting.texture_identifier(),
            Some("minecraft:painting/kebab".to_string())
        );
        let p_parsed = BlockEntityData::from_visual_json(&p_json).expect("Parse painting");
        assert_eq!(painting, p_parsed);

        // 3. End Crystal
        let crystal = BlockEntityData::new_end_crystal(true, Some([100, 64, -200]));
        let ec_json = crystal.to_visual_json();
        assert!(ec_json.contains("\"minecraft:end_crystal\""));
        assert!(ec_json.contains("100"));
        let ec_parsed = BlockEntityData::from_visual_json(&ec_json).expect("Parse end crystal");
        assert_eq!(crystal, ec_parsed);

        // 4. Sign
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

        // 5. Generic Custom
        let generic = BlockEntityData::new_generic("custom:copper_statue", vec![1, 2, 3, 4]);
        let g_json = generic.to_visual_json();
        assert!(g_json.contains("\"generic\""));
        assert!(g_json.contains("\"custom:copper_statue\""));
        let g_parsed = BlockEntityData::from_visual_json(&g_json).expect("Parse generic");
        assert_eq!(generic, g_parsed);
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
        assert!(pot.is_voxel_block_entity());
        assert!(!pot.is_continuous_space_entity());

        let banner = BlockEntityData::new_banner("red", Vec::<(&str, &str)>::new());
        assert!(banner.is_voxel_block_entity());
        assert!(!banner.is_continuous_space_entity());

        let skull = BlockEntityData::new_skull(SkullKind::Skeleton, None, None);
        assert!(skull.is_voxel_block_entity());
        assert!(!skull.is_continuous_space_entity());

        let sign = BlockEntityData::new_sign(vec!["hello".to_string()], None::<&str>, false);
        assert!(sign.is_voxel_block_entity());
        assert!(!sign.is_continuous_space_entity());

        let item_frame = BlockEntityData::new_item_frame("minecraft:compass", 1, 0, false, false);
        assert!(!item_frame.is_voxel_block_entity());
        assert!(item_frame.is_continuous_space_entity());

        let painting = BlockEntityData::new_painting("minecraft:alban", 1, 1);
        assert!(!painting.is_voxel_block_entity());
        assert!(painting.is_continuous_space_entity());

        let crystal = BlockEntityData::new_end_crystal(true, None);
        assert!(!crystal.is_voxel_block_entity());
        assert!(crystal.is_continuous_space_entity());
    }
}
