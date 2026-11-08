//! Dynamic Atlas Chunk for runtime-registered entities (such as player skulls, banners, and paintings).
//!
//! Provides fixed-grid slot allocation (e.g. 1024x1024 atlas accommodating 256 unique 64x64 skins),
//! fast sub-image blitting, automatic UV bounds computation, and integration with `AtlasAddressMap`.

use std::collections::{HashMap, HashSet};

use crate::atlas::address_map::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, SpriteKind};
use crate::error::TextureError;

/// Standard dimensions of a dynamic skin atlas chunk.
pub const DYNAMIC_ATLAS_WIDTH: u32 = 1024;
pub const DYNAMIC_ATLAS_HEIGHT: u32 = 1024;
pub const DYNAMIC_SLOT_SIZE: u32 = 64;
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

use mtk_resource::banner::VANILLA_BANNER_DYES;

/// A fixed-size runtime dynamic atlas chunk that allocates 64x64 grid slots on demand.
#[derive(Debug, Clone)]
pub struct DynamicAtlasChunk {
    pub chunk_id: u16,
    pub width: u32,
    pub height: u32,
    pub rgba_buffer: Vec<u8>,
    pub pinned_slots: u16,
    slot_map: HashMap<String, u16>,
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

    /// Convenience helper to allocate and blit a skin in one step.
    pub fn add_skin_png(
        &mut self,
        skin_key: &str,
        png_bytes: &[u8],
    ) -> Result<DynamicSlotInfo, TextureError> {
        let slot_idx = self.allocate_slot(skin_key);
        self.blit_skin_png_bytes(slot_idx, png_bytes)
    }

    /// Registers all currently allocated slots into the given `AtlasAddressMap`.
    ///
    /// Maps both `minecraft:skin/<key>` and direct `<key>` aliases so that
    /// both ModelBaker textures and Direct texture identifiers resolve instantly.
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

            // Register dynamic entity aliases. Static atlas entries take strict precedence;
            // if any alias already resolves to a static chunk, insert_with_source retains the
            // static sprite while registering all non-conflicting dynamic aliases.
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

/// Multi-chunk manager for runtime dynamic skin atlases.
///
/// Automatically allocates new chunks (0xFFF0, 0xFFF1, ...) as slots are exhausted (256 slots per chunk),
/// deduplicates skin keys, tracks aliases and dirty states, and registers all active chunks to `AtlasAddressMap`.
#[derive(Debug, Clone)]
pub struct DynamicAtlasManager {
    base_chunk_id: u16,
    chunks: Vec<DynamicAtlasChunk>,
    key_to_location: HashMap<String, (u16, u16)>,
    dirty_chunks: HashSet<u16>,
}

impl Default for DynamicAtlasManager {
    fn default() -> Self {
        Self::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
    }
}

impl DynamicAtlasManager {
    /// Creates a new `DynamicAtlasManager` starting at `base_chunk_id`.
    pub fn new(base_chunk_id: u16) -> Self {
        Self {
            base_chunk_id,
            chunks: vec![DynamicAtlasChunk::new(base_chunk_id)],
            key_to_location: HashMap::new(),
            dirty_chunks: HashSet::new(),
        }
    }

    /// Total number of unique skin slots allocated across all chunks.
    pub fn allocated_count(&self) -> usize {
        self.chunks.iter().map(|c| c.allocated_count()).sum()
    }

    /// Prewarms the primary chunk with the 16 canonical vanilla dye banner textures,
    /// pinning slots 0..15 so they are permanently cached and immune to eviction.
    pub fn prewarm_vanilla_banners(
        &mut self,
        base_cloth: Option<&crate::RgbaBuffer>,
    ) -> Result<(), TextureError> {
        if self.chunks.is_empty() {
            self.chunks.push(DynamicAtlasChunk::new(self.base_chunk_id));
        }
        let chunk = &mut self.chunks[0];
        chunk.prewarm_vanilla_banners(base_cloth)?;
        let chunk_id = chunk.chunk_id;
        for (key, &slot) in &chunk.slot_map {
            self.key_to_location.insert(key.clone(), (chunk_id, slot));
        }
        self.dirty_chunks.insert(chunk_id);
        Ok(())
    }

    /// Number of chunks currently instantiated.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Checks if a skin key has already been registered.
    pub fn contains_key(&self, skin_key: &str) -> bool {
        self.key_to_location.contains_key(skin_key)
    }

    /// Returns reference to the entire key -> (chunk_id, slot_idx) mapping table.
    pub fn key_to_location(&self) -> &HashMap<String, (u16, u16)> {
        &self.key_to_location
    }

    /// Returns a complete map of all allocated primary keys AND alias keys to their (chunk_id, slot_idx).
    pub fn all_slot_locations(&self) -> HashMap<String, (u16, u16)> {
        let mut map = self.key_to_location.clone();
        for chunk in &self.chunks {
            let cid = chunk.chunk_id;
            for (key, &slot) in &chunk.slot_map {
                map.entry(key.clone()).or_insert((cid, slot));
            }
        }
        map
    }

    /// Returns the (chunk_id, slot_idx) for a skin key, if allocated.
    pub fn get_location(&self, skin_key: &str) -> Option<(u16, u16)> {
        self.key_to_location.get(skin_key).copied()
    }

    /// Allocates or retrieves an existing slot for a skin key.
    ///
    /// Automatically expands to a new chunk when the current chunk reaches 256 slots.
    /// Returns `(chunk_id, slot_idx)`.
    pub fn allocate_slot(&mut self, skin_key: &str) -> (u16, u16) {
        if let Some(&loc) = self.key_to_location.get(skin_key) {
            return loc;
        }

        let active_idx = self.chunks.len().saturating_sub(1);
        let active_full = self.chunks[active_idx].allocated_count() >= DYNAMIC_TOTAL_SLOTS;

        let target_idx = if active_full {
            if self.chunks.len() < MAX_DYNAMIC_CHUNKS {
                let next_chunk_id = self.base_chunk_id + self.chunks.len() as u16;
                self.chunks.push(DynamicAtlasChunk::new(next_chunk_id));
                self.chunks.len() - 1
            } else {
                active_idx
            }
        } else {
            active_idx
        };

        let chunk = &mut self.chunks[target_idx];
        let slot_idx = chunk.allocate_slot(skin_key);
        let chunk_id = chunk.chunk_id;

        self.key_to_location
            .insert(skin_key.to_string(), (chunk_id, slot_idx));
        self.dirty_chunks.insert(chunk_id);
        (chunk_id, slot_idx)
    }

    /// Blits a raw RGBA skin buffer into the allocated slot of a specific chunk.
    pub fn blit_skin_rgba(
        &mut self,
        chunk_id: u16,
        slot_idx: u16,
        skin_rgba: &[u8],
        skin_width: u32,
        skin_height: u32,
    ) -> Result<DynamicSlotInfo, TextureError> {
        let chunk = self
            .chunks
            .iter_mut()
            .find(|c| c.chunk_id == chunk_id)
            .ok_or_else(|| TextureError::Baking(format!("Dynamic chunk {} not found", chunk_id)))?;
        let info = chunk.blit_skin_rgba(slot_idx, skin_rgba, skin_width, skin_height)?;
        self.dirty_chunks.insert(chunk_id);
        Ok(info)
    }

    /// Decodes and blits a PNG skin into the specified chunk and slot.
    pub fn blit_skin_png_bytes(
        &mut self,
        chunk_id: u16,
        slot_idx: u16,
        png_bytes: &[u8],
    ) -> Result<DynamicSlotInfo, TextureError> {
        let dyn_img = image::load_from_memory(png_bytes).map_err(TextureError::Image)?;
        let rgba = dyn_img.to_rgba8();
        self.blit_skin_rgba(
            chunk_id,
            slot_idx,
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
        )
    }

    /// Allocates and blits a skin PNG in one call.
    pub fn add_skin_png(
        &mut self,
        skin_key: &str,
        png_bytes: &[u8],
    ) -> Result<(u16, DynamicSlotInfo), TextureError> {
        let (chunk_id, slot_idx) = self.allocate_slot(skin_key);
        let info = self.blit_skin_png_bytes(chunk_id, slot_idx, png_bytes)?;
        Ok((chunk_id, info))
    }

    /// Associates an alias key with an already allocated skin key.
    pub fn add_alias(&mut self, alias_key: &str, target_key: &str) {
        if let Some(&(chunk_id, slot_idx)) = self.key_to_location.get(target_key) {
            self.key_to_location
                .insert(alias_key.to_string(), (chunk_id, slot_idx));
            if let Some(chunk) = self.chunks.iter_mut().find(|c| c.chunk_id == chunk_id) {
                chunk.add_alias(alias_key, target_key);
            }
        }
    }

    /// Allocates, maps aliases, and blits a skin PNG.
    pub fn add_skin_png_with_aliases(
        &mut self,
        primary_key: &str,
        aliases: &[&str],
        png_bytes: &[u8],
    ) -> Result<(u16, DynamicSlotInfo), TextureError> {
        let (chunk_id, slot_idx) = self.allocate_slot(primary_key);
        for &alias in aliases {
            if !alias.is_empty() {
                self.key_to_location
                    .insert(alias.to_string(), (chunk_id, slot_idx));
            }
        }
        if let Some(chunk) = self.chunks.iter_mut().find(|c| c.chunk_id == chunk_id) {
            for &alias in aliases {
                if !alias.is_empty() {
                    chunk.slot_map.insert(alias.to_string(), slot_idx);
                }
            }
        }
        let info = self.blit_skin_png_bytes(chunk_id, slot_idx, png_bytes)?;
        Ok((chunk_id, info))
    }

    /// Registers all chunks and sprites into the given `AtlasAddressMap`.
    pub fn register_to_address_map(&self, address_map: &mut AtlasAddressMap) {
        for chunk in &self.chunks {
            chunk.register_to_address_map(address_map);
        }
    }

    /// Returns the RGBA bytes of the primary chunk (0xFFF0) if any skins allocated.
    pub fn primary_rgba_buffer(&self) -> Option<&[u8]> {
        if self.allocated_count() > 0 {
            self.chunks.first().map(|c| c.rgba_buffer.as_slice())
        } else {
            None
        }
    }

    /// Returns RGBA bytes of a specific dynamic chunk ID if present.
    pub fn chunk_rgba_buffer(&self, chunk_id: u16) -> Option<&[u8]> {
        self.chunks
            .iter()
            .find(|c| c.chunk_id == chunk_id)
            .map(|c| c.rgba_buffer.as_slice())
    }

    /// Returns all allocated chunk IDs and their RGBA byte buffers.
    pub fn all_chunk_buffers(&self) -> Vec<(u16, &[u8])> {
        self.chunks
            .iter()
            .filter(|c| c.allocated_count() > 0)
            .map(|c| (c.chunk_id, c.rgba_buffer.as_slice()))
            .collect()
    }

    /// Returns owned copy of all allocated chunk buffers as a HashMap.
    pub fn all_chunk_buffers_owned(&self) -> HashMap<u16, Vec<u8>> {
        self.chunks
            .iter()
            .filter(|c| c.allocated_count() > 0)
            .map(|c| (c.chunk_id, c.rgba_buffer.clone()))
            .collect()
    }

    /// Returns dirty chunk IDs and their RGBA byte buffers.
    pub fn dirty_chunk_buffers(&self) -> Vec<(u16, &[u8])> {
        self.chunks
            .iter()
            .filter(|c| self.dirty_chunks.contains(&c.chunk_id))
            .map(|c| (c.chunk_id, c.rgba_buffer.as_slice()))
            .collect()
    }

    /// Returns reference to the set of dirty chunk IDs.
    pub fn dirty_chunk_ids(&self) -> &HashSet<u16> {
        &self.dirty_chunks
    }

    /// Clears the dirty chunk tracking set.
    pub fn clear_dirty(&mut self) {
        self.dirty_chunks.clear();
    }

    /// Returns immutable slice of all active dynamic chunks.
    pub fn chunks(&self) -> &[DynamicAtlasChunk] {
        &self.chunks
    }
}

#[cfg(test)]
#[path = "dynamic_tests.rs"]
mod tests;
