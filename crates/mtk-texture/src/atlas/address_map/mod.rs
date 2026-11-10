pub mod chunk;
pub mod sprite;

#[cfg(test)]
mod tests;

pub use chunk::{AtlasChunkMeta, AtlasTextureSlot};
pub(crate) use sprite::AtlasKeySource;
pub use sprite::{AtlasSpriteLocation, SpriteKind};

use mtk_resource::{ResourceLocation, TextureKeyVariants};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::atlas::dynamic::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;

/// Authoritative mapping table holding all sprite locations and chunk descriptors.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AtlasAddressMap {
    #[serde(default)]
    pub chunks: Vec<AtlasChunkMeta>,
    /// Static atlas sprites (100% texture coverage, Frame 0 for animated sprites).
    pub sprites: HashMap<ResourceLocation, AtlasSpriteLocation>,
    /// Dedicated animated atlas sprites (full strips for multi-frame animated textures).
    #[serde(default)]
    pub anim_sprites: HashMap<ResourceLocation, AtlasSpriteLocation>,
    /// Source priority records per sprite key (Static vs Dynamic).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) sprite_sources: HashMap<ResourceLocation, AtlasKeySource>,
    /// Optional CTM rules associated with this address map.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ctm_rules: Vec<mtk_resource::CtmRule>,
}

impl AtlasAddressMap {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            sprites: HashMap::new(),
            anim_sprites: HashMap::new(),
            sprite_sources: HashMap::new(),
            ctm_rules: Vec::new(),
        }
    }

    /// O(1) primary lookup by exact ResourceLocation (checks static sprites first, then anim sprites).
    pub fn lookup(&self, location: &ResourceLocation) -> Option<&AtlasSpriteLocation> {
        self.sprites
            .get(location)
            .or_else(|| self.anim_sprites.get(location))
    }

    /// Lookup static Frame 0 sprite location (100% coverage across all textures).
    pub fn lookup_static(&self, location: &ResourceLocation) -> Option<&AtlasSpriteLocation> {
        self.sprites.get(location)
    }

    /// Lookup dedicated animated strip sprite location.
    pub fn lookup_animated(&self, location: &ResourceLocation) -> Option<&AtlasSpriteLocation> {
        self.anim_sprites.get(location)
    }

    /// O(1) lookup by string (e.g. `"minecraft:block/stone"`, `"block/stone"`, or `"textures/block/stone.png"`).
    pub fn lookup_str(&self, s: &str) -> Option<&AtlasSpriteLocation> {
        if let Ok(loc) = ResourceLocation::parse_texture_path(s) {
            self.lookup(&loc)
        } else if let Ok(loc) = ResourceLocation::parse(s) {
            self.lookup(&loc)
        } else {
            None
        }
    }

    /// Lookup static Frame 0 sprite location by string.
    pub fn lookup_static_str(&self, s: &str) -> Option<&AtlasSpriteLocation> {
        if let Ok(loc) = ResourceLocation::parse_texture_path(s) {
            self.lookup_static(&loc)
        } else if let Ok(loc) = ResourceLocation::parse(s) {
            self.lookup_static(&loc)
        } else {
            None
        }
    }

    /// Lookup animated strip sprite location by string.
    pub fn lookup_animated_str(&self, s: &str) -> Option<&AtlasSpriteLocation> {
        if self.anim_sprites.is_empty() || s.is_empty() {
            return None;
        }
        let s = s.trim();
        let s = s.strip_suffix(".png").unwrap_or(s);
        let (ns, path) = if let Some((ns, p)) = s.split_once(':') {
            (ns, p)
        } else {
            ("minecraft", s)
        };
        let norm_path = path.strip_prefix("textures/").unwrap_or(path);

        if norm_path.starts_with("block/") {
            let loc = ResourceLocation::new(ns, norm_path);
            if let Some(sp) = self.anim_sprites.get(&loc) {
                return Some(sp);
            }
            return None;
        }

        for (loc, sp) in &self.anim_sprites {
            if loc.namespace == ns
                && (loc.path == path
                    || loc.path == norm_path
                    || loc.path.strip_prefix("block/").unwrap_or(&loc.path) == norm_path
                    || norm_path.strip_prefix("block/").unwrap_or(norm_path) == loc.path)
            {
                return Some(sp);
            }
        }
        None
    }

    /// Resolves a texture key across all possible canonical aliases (skin/, banner:, etc.).
    pub fn resolve_texture_key(&self, key: &str) -> Option<&AtlasSpriteLocation> {
        if let Some(loc) = self.lookup_str(key) {
            return Some(loc);
        }
        for variant in TextureKeyVariants::for_key(key) {
            if let Some(loc) = self.lookup_str(&variant) {
                return Some(loc);
            }
        }
        let lower = key.to_ascii_lowercase();
        if lower != key {
            if let Some(loc) = self.lookup_str(&lower) {
                return Some(loc);
            }
            for variant in TextureKeyVariants::for_key(&lower) {
                if let Some(loc) = self.lookup_str(&variant) {
                    return Some(loc);
                }
            }
        }
        None
    }

    /// Inserts a sprite location with source priority tracking.
    ///
    /// Static sprites take precedence over dynamic sprites. If an alias already resolves
    /// to a static chunk (`chunk_id < DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID`), a dynamic insertion
    /// will not override it and returns `false`.
    pub(crate) fn insert_with_source(
        &mut self,
        key: ResourceLocation,
        loc: AtlasSpriteLocation,
        source: AtlasKeySource,
    ) -> bool {
        if let Some(existing) = self.sprites.get(&key) {
            let default_source = if existing.chunk_id < DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID {
                AtlasKeySource::Static
            } else {
                AtlasKeySource::Dynamic
            };
            let existing_source = self
                .sprite_sources
                .get(&key)
                .copied()
                .unwrap_or(default_source);
            if source == AtlasKeySource::Dynamic && existing_source == AtlasKeySource::Static {
                return false;
            }
        }
        self.sprite_sources.insert(key.clone(), source);
        self.sprites.insert(key, loc);
        true
    }

    /// Merge another address map into this one.
    pub fn merge(&mut self, other: AtlasAddressMap) {
        self.chunks.extend(other.chunks);
        self.sprites.extend(other.sprites);
        self.anim_sprites.extend(other.anim_sprites);
        self.sprite_sources.extend(other.sprite_sources);
    }

    /// Synchronizes texture slots across all chunks based on their active channel flags.
    pub fn refresh_textures(&mut self) {
        for chunk in &mut self.chunks {
            chunk.refresh_textures();
        }
    }

    /// Serialize to formatted JSON, ensuring all chunks have their authoritative textures populated.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        if self.chunks.iter().all(|c| !c.textures.is_empty()) {
            serde_json::to_string_pretty(self)
        } else {
            let mut copy = self.clone();
            copy.refresh_textures();
            serde_json::to_string_pretty(&copy)
        }
    }

    /// Deserialize from JSON string, ensuring backward compatibility with older caches lacking `textures`.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        let mut map: Self = serde_json::from_str(json_str)?;
        for chunk in &mut map.chunks {
            if chunk.textures.is_empty() {
                chunk.refresh_textures();
            }
        }
        Ok(map)
    }

    /// Lookup chunk descriptor by chunk ID.
    pub fn get_chunk_meta(&self, chunk_id: u16) -> Option<&AtlasChunkMeta> {
        self.chunks.iter().find(|c| c.chunk_id == chunk_id)
    }

    /// Returns a slice of all chunk descriptors.
    pub fn get_chunks(&self) -> &[AtlasChunkMeta] {
        &self.chunks
    }

    /// Returns the fallback chunk ID for unmapped textures (defaults to the first "blocks" chunk, or 0).
    pub fn default_chunk_id(&self) -> u16 {
        self.chunks
            .iter()
            .find(|c| c.category == "blocks")
            .map(|c| c.chunk_id)
            .unwrap_or(0)
    }

    /// Lookup fallback sprite location when a specific texture is missing from the atlas.
    pub fn lookup_fallback(&self) -> Option<&AtlasSpriteLocation> {
        for cand in &[
            crate::FALLBACK_TEXTURE_KEY,
            "minecraft:missingno",
            "missingno",
        ] {
            if let Some(loc) = self.lookup_str(cand) {
                return Some(loc);
            }
        }
        None
    }
}
