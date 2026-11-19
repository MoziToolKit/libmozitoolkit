//! Skull and player head entity definitions, SkullKind, and player profile texture extraction.

use serde::{Deserialize, Serialize};

/// Skull and Head species in Minecraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkullKind {
    Skeleton,
    WitherSkeleton,
    Zombie,
    Player,
    Creeper,
    Dragon,
    Piglin,
}

impl SkullKind {
    /// Resolves `SkullKind` from a blockstate or item name (e.g. "player_head", "skeleton_skull").
    pub fn from_block_name(name: &str) -> Option<Self> {
        let clean = name.strip_prefix("minecraft:").unwrap_or(name);
        if clean.contains("wither_skeleton") {
            Some(Self::WitherSkeleton)
        } else if clean.contains("skeleton") {
            Some(Self::Skeleton)
        } else if clean.contains("zombie") {
            Some(Self::Zombie)
        } else if clean.contains("player") {
            Some(Self::Player)
        } else if clean.contains("creeper") {
            Some(Self::Creeper)
        } else if clean.contains("dragon") {
            Some(Self::Dragon)
        } else if clean.contains("piglin") {
            Some(Self::Piglin)
        } else {
            None
        }
    }

    /// Resolves default vanilla entity texture path in standard assets.
    pub fn default_texture_path(&self) -> &'static str {
        match self {
            Self::Skeleton => "minecraft:entity/skeleton/skeleton",
            Self::WitherSkeleton => "minecraft:entity/skeleton/wither_skeleton",
            Self::Zombie => "minecraft:entity/zombie/zombie",
            Self::Player => "minecraft:entity/player/wide/steve",
            Self::Creeper => "minecraft:entity/creeper/creeper",
            Self::Dragon => "minecraft:entity/enderdragon/dragon",
            Self::Piglin => "minecraft:entity/piglin/piglin",
        }
    }
}

/// Player Profile Data extracted from `SkullOwner` or `components."minecraft:profile"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PlayerProfileData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_base64: Option<String>,
}

impl PlayerProfileData {
    pub fn new(
        name: Option<String>,
        id: Option<String>,
        skin_url: Option<String>,
        skin_base64: Option<String>,
    ) -> Self {
        let mut data = Self {
            name,
            id,
            skin_url,
            skin_base64,
        };
        // Auto-extract skin_url from skin_base64 if not already populated
        if data.skin_url.is_none() {
            if let Some(ref b64) = data.skin_base64 {
                data.skin_url = extract_skin_url_from_base64(b64);
            }
        }
        data
    }

    /// Derives the canonical cache key for this profile.
    pub fn skin_key(&self) -> String {
        derive_skin_key(
            self.skin_url.as_deref(),
            self.id.as_deref(),
            self.name.as_deref(),
        )
    }

    /// Returns the unique skin hash (e.g. from `http://textures.minecraft.net/texture/<hash>`).
    pub fn skin_hash(&self) -> Option<&str> {
        self.skin_url.as_deref().and_then(extract_skin_hash)
    }

    /// Returns the resolved texture resource identifier or fallback.
    pub fn texture_identifier(&self) -> String {
        let key = self.skin_key();
        if key != "default_steve" {
            format!("minecraft:skin/{}", key)
        } else {
            "minecraft:entity/player/wide/steve".to_string()
        }
    }
}

pub use mtk_resource::skin::{
    derive_skin_key, extract_skin_url_from_property_base64 as extract_skin_url_from_base64,
};

/// Extracts the final URL segment hash from a skin URL.
pub fn extract_skin_hash(url: &str) -> Option<&str> {
    let clean = url.trim_end_matches('/');
    clean.rsplit('/').next().filter(|s| !s.is_empty())
}
