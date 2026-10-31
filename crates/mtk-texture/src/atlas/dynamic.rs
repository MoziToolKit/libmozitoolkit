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
pub const DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID: u16 = 0xFFF0;

/// Maximum number of dynamic chunks supported (0xFFF0..=0xFFFF, 16 chunks = 4,096 unique 64x64 skins).
pub const MAX_DYNAMIC_CHUNKS: usize = 16;
pub const DYNAMIC_CHUNK_START: u16 = 0xFFF0;
pub const DYNAMIC_CHUNK_END: u16 = 0xFFFF;

/// Information about an allocated slot inside a dynamic atlas chunk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicSlotInfo {
    pub slot_index: u16,
    pub col: u32,
    pub row: u32,
    pub pixel_rect: [u32; 4],
    pub uv_bounds: [f32; 4],
}

/// 16 Canonical Vanilla Dye colors for banner rendering ([name, [R, G, B]]).
pub const VANILLA_BANNER_DYES: [(&str, [u8; 3]); 16] = [
    ("white", [249, 255, 254]),
    ("orange", [249, 128, 29]),
    ("magenta", [199, 78, 189]),
    ("light_blue", [58, 179, 218]),
    ("yellow", [254, 216, 61]),
    ("lime", [128, 199, 31]),
    ("pink", [243, 139, 170]),
    ("gray", [71, 79, 82]),
    ("light_gray", [157, 157, 151]),
    ("cyan", [22, 156, 156]),
    ("purple", [137, 50, 184]),
    ("blue", [60, 68, 170]),
    ("brown", [131, 84, 50]),
    ("green", [94, 124, 22]),
    ("red", [176, 46, 38]),
    ("black", [29, 29, 33]),
];

/// Resolves a dye name or integer index string to canonical sRGB [R, G, B].
pub fn dye_color_to_rgb(dye_name_or_id: &str) -> [u8; 3] {
    if let Ok(id) = dye_name_or_id.parse::<usize>() {
        if id < VANILLA_BANNER_DYES.len() {
            return VANILLA_BANNER_DYES[id].1;
        }
    }
    let clean = dye_name_or_id
        .strip_prefix("minecraft:")
        .unwrap_or(dye_name_or_id);
    let stem = clean.strip_suffix("_dye").unwrap_or(clean);
    VANILLA_BANNER_DYES
        .iter()
        .find(|(n, _)| *n == stem)
        .map(|(_, rgb)| *rgb)
        .unwrap_or(VANILLA_BANNER_DYES[0].1)
}

/// Normalizes 2-letter codes or asset stems to canonical banner pattern names.
pub fn normalize_pattern_stem(code_or_name: &str) -> &'static str {
    let clean = code_or_name
        .strip_prefix("minecraft:")
        .unwrap_or(code_or_name);
    let stem = clean.strip_prefix("entity/banner/").unwrap_or(clean);
    let stem = stem.strip_prefix("banner/").unwrap_or(stem);
    let stem = stem.strip_prefix("pattern_").unwrap_or(stem);

    match stem {
        "b" | "base" | "banner_base" => "base",
        "bs" | "stripe_bottom" => "stripe_bottom",
        "ts" | "stripe_top" => "stripe_top",
        "ls" | "stripe_left" => "stripe_left",
        "rs" | "stripe_right" => "stripe_right",
        "cs" | "stripe_center" => "stripe_center",
        "ms" | "stripe_middle" => "stripe_middle",
        "drs" | "stripe_downright" => "stripe_downright",
        "dls" | "stripe_downleft" => "stripe_downleft",
        "ss" | "small_stripes" => "small_stripes",
        "cr" | "cross" => "cross",
        "sc" | "straight_cross" => "straight_cross",
        "ld" | "diagonal_left" => "diagonal_left",
        "rud" | "diagonal_right" => "diagonal_right",
        "lud" | "diagonal_up_left" => "diagonal_up_left",
        "rd" | "diagonal_up_right" => "diagonal_up_right",
        "vh" | "half_vertical" => "half_vertical",
        "vhr" | "half_vertical_right" => "half_vertical_right",
        "hh" | "half_horizontal" => "half_horizontal",
        "hhb" | "half_horizontal_bottom" => "half_horizontal_bottom",
        "bl" | "square_bottom_left" => "square_bottom_left",
        "br" | "square_bottom_right" => "square_bottom_right",
        "tl" | "square_top_left" => "square_top_left",
        "tr" | "square_top_right" => "square_top_right",
        "bt" | "triangle_bottom" => "triangle_bottom",
        "tt" | "triangle_top" => "triangle_top",
        "bts" | "triangles_bottom" => "triangles_bottom",
        "tts" | "triangles_top" => "triangles_top",
        "mc" | "circle" => "circle",
        "mr" | "rhombus" => "rhombus",
        "bo" | "border" => "border",
        "cbo" | "curly_border" => "curly_border",
        "bri" | "bricks" => "bricks",
        "gra" | "gradient" => "gradient",
        "gru" | "gradient_up" => "gradient_up",
        "cre" | "creeper" => "creeper",
        "sku" | "skull" => "skull",
        "flo" | "flower" => "flower",
        "moj" | "mojang" => "mojang",
        "glb" | "globe" => "globe",
        "pig" | "piglin" => "piglin",
        "flw" | "flow" => "flow",
        "gus" | "guster" => "guster",
        _ => "base",
    }
}

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
    pub fn create_fallback_banner_base() -> crate::image::buffer::RgbaBuffer {
        let mut buf = crate::image::buffer::RgbaBuffer::new(64, 64);
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
        base_cloth: Option<&crate::image::buffer::RgbaBuffer>,
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
            };

            let mut variants = vec![
                format!("minecraft:skin/{}", skin_key),
                format!("skin/{}", skin_key),
                format!("minecraft:player/{}", skin_key),
                format!("player/{}", skin_key),
                format!("entity/player/{}", skin_key),
                format!("minecraft:banner/{}", skin_key),
                format!("banner/{}", skin_key),
                format!("minecraft:entity/banner/{}", skin_key),
                format!("entity/banner/{}", skin_key),
                skin_key.clone(),
            ];

            if let Some(stripped) = skin_key.strip_prefix("banner:") {
                variants.push(format!("minecraft:banner/{}", stripped));
                variants.push(format!("banner/{}", stripped));
                variants.push(format!("minecraft:entity/banner/{}", stripped));
                variants.push(format!("entity/banner/{}", stripped));
                variants.push(format!("minecraft:{}_banner", stripped));
                variants.push(format!("{}_banner", stripped));
            }
            if let Some(stripped) = skin_key.strip_prefix("minecraft:banner/") {
                variants.push(format!("banner:{}", stripped));
                variants.push(format!("banner/{}", stripped));
                variants.push(format!("minecraft:{}_banner", stripped));
                variants.push(format!("{}_banner", stripped));
            }

            for v in variants {
                if let Ok(loc_key) = mtk_resource::ResourceLocation::parse(&v) {
                    address_map.sprites.insert(loc_key, loc.clone());
                }
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
        base_cloth: Option<&crate::image::buffer::RgbaBuffer>,
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
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_atlas_allocation_and_deduplication() {
        let mut atlas = DynamicAtlasChunk::new(0xFFF0);
        let s1 = atlas.allocate_slot("alex");
        let s2 = atlas.allocate_slot("steve");
        let s1_again = atlas.allocate_slot("alex");

        assert_eq!(s1, 0);
        assert_eq!(s2, 1);
        assert_eq!(s1_again, 0); // Natural deduplication
        assert_eq!(atlas.allocated_count(), 2);
    }

    #[test]
    fn test_dynamic_atlas_slot_uv_bounds() {
        let info0 = DynamicAtlasChunk::slot_info(0);
        assert_eq!(info0.col, 0);
        assert_eq!(info0.row, 0);
        assert_eq!(info0.pixel_rect, [0, 0, 64, 64]);
        assert_eq!(info0.uv_bounds, [0.0, 0.9375, 0.0625, 1.0]);

        let info17 = DynamicAtlasChunk::slot_info(17); // row 1, col 1
        assert_eq!(info17.col, 1);
        assert_eq!(info17.row, 1);
        assert_eq!(info17.pixel_rect, [64, 64, 64, 64]);
        assert_eq!(info17.uv_bounds, [0.0625, 0.875, 0.125, 0.9375]);
    }

    #[test]
    fn test_dynamic_atlas_blit_and_address_map() {
        let mut atlas = DynamicAtlasChunk::new(0xFFF0);
        let dummy_skin_rgba = vec![255u8; 64 * 64 * 4];
        let info = atlas
            .blit_skin_rgba(0, &dummy_skin_rgba, 64, 64)
            .expect("Blit should succeed");

        assert_eq!(info.slot_index, 0);
        atlas.slot_map.insert("player_test".to_string(), 0);

        let mut addr_map = AtlasAddressMap::new();
        atlas.register_to_address_map(&mut addr_map);

        let loc = addr_map
            .lookup_str("minecraft:skin/player_test")
            .expect("Sprite must be registered");
        assert_eq!(loc.chunk_id, 0xFFF0);
        assert_eq!(loc.uv_bounds, [0.0, 0.9375, 0.0625, 1.0]);
    }

    #[test]
    fn test_dynamic_atlas_overflow_round_robin() {
        let mut atlas = DynamicAtlasChunk::new(0xFFF0);
        for i in 0..256 {
            let slot = atlas.allocate_slot(&format!("skin_{}", i));
            assert_eq!(slot, i as u16);
        }
        // Slot 256 wraps around to 0
        let slot_256 = atlas.allocate_slot("skin_256");
        assert_eq!(slot_256, 0);
        // Slot 257 wraps around to 1
        let slot_257 = atlas.allocate_slot("skin_257");
        assert_eq!(slot_257, 1);
        // Stale skin_0 should have been purged, skin_256 now owns slot 0
        assert_eq!(atlas.get_slot("skin_256"), Some(0));
        assert_eq!(atlas.get_slot("skin_0"), None);
    }

    #[test]
    fn test_dynamic_atlas_manager_multi_chunk_expansion() {
        let mut mgr = DynamicAtlasManager::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID);
        assert_eq!(mgr.chunk_count(), 1);

        // Fill up chunk 0 (0xFFF0) with 256 skins
        for i in 0..256 {
            let (cid, slot) = mgr.allocate_slot(&format!("player_{}", i));
            assert_eq!(cid, 0xFFF0);
            assert_eq!(slot, i as u16);
        }
        assert_eq!(mgr.allocated_count(), 256);
        assert_eq!(mgr.chunk_count(), 1);

        // 257th skin must automatically trigger allocation of chunk 1 (0xFFF1)
        let (cid_256, slot_256) = mgr.allocate_slot("player_256");
        assert_eq!(cid_256, 0xFFF1);
        assert_eq!(slot_256, 0);
        assert_eq!(mgr.chunk_count(), 2);
        assert_eq!(mgr.allocated_count(), 257);

        // Deduplication works across chunks
        let (re_cid, re_slot) = mgr.allocate_slot("player_0");
        assert_eq!(re_cid, 0xFFF0);
        assert_eq!(re_slot, 0);

        let (re_cid_256, re_slot_256) = mgr.allocate_slot("player_256");
        assert_eq!(re_cid_256, 0xFFF1);
        assert_eq!(re_slot_256, 0);

        // Address map registration registers both chunks and correct sprite locations
        let mut addr_map = AtlasAddressMap::new();
        mgr.register_to_address_map(&mut addr_map);

        assert_eq!(addr_map.chunks.len(), 2);
        assert_eq!(addr_map.chunks[0].chunk_id, 0xFFF0);
        assert_eq!(addr_map.chunks[0].category_chunk_index, 1);
        assert_eq!(addr_map.chunks[1].chunk_id, 0xFFF1);
        assert_eq!(addr_map.chunks[1].category_chunk_index, 2);

        let loc0 = addr_map
            .lookup_str("minecraft:skin/player_0")
            .expect("player_0 must be in address map");
        assert_eq!(loc0.chunk_id, 0xFFF0);

        let loc256 = addr_map
            .lookup_str("minecraft:skin/player_256")
            .expect("player_256 must be in address map");
        assert_eq!(loc256.chunk_id, 0xFFF1);
    }

    #[test]
    fn test_dynamic_atlas_manager_dirty_tracking() {
        let mut mgr = DynamicAtlasManager::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID);
        assert_eq!(mgr.dirty_chunk_ids().len(), 0);

        let dummy_skin = vec![128u8; 64 * 64 * 4];
        let (cid, slot) = mgr.allocate_slot("steve");
        let _ = mgr.blit_skin_rgba(cid, slot, &dummy_skin, 64, 64).unwrap();

        assert!(mgr.dirty_chunk_ids().contains(&0xFFF0));
        assert_eq!(mgr.dirty_chunk_buffers().len(), 1);

        mgr.clear_dirty();
        assert_eq!(mgr.dirty_chunk_ids().len(), 0);
    }

    #[test]
    fn test_dynamic_atlas_prewarm_vanilla_banners() {
        let mut mgr = DynamicAtlasManager::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID);
        mgr.prewarm_vanilla_banners(None)
            .expect("Prewarm must succeed");

        assert_eq!(mgr.allocated_count(), 16);
        assert!(mgr.contains_key("banner:white"));
        assert!(mgr.contains_key("banner:red"));
        assert!(mgr.contains_key("banner:black"));
        assert!(mgr.contains_key("minecraft:red_banner"));

        // Red is index 14
        let red_loc = mgr
            .get_location("banner:red")
            .expect("red banner must exist");
        assert_eq!(red_loc, (0xFFF0, 14));

        // Primary rgba buffer is populated and has red pixels in slot 14
        let buf = mgr.primary_rgba_buffer().expect("Must have primary buffer");
        assert_eq!(buf.len(), 1024 * 1024 * 4);

        // Next allocated slot should be 16
        let (cid, slot) = mgr.allocate_slot("custom_player");
        assert_eq!(cid, 0xFFF0);
        assert_eq!(slot, 16);

        // Address map registration resolves banner keys
        let mut addr_map = AtlasAddressMap::new();
        mgr.register_to_address_map(&mut addr_map);

        let loc = addr_map
            .lookup_str("minecraft:banner/red")
            .expect("banner/red in map");
        assert_eq!(loc.chunk_id, 0xFFF0);
        assert_eq!(loc.pixel_rect, DynamicAtlasChunk::slot_info(14).pixel_rect);
    }

    #[test]
    fn test_dynamic_atlas_pinned_slots_overflow_protection() {
        let mut chunk = DynamicAtlasChunk::new(0xFFF0);
        chunk.prewarm_vanilla_banners(None).unwrap();
        assert_eq!(chunk.pinned_slots, 16);
        assert_eq!(chunk.allocated_count(), 16);

        // Fill slots from 16 to 255 (240 dynamic slots)
        for i in 16..256 {
            let slot = chunk.allocate_slot(&format!("entity_{}", i));
            assert_eq!(slot, i as u16);
        }

        // Slot 256 should wrap around to 16, NEVER to 0..15!
        let wrap_slot = chunk.allocate_slot("overflow_1");
        assert_eq!(wrap_slot, 16);

        // Pinned banner:red (slot 14) and banner:white (slot 0) must remain intact!
        assert_eq!(chunk.get_slot("banner:red"), Some(14));
        assert_eq!(chunk.get_slot("banner:white"), Some(0));

        // Slot 16 old occupant was evicted, overflow_1 owns it
        assert_eq!(chunk.get_slot("entity_16"), None);
        assert_eq!(chunk.get_slot("overflow_1"), Some(16));
    }
}
