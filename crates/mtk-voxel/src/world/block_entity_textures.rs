//! # Runtime block-entity textures are a host concern
//!
//! Player-head skins and composite banners need network access, a disk cache and procedural
//! image baking. `mtk-voxel` (L0 in `ARCHITECTURE_2.0`) must not own any of that: it only needs
//! the sprites to exist in the atlas address map before meshing, and the RGBA chunks afterwards
//! for whatever the host binds them to.
//!
//! Hosts therefore inject a [`BlockEntityTextureProvider`]. `mtk-block-entity-textures` ships the
//! production implementation; tests can inject a deterministic fake, and builds that do not need
//! runtime textures simply leave the slot empty.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use glam::IVec3;

use crate::storage::VoxelStorage;
use crate::world::VoxelWorld;

/// Whether texture resolution may block on the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureResolveMode {
    /// Use the in-memory/disk cache only; anything uncached is fetched in the background.
    Cached,
    /// Block on the network for uncached entries (save import, offline CLI).
    Blocking,
}

/// Wrapper for skin-resolution callbacks that satisfies Debug and Clone.
#[derive(Clone)]
pub struct SkinResolvedCallback(pub Arc<dyn Fn(String) + Send + Sync>);

impl std::fmt::Debug for SkinResolvedCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SkinResolvedCallback(...)")
    }
}

impl SkinResolvedCallback {
    /// Invokes the callback with the finished skin key.
    pub fn call(&self, key: String) {
        (self.0)(key);
    }
}

/// Result of one [`BlockEntityTextureProvider::prepare`] pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TexturePrepareOutcome {
    /// New sprites were allocated (first-time skull skin, composite banner, refreshed skin).
    pub added: bool,
    /// The provider's sprite set changed and should be merged into the host's atlas map.
    pub atlas_changed: bool,
}

/// Host-supplied source of runtime textures for block entities.
///
/// One implementation is expected per world. It owns its cache and atlas allocations; the world
/// only calls [`prepare`](Self::prepare) before meshing and reads the results afterwards.
pub trait BlockEntityTextureProvider: Send + Sync {
    /// Resolves textures for the block entities in `sections` (every section when `None`),
    /// registers the provider's sprites into `atlas`, and marks affected sections dirty.
    fn prepare(
        &mut self,
        storage: &mut VoxelStorage,
        atlas: &mut mtk_texture::AtlasAddressMap,
        sections: Option<&[IVec3]>,
        mode: TextureResolveMode,
        progress: Option<mtk_core::ProgressCallback>,
    ) -> TexturePrepareOutcome;

    /// RGBA atlas chunks (`chunk_id -> 1024x1024x4 bytes`) currently allocated.
    fn atlases(&self) -> Vec<(u16, &[u8])>;

    /// Key (and alias) to `(chunk_id, slot)` mapping for host material binding.
    fn slots(&self) -> HashMap<String, (u16, u16)>;

    /// Sets the on-disk cache directory used for downloaded player skins.
    fn set_skin_cache_dir(&mut self, cache_dir: Option<PathBuf>);

    /// Enables or disables outbound network requests for block-entity textures.
    fn set_allow_network(&mut self, allow_network: bool);

    /// Installs the procedural banner baker used for composite banner patterns.
    fn set_banner_baker(&mut self, baker: mtk_texture::BannerBaker);

    /// Registers a callback fired when a background skin download completes.
    fn set_skin_resolved_callback(&mut self, callback: SkinResolvedCallback);

    /// Registers an in-memory player skin PNG payload keyed by skin key/hash.
    fn register_memory_skin(&mut self, _key: &str, _png: Vec<u8>) {}

    /// Marks previously merged atlas entries as stale so the next [`prepare`](Self::prepare)
    /// re-registers the provider's dynamic slots into a replacement `AtlasAddressMap`.
    fn invalidate_atlas_registration(&mut self) {}

    /// Owned copy of [`Self::atlases`], for hosts that need to hand the pixels over.
    fn atlases_owned(&self) -> HashMap<u16, Vec<u8>> {
        self.atlases()
            .into_iter()
            .map(|(chunk_id, bytes)| (chunk_id, bytes.to_vec()))
            .collect()
    }
}

/// Read-only accessors and configuration forwarders.
///
/// These keep the host-facing shape that existed before the provider was extracted (the Python
/// binding and the live-sync session call them), while the implementation — and the network,
/// disk cache and banner baking behind it — lives in the injected provider.
impl VoxelWorld {
    /// RGBA bytes of the primary dynamic chunk (`0xFFF0`), when one exists.
    pub fn dynamic_entity_atlas_rgba(&self) -> Option<&[u8]> {
        let primary = mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
        let textures = self.block_textures.as_ref()?;
        textures
            .atlases()
            .into_iter()
            .find(|(chunk_id, _)| *chunk_id == primary)
            .map(|(_, bytes)| bytes)
    }

    /// All allocated dynamic chunks as borrowed `(chunk_id, RGBA bytes)` pairs.
    pub fn dynamic_entity_atlases(&self) -> Vec<(u16, &[u8])> {
        self.block_textures
            .as_ref()
            .map(|textures| textures.atlases())
            .unwrap_or_default()
    }

    /// All allocated dynamic chunks as owned `(chunk_id, RGBA bytes)` pairs.
    pub fn dynamic_entity_atlases_owned(&self) -> HashMap<u16, Vec<u8>> {
        self.block_textures
            .as_ref()
            .map(|textures| textures.atlases_owned())
            .unwrap_or_default()
    }

    /// Key/alias to `(chunk_id, slot)` mapping, for host material binding.
    pub fn dynamic_entity_slots(&self) -> HashMap<String, (u16, u16)> {
        self.block_textures
            .as_ref()
            .map(|textures| textures.slots())
            .unwrap_or_default()
    }

    /// Returns true if the assembled world mesh references any dynamic atlas chunk.
    #[inline]
    pub fn has_dynamic_entity_chunks(&self) -> bool {
        self.used_chunk_ids()
            .iter()
            .any(|&chunk| chunk >= mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID as u32)
    }

    /// Dynamic atlases and slots, but only when the world mesh actually references them.
    pub fn dynamic_entity_atlases_if_used(
        &self,
    ) -> (
        Option<HashMap<u16, Vec<u8>>>,
        Option<HashMap<String, (u16, u16)>>,
    ) {
        if self.has_dynamic_entity_chunks() {
            (
                Some(self.dynamic_entity_atlases_owned()),
                Some(self.dynamic_entity_slots()),
            )
        } else {
            (None, None)
        }
    }

    /// Sets the on-disk cache directory for downloaded player skins.
    pub fn set_skin_cache_dir(&mut self, cache_dir: Option<std::path::PathBuf>) {
        if let Some(textures) = self.block_textures.as_mut() {
            textures.set_skin_cache_dir(cache_dir);
        }
    }

    /// Enables or disables outbound network requests for block-entity textures.
    pub fn set_allow_network(&mut self, allow_network: bool) {
        if let Some(textures) = self.block_textures.as_mut() {
            textures.set_allow_network(allow_network);
        }
    }

    /// Sets the callback triggered when a background skin download finishes.
    pub fn set_skin_resolved_callback<F>(&mut self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        if let Some(textures) = self.block_textures.as_mut() {
            textures.set_skin_resolved_callback(SkinResolvedCallback(Arc::new(callback)));
        }
    }

    /// Injects a decoded/downloaded player skin PNG directly into the runtime texture cache.
    pub fn register_memory_skin(&mut self, key: &str, png: Vec<u8>) {
        if let Some(textures) = self.block_textures.as_mut() {
            textures.register_memory_skin(key, png);
        }
    }

    /// Cache-only resolution over every section (background fetches anything uncached).
    pub fn ensure_dynamic_entities_resolved(&mut self) -> bool {
        self.prepare_block_entity_textures(None, TextureResolveMode::Cached, None)
    }

    /// Cache-only resolution over the given sections.
    pub fn ensure_dynamic_entities_resolved_for_sections(&mut self, coords: &[IVec3]) -> bool {
        self.prepare_block_entity_textures(Some(coords), TextureResolveMode::Cached, None)
    }

    /// Blocking resolution over every section.
    pub fn ensure_dynamic_entities_resolved_sync(&mut self) -> bool {
        self.prepare_block_entity_textures(None, TextureResolveMode::Blocking, None)
    }

    /// Blocking resolution over every section with progress reporting.
    pub fn ensure_dynamic_entities_resolved_sync_with_progress(
        &mut self,
        progress: Option<mtk_core::ProgressCallback>,
    ) -> bool {
        self.prepare_block_entity_textures(None, TextureResolveMode::Blocking, progress)
    }

    /// Blocking resolution over the given sections with progress reporting.
    pub fn ensure_dynamic_entities_resolved_sync_for_sections_with_progress(
        &mut self,
        coords: &[IVec3],
        progress: Option<mtk_core::ProgressCallback>,
    ) -> bool {
        self.prepare_block_entity_textures(Some(coords), TextureResolveMode::Blocking, progress)
    }
}
