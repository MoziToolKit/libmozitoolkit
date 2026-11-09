//! Multi-chunk manager for runtime dynamic skin atlases.

use std::collections::{HashMap, HashSet};

use super::chunk::{
    DynamicAtlasChunk, DynamicSlotInfo, DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID, DYNAMIC_TOTAL_SLOTS,
    MAX_DYNAMIC_CHUNKS,
};
use crate::atlas::address_map::AtlasAddressMap;
use crate::error::TextureError;

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
