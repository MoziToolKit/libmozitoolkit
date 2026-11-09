use serde::{Deserialize, Serialize};

use super::sprite::default_category;
use crate::atlas::pbr::PbrMeta;

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
