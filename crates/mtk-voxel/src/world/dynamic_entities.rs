//! # Dynamic Entities and Skin Resolution Pipeline
//!
//! Manages dynamic atlas allocation, asynchronous/synchronous skin fetching for player skulls,
//! and procedural multi-layer banner baking.

use std::collections::HashMap;
use std::sync::Arc;

use glam::IVec3;

use super::VoxelWorld;

/// Wrapper for skin resolution callbacks that satisfies Debug and Clone.
#[derive(Clone)]
pub struct SkinResolvedCallback(pub Arc<dyn Fn(String) + Send + Sync>);

impl std::fmt::Debug for SkinResolvedCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SkinResolvedCallback(...)")
    }
}

#[inline]
fn local_to_world(sec_coord: IVec3, local_idx: u16) -> (i32, i32, i32) {
    (
        sec_coord.x * 16 + (((local_idx >> 8) & 15) as i32),
        sec_coord.y * 16 + (((local_idx >> 4) & 15) as i32),
        sec_coord.z * 16 + ((local_idx & 15) as i32),
    )
}

fn collect_skull_aliases(key: &str, name: Option<&str>, uuid: Option<&str>) -> Vec<String> {
    let mut aliases = vec![key.to_string()];
    if let Some(n) = name {
        let clean_n = mtk_resource::skin::clean_player_name(n);
        if !clean_n.is_empty() {
            aliases.push(format!("player_{clean_n}"));
            aliases.push(clean_n);
        }
    }
    if let Some(u) = uuid {
        let clean_u = mtk_resource::skin::clean_uuid(u);
        if !clean_u.is_empty() {
            aliases.push(format!("uuid_{clean_u}"));
            aliases.push(clean_u);
        }
    }
    aliases
}

impl VoxelWorld {
    #[inline]
    pub(crate) fn is_key_in_static_atlas(&self, key: &str) -> bool {
        self.config
            .atlas_address_map
            .as_ref()
            .and_then(|m| m.resolve_texture_key(key))
            .map(|loc| loc.chunk_id < mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
            .unwrap_or(false)
    }

    /// Returns the RGBA pixel bytes (1024x1024x4) of the primary dynamic entity atlas (0xFFF0).
    pub fn dynamic_entity_atlas_rgba(&self) -> Option<&[u8]> {
        self.dynamic_entities.primary_rgba_buffer()
    }

    /// Returns all active dynamic entity atlas chunk buffers as (chunk_id, &[u8]).
    pub fn dynamic_entity_atlases(&self) -> Vec<(u16, &[u8])> {
        self.dynamic_entities.all_chunk_buffers()
    }

    /// Returns all active dynamic entity atlas chunk buffers owned as (chunk_id, Vec<u8>).
    pub fn dynamic_entity_atlases_owned(&self) -> HashMap<u16, Vec<u8>> {
        self.dynamic_entities.all_chunk_buffers_owned()
    }

    /// Returns a copy of the dynamic entity key and alias to (chunk_id, slot_index) mapping.
    pub fn dynamic_entity_slots(&self) -> HashMap<String, (u16, u16)> {
        self.dynamic_entities.all_slot_locations()
    }

    /// Returns true if the active world mesh contains any faces referencing dynamic entity atlas chunks.
    #[inline]
    pub fn has_dynamic_entity_chunks(&self) -> bool {
        self.used_chunk_ids()
            .iter()
            .any(|&c| c >= mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID as u32)
    }

    /// Returns dynamic entity atlases and slot mapping if any dynamic entity chunk is referenced by the world geometry.
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
        self.skin_resolver = mtk_resource::skin::SkinResolver::new(cache_dir);
    }

    /// Sets the callback triggered when a background skin download finishes.
    pub fn set_skin_resolved_callback<F>(&mut self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.on_skin_resolved = Some(SkinResolvedCallback(Arc::new(callback)));
    }

    /// Pre-scans all block entities in the world and ensures dynamic entities (player skulls)
    /// have their skin textures resolved non-blockingly from memory or disk cache.
    /// Uncached skins trigger background fetching without stalling the mesher thread.
    pub fn ensure_dynamic_entities_resolved(&mut self) -> bool {
        let coords: Vec<IVec3> = self.storage.sections.keys().copied().collect();
        self.ensure_dynamic_entities_resolved_for_sections(&coords)
    }

    /// Scans block entities in the given sections and ensures dynamic entities (player skulls)
    /// have their skin textures resolved non-blockingly from memory or disk cache.
    pub fn ensure_dynamic_entities_resolved_for_sections(&mut self, coords: &[IVec3]) -> bool {
        let mut newly_added = false;
        let mut dirty_coords = Vec::new();

        for sec_coord in coords {
            if let Some(sec) = self.storage.sections.get(sec_coord) {
                for (&local_idx, be) in &sec.block_entities {
                    if self.storage.has_explicit_bounds {
                        let (wx, wy, wz) = local_to_world(*sec_coord, local_idx);
                        if !self.storage.contains(wx, wy, wz) {
                            continue;
                        }
                    }
                    if let mtk_model::BlockEntityData::Skull { kind, profile, .. } = be {
                        if *kind == mtk_model::skull::SkullKind::Player {
                            let (skin_url, uuid, name) = if let Some(prof) = profile {
                                (
                                    prof.skin_url.as_deref(),
                                    prof.id.as_deref(),
                                    prof.name.as_deref(),
                                )
                            } else {
                                (None, None, None)
                            };

                            if skin_url.is_none() && uuid.is_none() && name.is_none() {
                                continue;
                            }

                            let key = mtk_resource::skin::derive_skin_key(skin_url, uuid, name);
                            let hash_opt =
                                self.skin_resolver.resolve_skin_hash(skin_url, uuid, name);
                            let primary = hash_opt.as_deref().unwrap_or(&key);

                            let in_static_atlas = self.is_key_in_static_atlas(primary)
                                || self.is_key_in_static_atlas(&key);

                            if !in_static_atlas
                                && !self.dynamic_entities.contains_key(primary)
                                && !self.dynamic_entities.contains_key(&key)
                            {
                                if let Some(png) =
                                    self.skin_resolver.try_resolve_cached(skin_url, uuid, name)
                                {
                                    let aliases = collect_skull_aliases(&key, name, uuid);
                                    let alias_refs: Vec<&str> =
                                        aliases.iter().map(|s| s.as_str()).collect();
                                    if self
                                        .dynamic_entities
                                        .add_skin_png_with_aliases(primary, &alias_refs, &png)
                                        .is_ok()
                                    {
                                        newly_added = true;
                                        dirty_coords.push(*sec_coord);
                                    }

                                    // Background online validation to sync latest skin if remote changed
                                    let cb_opt = self.on_skin_resolved.clone();
                                    self.skin_resolver.check_and_queue_online_validation(
                                        uuid.map(|s| s.to_string()),
                                        name.map(|s| s.to_string()),
                                        move |k, _new_png| {
                                            if let Some(ref cb) = cb_opt {
                                                (cb.0)(k);
                                            }
                                        },
                                    );
                                } else {
                                    let cb_opt = self.on_skin_resolved.clone();
                                    self.skin_resolver.queue_fetch_background(
                                        skin_url.map(|s| s.to_string()),
                                        uuid.map(|s| s.to_string()),
                                        name.map(|s| s.to_string()),
                                        move |k, maybe_png| {
                                            if maybe_png.is_some() {
                                                if let Some(ref cb) = cb_opt {
                                                    (cb.0)(k);
                                                }
                                            }
                                        },
                                    );
                                }
                            }
                        }
                    } else if let mtk_model::BlockEntityData::Banner {
                        base_color,
                        patterns,
                    } = be
                    {
                        let key =
                            mtk_model::banner::banner_texture_identifier(base_color, patterns);
                        if !self.is_key_in_static_atlas(&key)
                            && !self.dynamic_entities.contains_key(&key)
                        {
                            let pat_tuples: Vec<(&str, &str)> = patterns
                                .iter()
                                .map(|p| (p.pattern.as_str(), p.color.as_str()))
                                .collect();
                            let baked = self.banner_baker.bake(base_color, &pat_tuples);
                            let (cid, slot_idx) = self.dynamic_entities.allocate_slot(&key);
                            if self
                                .dynamic_entities
                                .blit_skin_rgba(
                                    cid,
                                    slot_idx,
                                    &baked.pixels,
                                    baked.width,
                                    baked.height,
                                )
                                .is_ok()
                            {
                                newly_added = true;
                                dirty_coords.push(*sec_coord);
                            }
                        }
                    }
                }
            }
        }

        for coord in dirty_coords {
            self.storage.dirty_sections.insert(coord);
        }

        if newly_added {
            let mut new_map = if let Some(ref current_map) = self.config.atlas_address_map {
                (**current_map).clone()
            } else {
                mtk_texture::AtlasAddressMap::new()
            };
            self.dynamic_entities.register_to_address_map(&mut new_map);
            self.config.atlas_address_map = Some(Arc::new(new_map));
        }

        newly_added
    }

    /// Synchronous variant: waits for network if uncached (used primarily in tests, offline CLI, and save imports).
    pub fn ensure_dynamic_entities_resolved_sync(&mut self) -> bool {
        let coords: Vec<IVec3> = self.storage.sections.keys().copied().collect();
        self.ensure_dynamic_entities_resolved_sync_for_sections_with_progress(&coords, None)
    }

    /// Synchronous variant with progress reporting.
    pub fn ensure_dynamic_entities_resolved_sync_with_progress(
        &mut self,
        progress: Option<mtk_core::ProgressCallback>,
    ) -> bool {
        let coords: Vec<IVec3> = self.storage.sections.keys().copied().collect();
        self.ensure_dynamic_entities_resolved_sync_for_sections_with_progress(&coords, progress)
    }

    /// Synchronous variant for specific sections with batch parallel resolving and progress reporting.
    pub fn ensure_dynamic_entities_resolved_sync_for_sections_with_progress(
        &mut self,
        coords: &[IVec3],
        progress: Option<mtk_core::ProgressCallback>,
    ) -> bool {
        let mut newly_added = false;
        let mut dirty_coords = Vec::new();

        struct UnresolvedSkull {
            primary: String,
            aliases: Vec<String>,
            skin_url: Option<String>,
            uuid: Option<String>,
            name: Option<String>,
            sec_coords: Vec<IVec3>,
        }

        let mut uncached_map: HashMap<String, UnresolvedSkull> = HashMap::new();

        for sec_coord in coords {
            if let Some(sec) = self.storage.sections.get(sec_coord) {
                for (&local_idx, be) in &sec.block_entities {
                    if self.storage.has_explicit_bounds {
                        let (wx, wy, wz) = local_to_world(*sec_coord, local_idx);
                        if !self.storage.contains(wx, wy, wz) {
                            continue;
                        }
                    }
                    if let mtk_model::BlockEntityData::Skull { kind, profile, .. } = be {
                        if *kind == mtk_model::skull::SkullKind::Player {
                            let (skin_url, uuid, name) = if let Some(prof) = profile {
                                (
                                    prof.skin_url.as_deref(),
                                    prof.id.as_deref(),
                                    prof.name.as_deref(),
                                )
                            } else {
                                (None, None, None)
                            };

                            if skin_url.is_none() && uuid.is_none() && name.is_none() {
                                continue;
                            }

                            let key = mtk_resource::skin::derive_skin_key(skin_url, uuid, name);
                            let hash_opt =
                                self.skin_resolver.resolve_skin_hash(skin_url, uuid, name);
                            let primary = hash_opt.unwrap_or_else(|| key.clone());

                            let in_static_atlas = self.is_key_in_static_atlas(&primary)
                                || self.is_key_in_static_atlas(&key);

                            if !in_static_atlas
                                && !self.dynamic_entities.contains_key(&primary)
                                && !self.dynamic_entities.contains_key(&key)
                            {
                                if let Some(png) =
                                    self.skin_resolver.try_resolve_cached(skin_url, uuid, name)
                                {
                                    let aliases = collect_skull_aliases(&key, name, uuid);
                                    let alias_refs: Vec<&str> =
                                        aliases.iter().map(|s| s.as_str()).collect();
                                    if self
                                        .dynamic_entities
                                        .add_skin_png_with_aliases(&primary, &alias_refs, &png)
                                        .is_ok()
                                    {
                                        newly_added = true;
                                        dirty_coords.push(*sec_coord);
                                    }
                                } else {
                                    let entry =
                                        uncached_map.entry(primary.clone()).or_insert_with(|| {
                                            let aliases = collect_skull_aliases(&key, name, uuid);
                                            UnresolvedSkull {
                                                primary,
                                                aliases,
                                                skin_url: skin_url.map(|s| s.to_string()),
                                                uuid: uuid.map(|s| s.to_string()),
                                                name: name.map(|s| s.to_string()),
                                                sec_coords: Vec::new(),
                                            }
                                        });
                                    entry.sec_coords.push(*sec_coord);
                                }
                            }
                        }
                    } else if let mtk_model::BlockEntityData::Banner {
                        base_color,
                        patterns,
                    } = be
                    {
                        let key =
                            mtk_model::banner::banner_texture_identifier(base_color, patterns);
                        if !self.is_key_in_static_atlas(&key)
                            && !self.dynamic_entities.contains_key(&key)
                        {
                            let pat_tuples: Vec<(&str, &str)> = patterns
                                .iter()
                                .map(|p| (p.pattern.as_str(), p.color.as_str()))
                                .collect();
                            let baked = self.banner_baker.bake(base_color, &pat_tuples);
                            let (cid, slot_idx) = self.dynamic_entities.allocate_slot(&key);
                            if self
                                .dynamic_entities
                                .blit_skin_rgba(
                                    cid,
                                    slot_idx,
                                    &baked.pixels,
                                    baked.width,
                                    baked.height,
                                )
                                .is_ok()
                            {
                                newly_added = true;
                                dirty_coords.push(*sec_coord);
                            }
                        }
                    }
                }
            }
        }

        if !uncached_map.is_empty() {
            let total = uncached_map.len();
            let throttler =
                mtk_core::ProgressThrottler::new("resolving_player_skins", total, progress)
                    .with_prefix("Resolving player head skins");

            let items: Vec<UnresolvedSkull> = uncached_map.into_values().collect();
            let resolver = self.skin_resolver.clone();

            #[cfg(feature = "parallel")]
            let fetched: Vec<(UnresolvedSkull, Option<Vec<u8>>)> = {
                use rayon::prelude::*;
                items
                    .into_par_iter()
                    .map(|item| {
                        let res = resolver.resolve_skin_png(
                            item.skin_url.as_deref(),
                            item.uuid.as_deref(),
                            item.name.as_deref(),
                        );
                        throttler.inc();
                        (item, res)
                    })
                    .collect()
            };

            #[cfg(not(feature = "parallel"))]
            let fetched: Vec<(UnresolvedSkull, Option<Vec<u8>>)> = items
                .into_iter()
                .map(|item| {
                    let res = resolver.resolve_skin_png(
                        item.skin_url.as_deref(),
                        item.uuid.as_deref(),
                        item.name.as_deref(),
                    );
                    throttler.inc();
                    (item, res)
                })
                .collect();

            for (item, maybe_png) in fetched {
                if let Some(png) = maybe_png {
                    let alias_refs: Vec<&str> = item.aliases.iter().map(|s| s.as_str()).collect();
                    if self
                        .dynamic_entities
                        .add_skin_png_with_aliases(&item.primary, &alias_refs, &png)
                        .is_ok()
                    {
                        newly_added = true;
                        dirty_coords.extend(item.sec_coords);
                    }
                }
            }
        }

        for coord in dirty_coords {
            self.storage.dirty_sections.insert(coord);
        }

        if newly_added {
            let mut new_map = if let Some(ref current_map) = self.config.atlas_address_map {
                (**current_map).clone()
            } else {
                mtk_texture::AtlasAddressMap::new()
            };
            self.dynamic_entities.register_to_address_map(&mut new_map);
            self.config.atlas_address_map = Some(Arc::new(new_map));
        }

        newly_added
    }
}
