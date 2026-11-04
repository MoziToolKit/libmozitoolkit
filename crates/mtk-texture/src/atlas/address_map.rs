use mtk_resource::{AnimationMetadata, ResourceLocation, TextureKeyVariants};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::atlas::dynamic::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
use crate::atlas::pbr::PbrMeta;

/// Source priority level for atlas address map keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtlasKeySource {
    /// Authoritative prebaked static asset from resource packs (higher priority).
    Static = 0,
    /// Procedurally registered dynamic entity sprite (e.g. skin, banner).
    Dynamic = 1,
}

/// Classification of sprite placement and usage mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SpriteKind {
    /// Placed on a static chunk (either a truly static sprite or Frame 0 of an animated sprite).
    #[default]
    StaticAtlas,
    /// Placed on a dedicated animated chunk (full vertical strip, frame_count > 1).
    AnimatedAtlas,
    /// Standalone-only texture.
    StandaloneOnly,
}

/// Address and metrics for a single sprite located within a stitched Atlas Chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasSpriteLocation {
    pub chunk_id: u16,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub is_animated: bool,
    #[serde(default)]
    pub sprite_kind: SpriteKind,
    #[serde(default)]
    pub texture_id: u32,
    /// Normalized UV bounds: `[u_min, v_min, u_max, v_max]` in [0.0..1.0] atlas space (Frame 0 or static frame).
    pub uv_bounds: [f32; 4],
    /// Normalized UV bounds of Frame 0 specifically: `[u_min, v_min, u_max, v_max]`.
    #[serde(default)]
    pub frame_0_uv_bounds: [f32; 4],
    /// Normalized local UV bounds in single-block [0.0..1.0] space (default [0.0, 0.0, 1.0, 1.0]).
    #[serde(default = "default_local_uv_bounds")]
    pub local_uv_bounds: [f32; 4],
    /// Step size in UV space per animation frame: `[u_step, v_step]`.
    #[serde(default)]
    pub frame_uv_step: [f32; 2],
    /// Physical pixel rectangle on the atlas chunk for Frame 0: `[x, y, width, height]`.
    #[serde(default)]
    pub pixel_rect: [u32; 4],
    /// Physical pixel rectangle on the atlas chunk for the entire animation strip: `[x, y, width, height]`.
    #[serde(default)]
    pub strip_pixel_rect: [u32; 4],
    /// Single-frame physical resolution: `[frame_width, frame_height]`.
    #[serde(default)]
    pub frame_size: [u32; 2],
    /// Total animation frames.
    pub frame_count: u32,
    /// Optional animation timing metadata.
    pub animation: Option<AnimationMetadata>,
    /// Whether this slot contains a non-default Normal companion map.
    pub has_normal: bool,
    /// Whether this slot contains a non-default Specular companion map.
    pub has_specular: bool,
    /// Whether this slot contains a non-default Overlay companion map.
    #[serde(default)]
    pub has_overlay: bool,
    /// Average solid color of the texture as a hex string `#RRGGBB` (excluding alpha).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_color: Option<String>,
    /// Optional slot index for dynamic entity atlases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_index: Option<u16>,
}

impl Default for AtlasSpriteLocation {
    fn default() -> Self {
        Self {
            chunk_id: 0,
            category: default_category(),
            is_animated: false,
            sprite_kind: SpriteKind::default(),
            texture_id: 0,
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            local_uv_bounds: default_local_uv_bounds(),
            frame_uv_step: [0.0, 0.0],
            pixel_rect: [0, 0, 16, 16],
            strip_pixel_rect: [0, 0, 16, 16],
            frame_size: [16, 16],
            frame_count: 1,
            animation: None,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            average_color: None,
            slot_index: None,
        }
    }
}

fn default_category() -> String {
    "blocks".to_string()
}

fn default_local_uv_bounds() -> [f32; 4] {
    [0.0, 0.0, 1.0, 1.0]
}

/// Specification for an individual texture channel / file belonging to an Atlas Chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtlasTextureSlot {
    /// Texture role: `"albedo"`, `"normal"`, `"orm"`, `"aux"`, `"overlay"`.
    pub role: String,
    /// Canonical file name on disk or in archive, e.g. `"blocks_chunk_001_orm.png"`.
    pub file_name: String,
    /// Texture color space required for DCC shader sampler: `"sRGB"` or `"Non-Color"`.
    pub colorspace: String,
}

/// Metadata for an individual Atlas Chunk (sheet).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasChunkMeta {
    pub chunk_id: u16,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub is_animated: bool,
    #[serde(default = "default_chunk_index")]
    pub category_chunk_index: usize,
    pub width: u32,
    pub height: u32,
    pub has_normal: bool,
    pub has_specular: bool,
    #[serde(default)]
    pub has_overlay: bool,
    /// Whether the packed `_orm` sheet was written for this chunk.
    #[serde(default)]
    pub has_orm: bool,
    /// Whether the packed `_aux` sheet was written for this chunk.
    #[serde(default)]
    pub has_aux: bool,
    /// Whether this chunk contains any sprites that support biome or hardcoded tinting.
    #[serde(default = "default_has_tint")]
    pub has_tint: bool,
    /// Canonical PBR layout metadata; `None` means a legacy cache predating the current format.
    #[serde(default)]
    pub pbr: Option<PbrMeta>,
    /// Authoritative texture slots for DCC shader loading and binding.
    #[serde(default)]
    pub textures: Vec<AtlasTextureSlot>,
}

fn default_has_tint() -> bool {
    true
}

fn default_chunk_index() -> usize {
    1
}

impl AtlasChunkMeta {
    /// Canonical file stem for this atlas sheet, e.g. `"blocks_chunk_001"` or `"blocks_anim_chunk_001"`.
    pub fn file_stem(&self) -> String {
        if self.is_animated {
            format!(
                "{}_anim_chunk_{:03}",
                self.category, self.category_chunk_index
            )
        } else {
            format!("{}_chunk_{:03}", self.category, self.category_chunk_index)
        }
    }

    /// Derives the canonical texture slots according to active channel flags and convention.
    pub fn build_texture_slots(&self) -> Vec<AtlasTextureSlot> {
        let stem = self.file_stem();
        let mut slots = Vec::with_capacity(5);

        // Albedo is always present for every atlas sheet
        slots.push(AtlasTextureSlot {
            role: "albedo".to_string(),
            file_name: format!("{stem}.png"),
            colorspace: "sRGB".to_string(),
        });

        if self.has_normal {
            slots.push(AtlasTextureSlot {
                role: "normal".to_string(),
                file_name: format!("{stem}_n.png"),
                colorspace: "Non-Color".to_string(),
            });
        }

        if self.has_orm {
            slots.push(AtlasTextureSlot {
                role: "orm".to_string(),
                file_name: format!("{stem}_orm.png"),
                colorspace: "Non-Color".to_string(),
            });
        }

        if self.has_aux {
            slots.push(AtlasTextureSlot {
                role: "aux".to_string(),
                file_name: format!("{stem}_aux.png"),
                colorspace: "Non-Color".to_string(),
            });
        }

        if self.has_overlay {
            slots.push(AtlasTextureSlot {
                role: "overlay".to_string(),
                file_name: format!("{stem}_overlay.png"),
                colorspace: "sRGB".to_string(),
            });
        }

        slots
    }

    /// Returns the active texture slots, automatically deriving them from channel flags if empty.
    pub fn active_textures(&self) -> Vec<AtlasTextureSlot> {
        if !self.textures.is_empty() {
            self.textures.clone()
        } else {
            self.build_texture_slots()
        }
    }

    /// Synchronizes `self.textures` with current flags.
    pub fn refresh_textures(&mut self) {
        self.textures = self.build_texture_slots();
    }
}

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
    pub sprite_sources: HashMap<ResourceLocation, AtlasKeySource>,
}

impl AtlasAddressMap {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            sprites: HashMap::new(),
            anim_sprites: HashMap::new(),
            sprite_sources: HashMap::new(),
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
    pub fn insert_with_source(
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
            "missing",
            "missingno",
            "minecraft:missingno",
            "minecraft:block/stone",
            "block/stone",
            "minecraft:block/dirt",
            "block/dirt",
            "minecraft:block/oak_leaves",
            "block/oak_leaves",
        ] {
            if let Some(loc) = self.lookup_str(cand) {
                return Some(loc);
            }
        }
        let def_chunk = self.default_chunk_id();
        self.sprites
            .values()
            .find(|s| s.chunk_id == def_chunk)
            .or_else(|| self.sprites.values().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_texture_slots() {
        let mut meta = AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 1024,
            height: 1024,
            has_normal: true,
            has_specular: false,
            has_overlay: true,
            has_orm: true,
            has_aux: false,
            has_tint: true,
            pbr: None,
            textures: Vec::new(),
        };

        meta.refresh_textures();
        assert_eq!(meta.textures.len(), 4); // albedo, normal, orm, overlay

        let albedo = meta.textures.iter().find(|s| s.role == "albedo").unwrap();
        assert_eq!(albedo.file_name, "blocks_chunk_001.png");
        assert_eq!(albedo.colorspace, "sRGB");

        let normal = meta.textures.iter().find(|s| s.role == "normal").unwrap();
        assert_eq!(normal.file_name, "blocks_chunk_001_n.png");
        assert_eq!(normal.colorspace, "Non-Color");

        let orm = meta.textures.iter().find(|s| s.role == "orm").unwrap();
        assert_eq!(orm.file_name, "blocks_chunk_001_orm.png");
        assert_eq!(orm.colorspace, "Non-Color");

        let overlay = meta.textures.iter().find(|s| s.role == "overlay").unwrap();
        assert_eq!(overlay.file_name, "blocks_chunk_001_overlay.png");
        assert_eq!(overlay.colorspace, "sRGB");
    }

    #[test]
    fn test_backward_compatibility_json() {
        // Old JSON without `textures` array
        let legacy_json = r#"{
            "chunks": [
                {
                    "chunk_id": 0,
                    "category": "blocks",
                    "category_chunk_index": 1,
                    "width": 1024,
                    "height": 1024,
                    "has_normal": false,
                    "has_specular": false,
                    "has_orm": true
                }
            ],
            "sprites": {}
        }"#;

        let map = AtlasAddressMap::from_json(legacy_json).expect("deserialize legacy json");
        let chunk = &map.chunks[0];
        assert_eq!(chunk.textures.len(), 2); // albedo + orm
        assert_eq!(chunk.textures[0].role, "albedo");
        assert_eq!(chunk.textures[1].role, "orm");
        assert_eq!(chunk.textures[1].colorspace, "Non-Color");

        // Serialize back
        let serialized = map.to_json().expect("serialize to json");
        assert!(serialized.contains(r#""textures""#));
        assert!(serialized.contains(r#""blocks_chunk_001_orm.png""#));
    }
}
