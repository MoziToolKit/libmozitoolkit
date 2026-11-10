//! Dynamic Atlas Chunk representation allocating 64x64 slots on a fixed-grid.

use std::collections::HashMap;

use crate::atlas::address_map::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, SpriteKind};
use crate::error::TextureError;
use mtk_resource::banner::VANILLA_BANNER_DYES;

/// Standard dimensions of a dynamic skin atlas chunk.
pub const DYNAMIC_ATLAS_WIDTH: u32 = 1024;
pub const DYNAMIC_ATLAS_HEIGHT: u32 = 1024;
pub(crate) const DYNAMIC_SLOT_SIZE: u32 = 64;
pub const DYNAMIC_SLOTS_PER_ROW: u32 = 16;
pub const DYNAMIC_TOTAL_SLOTS: usize = 256;

/// Default reserved chunk ID for the dynamic entity atlas (skins, banners, etc.)
pub use mtk_core::constants::atlas::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;

/// Maximum number of dynamic chunks supported (0xFFF0..=0xFFFF, 16 chunks = 4,096 unique 64x64 skins).
pub(crate) const MAX_DYNAMIC_CHUNKS: usize = 16;
pub const DYNAMIC_CHUNK_START: u16 = DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;

/// Information about an allocated slot inside a dynamic atlas chunk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicSlotInfo {
    pub slot_index: u16,
    pub col: u32,
    pub row: u32,
    pub pixel_rect: [u32; 4],
    pub uv_bounds: [f32; 4],
}

/// A fixed-size runtime dynamic atlas chunk that allocates 64x64 grid slots on demand.
#[derive(Debug, Clone)]
pub struct DynamicAtlasChunk {
    pub chunk_id: u16,
    pub width: u32,
    pub height: u32,
    pub rgba_buffer: Vec<u8>,
    pub pinned_slots: u16,
    pub(crate) slot_map: HashMap<String, u16>,
    next_slot: u16,
}

impl Default for DynamicAtlasChunk {
    fn default() -> Self {
        Self::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
    }
}

impl DynamicAtlasChunk {
    /// Creates a new empty dynamic atlas chunk with the given `chunk_id`.
    pub fn new(chunk_id: u16) -> Self {
        let size = (DYNAMIC_ATLAS_WIDTH * DYNAMIC_ATLAS_HEIGHT * 4) as usize;
        Self {
            chunk_id,
            width: DYNAMIC_ATLAS_WIDTH,
            height: DYNAMIC_ATLAS_HEIGHT,
            rgba_buffer: vec![0u8; size],
            pinned_slots: 0,
            slot_map: HashMap::new(),
            next_slot: 0,
        }
    }

    /// Creates a synthetic 64x64 banner cloth fallback buffer (neutral light-gray cloth).
    pub fn create_fallback_banner_base() -> crate::RgbaBuffer {
        let mut buf = crate::RgbaBuffer::new(64, 64);
        // Cloth front/back/sides in standard banner UV layout occupies (0, 0, 42, 41)
        for y in 0..41 {
            for x in 0..42 {
                let is_border = x == 0 || x == 41 || y == 0 || y == 40;
                let val = if is_border { 220u8 } else { 244u8 };
                buf.set_pixel(x, y, [val, val, val, 255]);
            }
        }
        buf
    }

    /// Prewarms slots 0..15 with the 16 vanilla dye colors of banner cloth, pinning them permanently.
    pub fn prewarm_vanilla_banners(
        &mut self,
        base_cloth: Option<&crate::RgbaBuffer>,
    ) -> Result<(), TextureError> {
        let fallback;
        let cloth = match base_cloth {
            Some(c) => c,
            None => {
                fallback = Self::create_fallback_banner_base();
                &fallback
            }
        };

        for (idx, (name, rgb)) in VANILLA_BANNER_DYES.iter().enumerate() {
            let slot = idx as u16;
            let tinted = cloth.to_tinted(*rgb);
            self.blit_skin_rgba(slot, &tinted.pixels, tinted.width, tinted.height)?;

            let aliases = [
                format!("banner:{}", name),
                format!("minecraft:banner/{}", name),
                format!("banner/{}", name),
                format!("minecraft:{}_banner", name),
                format!("{}_banner", name),
                name.to_string(),
            ];
            for alias in aliases {
                self.slot_map.insert(alias, slot);
            }
        }

        self.pinned_slots = 16;
        self.next_slot = self.next_slot.max(16);
        Ok(())
    }

    /// Returns the number of currently allocated unique skin slots.
    pub fn allocated_count(&self) -> usize {
        self.next_slot as usize
    }

    /// Checks if a skin key has already been allocated a slot.
    pub fn contains_key(&self, skin_key: &str) -> bool {
        self.slot_map.contains_key(skin_key)
    }

    /// Gets the slot index for a skin key, if already allocated.
    pub fn get_slot(&self, skin_key: &str) -> Option<u16> {
        self.slot_map.get(skin_key).copied()
    }

    /// Allocates or retrieves an existing slot for a skin key (natural deduplication).
    ///
    /// Respects `pinned_slots`: slots 0..pinned_slots are never evicted during wrap-around.
    /// Returns the slot index (0..255).
    pub fn allocate_slot(&mut self, skin_key: &str) -> u16 {
        if let Some(&slot_idx) = self.slot_map.get(skin_key) {
            return slot_idx;
        }

        let slot_idx = if (self.next_slot as usize) < DYNAMIC_TOTAL_SLOTS {
            let s = self.next_slot;
            self.next_slot += 1;
            s
        } else {
            // Reached maximum capacity (256 slots); wrap around in round-robin sequence within dynamic range
            let dynamic_capacity = DYNAMIC_TOTAL_SLOTS
                .saturating_sub(self.pinned_slots as usize)
                .max(1);
            let offset = (self.next_slot as usize - self.pinned_slots as usize) % dynamic_capacity;
            let s = (self.pinned_slots as usize + offset) as u16;
            self.next_slot = self.next_slot.wrapping_add(1);
            // Purge stale reverse entries previously mapping to this slot (pinned slots never purged)
            self.slot_map
                .retain(|_, &mut v| v != s || v < self.pinned_slots);
            s
        };

        self.slot_map.insert(skin_key.to_string(), slot_idx);
        slot_idx
    }

    /// Calculates physical pixel coordinates and normalized UV bounds for a given slot.
    pub fn slot_info(slot_idx: u16) -> DynamicSlotInfo {
        let col = (slot_idx as u32) % DYNAMIC_SLOTS_PER_ROW;
        let row = (slot_idx as u32) / DYNAMIC_SLOTS_PER_ROW;
        let px = col * DYNAMIC_SLOT_SIZE;
        let py = row * DYNAMIC_SLOT_SIZE;

        let u_min = px as f32 / DYNAMIC_ATLAS_WIDTH as f32;
        let u_max = (px + DYNAMIC_SLOT_SIZE) as f32 / DYNAMIC_ATLAS_WIDTH as f32;
        let v_min = 1.0 - (py + DYNAMIC_SLOT_SIZE) as f32 / DYNAMIC_ATLAS_HEIGHT as f32;
        let v_max = 1.0 - py as f32 / DYNAMIC_ATLAS_HEIGHT as f32;

        DynamicSlotInfo {
            slot_index: slot_idx,
            col,
            row,
            pixel_rect: [px, py, DYNAMIC_SLOT_SIZE, DYNAMIC_SLOT_SIZE],
            uv_bounds: [u_min, v_min, u_max, v_max],
        }
    }

    /// Blits a 64x64 (or 64x32) raw RGBA pixel buffer directly into the allocated slot.
    pub fn blit_skin_rgba(
        &mut self,
        slot_idx: u16,
        skin_rgba: &[u8],
        skin_width: u32,
        skin_height: u32,
    ) -> Result<DynamicSlotInfo, TextureError> {
        let info = Self::slot_info(slot_idx);

        if skin_width != 64 || (skin_height != 64 && skin_height != 32) {
            return Err(TextureError::Baking(format!(
                "Invalid skin dimensions: {}x{} (must be 64x64 or 64x32)",
                skin_width, skin_height
            )));
        }

        let expected_len = (skin_width * skin_height * 4) as usize;
        if skin_rgba.len() < expected_len {
            return Err(TextureError::Baking(format!(
                "Skin RGBA buffer too small: expected {}, got {}",
                expected_len,
                skin_rgba.len()
            )));
        }

        let base_x = info.pixel_rect[0] as usize;
        let base_y = info.pixel_rect[1] as usize;
        let atlas_stride = (DYNAMIC_ATLAS_WIDTH * 4) as usize;
        let row_bytes = (skin_width * 4) as usize;

        for y in 0..(skin_height as usize) {
            let dst_offset = (base_y + y) * atlas_stride + base_x * 4;
            let src_offset = y * row_bytes;
            self.rgba_buffer[dst_offset..dst_offset + row_bytes]
                .copy_from_slice(&skin_rgba[src_offset..src_offset + row_bytes]);
        }

        // If legacy 64x32 skin, clear or mirror lower 32 rows if needed (leaving transparent)
        if skin_height == 32 {
            for y in 32..64 {
                let dst_offset = (base_y + y) * atlas_stride + base_x * 4;
                self.rgba_buffer[dst_offset..dst_offset + row_bytes].fill(0);
            }
        }

        Ok(info)
    }

    /// Decodes a PNG image from memory and blits it into the specified slot.
    pub fn blit_skin_png_bytes(
        &mut self,
        slot_idx: u16,
        png_bytes: &[u8],
    ) -> Result<DynamicSlotInfo, TextureError> {
        let dyn_img = image::load_from_memory(png_bytes).map_err(TextureError::Image)?;
        let rgba = dyn_img.to_rgba8();
        self.blit_skin_rgba(slot_idx, rgba.as_raw(), rgba.width(), rgba.height())
    }

    /// Associates an alias key with an already allocated slot or another key.
    pub fn add_alias(&mut self, alias_key: &str, target_key: &str) {
        if let Some(&slot_idx) = self.slot_map.get(target_key) {
            self.slot_map.insert(alias_key.to_string(), slot_idx);
        }
    }

    /// Convenience helper to allocate for primary hash, map aliases to that slot, and blit the skin PNG.
    pub fn add_skin_png_with_aliases(
        &mut self,
        primary_key: &str,
        aliases: &[&str],
        png_bytes: &[u8],
    ) -> Result<DynamicSlotInfo, TextureError> {
        let slot_idx = self.allocate_slot(primary_key);
        for &alias in aliases {
            if !alias.is_empty() {
                self.slot_map.insert(alias.to_string(), slot_idx);
            }
        }
        self.blit_skin_png_bytes(slot_idx, png_bytes)
    }

    /// Registers all currently allocated slots into the given `AtlasAddressMap`.
    pub fn register_to_address_map(&self, address_map: &mut AtlasAddressMap) {
        // Ensure the dynamic chunk meta is registered
        if !address_map
            .chunks
            .iter()
            .any(|c| c.chunk_id == self.chunk_id)
        {
            address_map.chunks.push(self.to_chunk_meta());
        }

        for (skin_key, &slot_idx) in &self.slot_map {
            let info = Self::slot_info(slot_idx);
            let loc = AtlasSpriteLocation {
                chunk_id: self.chunk_id,
                category: "entities".to_string(),
                is_animated: false,
                sprite_kind: SpriteKind::StaticAtlas,
                texture_id: 0,
                uv_bounds: info.uv_bounds,
                frame_0_uv_bounds: info.uv_bounds,
                local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                frame_uv_step: [0.0, 0.0],
                pixel_rect: info.pixel_rect,
                strip_pixel_rect: info.pixel_rect,
                frame_size: [DYNAMIC_SLOT_SIZE, DYNAMIC_SLOT_SIZE],
                frame_count: 1,
                animation: None,
                has_normal: false,
                has_specular: false,
                has_overlay: false,
                average_color: None,
                is_opaque: true,
                slot_index: Some(slot_idx),
            };

            let key_locs = mtk_resource::TextureKeyVariants::resource_locations_for_key(skin_key);

            for loc_key in key_locs {
                address_map.insert_with_source(
                    loc_key,
                    loc.clone(),
                    crate::atlas::AtlasKeySource::Dynamic,
                );
            }
        }
    }

    /// Produces AtlasChunkMeta representation for Blender DCC material binding.
    pub fn to_chunk_meta(&self) -> AtlasChunkMeta {
        let category_chunk_index = (self
            .chunk_id
            .saturating_sub(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
            + 1) as usize;
        let mut meta = AtlasChunkMeta {
            chunk_id: self.chunk_id,
            category: "entities".to_string(),
            is_animated: false,
            category_chunk_index,
            width: self.width,
            height: self.height,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        meta.refresh_textures();
        meta
    }
}
