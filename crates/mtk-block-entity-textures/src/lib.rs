//! # Runtime textures for block entities
//!
//! Production implementation of [`mtk_voxel::BlockEntityTextureProvider`]: scans block entities
//! (player skulls, composite banners), resolves skins through the memory/disk/network tiers,
//! bakes banner images, and registers the results into a dynamic atlas.
//!
//! This lives outside `mtk-voxel` on purpose: `ARCHITECTURE_2.0` forbids L0 from owning network
//! access, disk caches and texture baking. Hosts (`mtk-py`, the live-sync client) construct this
//! and inject it into a [`VoxelWorld`](mtk_voxel::VoxelWorld).

use std::collections::HashMap;
use std::path::PathBuf;

use glam::IVec3;

use mtk_core::{ProgressCallback, ProgressThrottler};
use mtk_model::skull::SkullKind;
use mtk_model::BlockEntityData;
use mtk_resource::skin::{clean_player_name, clean_uuid, derive_skin_key, SkinResolver};
use mtk_texture::{AtlasAddressMap, BannerBaker, DynamicAtlasManager};
use mtk_voxel::{
    BlockEntityTextureProvider, SkinResolvedCallback, TexturePrepareOutcome, TextureResolveMode,
    VoxelStorage,
};

/// Runtime block-entity textures: dynamic atlas, skin resolver and banner baker.
pub struct BlockEntityTextures {
    manager: DynamicAtlasManager,
    skin_resolver: SkinResolver,
    banner_baker: BannerBaker,
    on_resolved: Option<SkinResolvedCallback>,
    allow_network: bool,
    /// Whether the current atlas allocation has been merged into the world's address map yet.
    /// Pinned banner slots exist from construction, so the first `prepare` must register them
    /// even when no new sprite was added.
    registered: bool,
}

impl Default for BlockEntityTextures {
    fn default() -> Self {
        let mut manager = DynamicAtlasManager::default();
        let baker = BannerBaker::default();
        let _ = manager.prewarm_vanilla_banners(Some(baker.base_cloth()));
        Self {
            manager,
            skin_resolver: SkinResolver::default(),
            banner_baker: baker,
            on_resolved: None,
            allow_network: true,
            registered: false,
        }
    }
}

impl BlockEntityTextures {
    /// Number of allocated dynamic slots (pinned banner slots included).
    pub fn allocated_count(&self) -> usize {
        self.manager.allocated_count()
    }

    /// Number of instantiated dynamic chunks.
    pub fn chunk_count(&self) -> usize {
        self.manager.chunk_count()
    }

    /// Resolves `(chunk_id, slot)` for a key or alias, when allocated.
    pub fn get_location(&self, key: &str) -> Option<(u16, u16)> {
        self.manager.get_location(key)
    }

    /// Registers an in-memory skin by its hash (tests, offline imports).
    pub fn register_memory_skin(&self, hash: &str, png: Vec<u8>) {
        self.skin_resolver.register_memory_skin(hash, png);
    }

    /// Sets the on-disk cache directory for downloaded player skins.
    pub fn set_skin_cache_dir(&mut self, cache_dir: Option<PathBuf>) {
        self.skin_resolver = SkinResolver::new(cache_dir).with_allow_network(self.allow_network);
    }

    /// Enables or disables outbound network requests for player head skins.
    pub fn set_allow_network(&mut self, allow_network: bool) {
        self.allow_network = allow_network;
        self.skin_resolver.set_allow_network(allow_network);
    }

    /// Installs the procedural banner baker used for composite banners.
    pub fn set_banner_baker(&mut self, baker: BannerBaker) {
        let _ = self
            .manager
            .prewarm_vanilla_banners(Some(baker.base_cloth()));
        self.banner_baker = baker;
        self.registered = false;
    }

    /// Registers a callback fired when a background skin download completes.
    pub fn set_skin_resolved_callback(&mut self, callback: SkinResolvedCallback) {
        self.on_resolved = Some(callback);
    }
}

/// One skull that needs a network fetch, grouped by its primary key.
struct UnresolvedSkull {
    primary: String,
    aliases: Vec<String>,
    skin_url: Option<String>,
    uuid: Option<String>,
    name: Option<String>,
    sec_coords: Vec<IVec3>,
}

fn local_to_world(sec_coord: IVec3, local_idx: u16) -> (i32, i32, i32) {
    (
        sec_coord.x * 16 + (((local_idx >> 8) & 15) as i32),
        sec_coord.y * 16 + (((local_idx >> 4) & 15) as i32),
        sec_coord.z * 16 + ((local_idx & 15) as i32),
    )
}

fn collect_skull_aliases(key: &str, name: Option<&str>, uuid: Option<&str>) -> Vec<String> {
    let mut aliases = vec![key.to_string()];
    if let Some(name) = name {
        let clean = clean_player_name(name);
        if !clean.is_empty() {
            aliases.push(format!("player_{clean}"));
            aliases.push(clean);
        }
    }
    if let Some(uuid) = uuid {
        let clean = clean_uuid(uuid);
        if !clean.is_empty() {
            aliases.push(format!("uuid_{clean}"));
            aliases.push(clean);
        }
    }
    aliases
}

fn is_in_static_atlas(atlas: &AtlasAddressMap, key: &str) -> bool {
    atlas
        .resolve_texture_key(key)
        .map(|loc| loc.chunk_id < mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
        .unwrap_or(false)
}

impl BlockEntityTextureProvider for BlockEntityTextures {
    fn prepare(
        &mut self,
        storage: &mut VoxelStorage,
        atlas: &mut AtlasAddressMap,
        sections: Option<&[IVec3]>,
        mode: TextureResolveMode,
        progress: Option<ProgressCallback>,
    ) -> TexturePrepareOutcome {
        let owned_coords;
        let coords: &[IVec3] = match sections {
            Some(coords) => coords,
            None => {
                owned_coords = storage.sections.keys().copied().collect::<Vec<_>>();
                &owned_coords
            }
        };

        let mut newly_added = false;
        let mut dirty_coords: Vec<IVec3> = Vec::new();
        let mut uncached: HashMap<String, UnresolvedSkull> = HashMap::new();

        for &sec_coord in coords {
            let Some(section) = storage.sections.get(&sec_coord) else {
                continue;
            };
            for (&local_idx, entity) in &section.block_entities {
                if storage.has_explicit_bounds {
                    let (wx, wy, wz) = local_to_world(sec_coord, local_idx);
                    if !storage.contains(wx, wy, wz) {
                        continue;
                    }
                }

                match entity {
                    BlockEntityData::Skull { kind, profile, .. } if *kind == SkullKind::Player => {
                        let (skin_url, uuid, name) = match profile {
                            Some(profile) => (
                                profile.skin_url.as_deref(),
                                profile.id.as_deref(),
                                profile.name.as_deref(),
                            ),
                            None => (None, None, None),
                        };
                        if skin_url.is_none() && uuid.is_none() && name.is_none() {
                            continue;
                        }

                        let key = derive_skin_key(skin_url, uuid, name);
                        let hash = self.skin_resolver.resolve_skin_hash(skin_url, uuid, name);
                        let primary = hash.clone().unwrap_or_else(|| key.clone());

                        if is_in_static_atlas(atlas, &primary) || is_in_static_atlas(atlas, &key) {
                            continue;
                        }
                        if self.manager.contains_key(&primary) || self.manager.contains_key(&key) {
                            continue;
                        }

                        if let Some(png) =
                            self.skin_resolver.try_resolve_cached(skin_url, uuid, name)
                        {
                            let aliases = collect_skull_aliases(&key, name, uuid);
                            let alias_refs: Vec<&str> =
                                aliases.iter().map(String::as_str).collect();
                            if self
                                .manager
                                .add_skin_png_with_aliases(&primary, &alias_refs, &png)
                                .is_ok()
                            {
                                newly_added = true;
                                dirty_coords.push(sec_coord);
                            }

                            if mode == TextureResolveMode::Cached {
                                // Keep the cached skin usable now, but refresh it if the remote
                                // texture changed since it was cached.
                                let callback = self.on_resolved.clone();
                                self.skin_resolver.check_and_queue_online_validation(
                                    uuid.map(str::to_string),
                                    name.map(str::to_string),
                                    move |finished, _png| {
                                        if let Some(callback) = &callback {
                                            callback.call(finished);
                                        }
                                    },
                                );
                            }
                        } else {
                            let entry = uncached.entry(primary.clone()).or_insert_with(|| {
                                UnresolvedSkull {
                                    primary,
                                    aliases: collect_skull_aliases(&key, name, uuid),
                                    skin_url: skin_url.map(str::to_string),
                                    uuid: uuid.map(str::to_string),
                                    name: name.map(str::to_string),
                                    sec_coords: Vec::new(),
                                }
                            });
                            entry.sec_coords.push(sec_coord);
                        }
                    }
                    BlockEntityData::Banner {
                        base_color,
                        patterns,
                    } => {
                        let key =
                            mtk_model::banner::banner_texture_identifier(base_color, patterns);
                        if is_in_static_atlas(atlas, &key) || self.manager.contains_key(&key) {
                            continue;
                        }
                        let pattern_pairs: Vec<(&str, &str)> = patterns
                            .iter()
                            .map(|p| (p.pattern.as_str(), p.color.as_str()))
                            .collect();
                        let baked = self.banner_baker.bake(base_color, &pattern_pairs);
                        let (chunk_id, slot) = self.manager.allocate_slot(&key);
                        if self
                            .manager
                            .blit_skin_rgba(
                                chunk_id,
                                slot,
                                &baked.pixels,
                                baked.width,
                                baked.height,
                            )
                            .is_ok()
                        {
                            newly_added = true;
                            dirty_coords.push(sec_coord);
                        }
                    }
                    _ => {}
                }
            }
        }

        if !uncached.is_empty() {
            match mode {
                TextureResolveMode::Blocking => {
                    let total = uncached.len();
                    let throttler =
                        ProgressThrottler::new("resolving_player_skins", total, progress)
                            .with_prefix("Resolving player head skins");
                    let items: Vec<UnresolvedSkull> = uncached.into_values().collect();
                    let resolver = self.skin_resolver.clone();

                    #[cfg(feature = "parallel")]
                    let fetched: Vec<(UnresolvedSkull, Option<Vec<u8>>)> = {
                        use rayon::prelude::*;
                        mtk_core::threading::install(|| {
                            items
                                .into_par_iter()
                                .map(|item| {
                                    let png = resolver.resolve_skin_png(
                                        item.skin_url.as_deref(),
                                        item.uuid.as_deref(),
                                        item.name.as_deref(),
                                    );
                                    throttler.inc();
                                    (item, png)
                                })
                                .collect()
                        })
                    };

                    #[cfg(not(feature = "parallel"))]
                    let fetched: Vec<(UnresolvedSkull, Option<Vec<u8>>)> = items
                        .into_iter()
                        .map(|item| {
                            let png = resolver.resolve_skin_png(
                                item.skin_url.as_deref(),
                                item.uuid.as_deref(),
                                item.name.as_deref(),
                            );
                            throttler.inc();
                            (item, png)
                        })
                        .collect();

                    for (item, maybe_png) in fetched {
                        let Some(png) = maybe_png else {
                            continue;
                        };
                        let alias_refs: Vec<&str> =
                            item.aliases.iter().map(String::as_str).collect();
                        if self
                            .manager
                            .add_skin_png_with_aliases(&item.primary, &alias_refs, &png)
                            .is_ok()
                        {
                            newly_added = true;
                            dirty_coords.extend(item.sec_coords);
                        }
                    }
                }
                TextureResolveMode::Cached => {
                    // Never stall the mesher: fetch in the background and let the host re-mesh
                    // when the callback fires.
                    for item in uncached.into_values() {
                        let callback = self.on_resolved.clone();
                        self.skin_resolver.queue_fetch_background(
                            item.skin_url,
                            item.uuid,
                            item.name,
                            move |finished, maybe_png| {
                                if maybe_png.is_some() {
                                    if let Some(callback) = &callback {
                                        callback.call(finished);
                                    }
                                }
                            },
                        );
                    }
                }
            }
        }

        for coord in dirty_coords {
            storage.dirty_sections.insert(coord);
        }

        if newly_added {
            self.registered = false;
        }
        let atlas_changed = !self.registered;
        if atlas_changed {
            self.manager.register_to_address_map(atlas);
            self.registered = true;
        }
        TexturePrepareOutcome {
            added: newly_added,
            atlas_changed,
        }
    }

    fn atlases(&self) -> Vec<(u16, &[u8])> {
        self.manager.all_chunk_buffers()
    }

    fn slots(&self) -> HashMap<String, (u16, u16)> {
        self.manager.all_slot_locations()
    }

    fn set_skin_cache_dir(&mut self, cache_dir: Option<PathBuf>) {
        BlockEntityTextures::set_skin_cache_dir(self, cache_dir);
    }

    fn set_allow_network(&mut self, allow_network: bool) {
        BlockEntityTextures::set_allow_network(self, allow_network);
    }

    fn set_banner_baker(&mut self, baker: BannerBaker) {
        BlockEntityTextures::set_banner_baker(self, baker);
    }

    fn set_skin_resolved_callback(&mut self, callback: SkinResolvedCallback) {
        BlockEntityTextures::set_skin_resolved_callback(self, callback);
    }
}
