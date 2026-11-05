//! # Voxel World Scene and Pipeline
//!
//! Provides the authoritative high-level `VoxelWorld` container integrating
//! sparse voxel storage, ambient occlusion, face culling, prebaked models,
//! multi-threaded Section meshing, incremental dirty remeshing, Section mesh
//! caching, world mesh assembly with vertex welding, and used material chunk
//! tracking.
//!
//! Fully host-agnostic and decoupled from WebSocket network synchronization.

use std::collections::HashMap;
use std::sync::Arc;

use glam::IVec3;
use mtk_core::MeshData;
use mtk_cull::FaceCuller;
use mtk_model::BakedModelDatabase;

use crate::source::VoxelSource;
use crate::storage::VoxelStorage;
use crate::types::{MesherConfig, VoxelError};

mod assembly;
mod debug;
mod rebuild;
mod sync;

/// Wrapper for skin resolution callbacks that satisfies Debug and Clone.
#[derive(Clone)]
pub struct SkinResolvedCallback(pub Arc<dyn Fn(String) + Send + Sync>);

impl std::fmt::Debug for SkinResolvedCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SkinResolvedCallback(...)")
    }
}

/// Unified 3D Voxel World scene managing storage, multi-source ingestion,
/// Section mesh caching, incremental remeshing, and unified geometry assembly.
#[derive(Debug, Clone)]
pub struct VoxelWorld {
    /// In-memory 3D Voxel storage (sections, palettes, biomes, bounds).
    pub storage: VoxelStorage,
    /// Active mesher configuration (AO, fluids, Z-up, origin centering, etc.).
    pub config: MesherConfig,
    /// Face Culling rules and state machine.
    pub culler: FaceCuller,
    /// Prebaked Minecraft blockstate model database for custom models.
    pub model_db: Option<Arc<BakedModelDatabase>>,
    /// Custom thread count for Rayon parallel meshing (None for global pool).
    pub num_threads: Option<usize>,
    /// Cache of section meshes: `(sx, sy, sz) -> MeshData`.
    pub(crate) section_mesh_cache: HashMap<IVec3, MeshData>,
    /// Assembled unified world mesh buffer.
    pub(crate) world_mesh: Option<MeshData>,
    /// Execution profile of the most recent rebuild operation.
    pub last_profile: Option<mtk_core::ExecutionProfile>,
    /// Unique Atlas Chunk IDs actually present on the active world mesh geometry.
    pub(crate) used_chunk_ids: Vec<u32>,
    /// Dedicated multi-chunk dynamic entity atlas manager for player skulls, custom heads, and banners.
    pub dynamic_entities: mtk_texture::DynamicAtlasManager,
    /// Multi-tier skin resolver for player heads.
    pub skin_resolver: mtk_resource::skin::SkinResolver,
    /// Procedural banner baker for multi-layer composite banners.
    pub banner_baker: mtk_texture::BannerBaker,
    /// Callback triggered when a background skin download finishes successfully.
    pub on_skin_resolved: Option<SkinResolvedCallback>,
}

#[inline]
fn create_default_dynamic_atlas() -> mtk_texture::DynamicAtlasManager {
    let mut mgr = mtk_texture::DynamicAtlasManager::default();
    let _ = mgr.prewarm_vanilla_banners(None);
    mgr
}

impl VoxelWorld {
    #[inline]
    fn is_key_in_static_atlas(&self, key: &str) -> bool {
        self.config
            .atlas_address_map
            .as_ref()
            .and_then(|m| m.resolve_texture_key(key))
            .map(|loc| loc.chunk_id < mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
            .unwrap_or(false)
    }

    /// Synchronizes all registered dynamic entity slots into `self.config.atlas_address_map`.
    pub fn sync_dynamic_atlas_to_config(&mut self) {
        if let Some(ref current_map) = self.config.atlas_address_map {
            let mut new_map = (**current_map).clone();
            self.dynamic_entities.register_to_address_map(&mut new_map);
            self.config.atlas_address_map = Some(Arc::new(new_map));
        }
    }

    /// Creates a new `VoxelWorld` with empty storage.
    pub fn new(
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
    ) -> Self {
        let cfg = config.unwrap_or_default();
        let banner_baker = cfg
            .banner_baker
            .as_ref()
            .map(|b| (**b).clone())
            .unwrap_or_default();
        let mut dynamic_entities = create_default_dynamic_atlas();
        if cfg.banner_baker.is_some() {
            let _ = dynamic_entities.prewarm_vanilla_banners(Some(banner_baker.base_cloth()));
        }
        let mut world = Self {
            storage: VoxelStorage::new(),
            config: cfg,
            culler: culler.unwrap_or_default(),
            model_db,
            num_threads: None,
            section_mesh_cache: HashMap::new(),
            world_mesh: None,
            used_chunk_ids: Vec::new(),
            last_profile: None,
            dynamic_entities,
            skin_resolver: mtk_resource::skin::SkinResolver::default(),
            banner_baker,
            on_skin_resolved: None,
        };
        world.sync_dynamic_atlas_to_config();
        world
    }

    /// Creates a `VoxelWorld` taking ownership of an existing `VoxelStorage`.
    pub fn from_storage(
        storage: VoxelStorage,
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
    ) -> Self {
        Self::from_storage_with_threads(storage, config, culler, model_db, None)
    }

    /// Creates a `VoxelWorld` taking ownership of an existing `VoxelStorage` with explicit thread configuration.
    pub fn from_storage_with_threads(
        storage: VoxelStorage,
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
        num_threads: Option<usize>,
    ) -> Self {
        let cfg = config.unwrap_or_default();
        let banner_baker = cfg
            .banner_baker
            .as_ref()
            .map(|b| (**b).clone())
            .unwrap_or_default();
        let mut dynamic_entities = create_default_dynamic_atlas();
        if cfg.banner_baker.is_some() {
            let _ = dynamic_entities.prewarm_vanilla_banners(Some(banner_baker.base_cloth()));
        }
        let mut world = Self {
            storage,
            config: cfg,
            culler: culler.unwrap_or_default(),
            model_db,
            num_threads,
            section_mesh_cache: HashMap::new(),
            world_mesh: None,
            used_chunk_ids: Vec::new(),
            last_profile: None,
            dynamic_entities,
            skin_resolver: mtk_resource::skin::SkinResolver::default(),
            banner_baker,
            on_skin_resolved: None,
        };
        world.sync_dynamic_atlas_to_config();
        world
    }

    /// Sets the worker thread count for parallel meshing.
    pub fn set_num_threads(&mut self, num_threads: Option<usize>) {
        self.num_threads = num_threads;
    }

    /// Returns the configured worker thread count.
    pub fn num_threads(&self) -> Option<usize> {
        self.num_threads
    }

    /// Updates or replaces the prebaked model database.
    pub fn set_model_db(&mut self, model_db: Option<Arc<BakedModelDatabase>>) {
        self.model_db = model_db;
    }

    /// Sets or replaces the procedural banner baker for multi-layer composite banners.
    pub fn set_banner_baker(&mut self, banner_baker: mtk_texture::BannerBaker) {
        let _ = self
            .dynamic_entities
            .prewarm_vanilla_banners(Some(banner_baker.base_cloth()));
        self.banner_baker = banner_baker;
        self.sync_dynamic_atlas_to_config();
    }

    /// Updates mesher configuration.
    pub fn set_config(&mut self, config: MesherConfig) {
        if let Some(ref baker) = config.banner_baker {
            self.banner_baker = (**baker).clone();
            let _ = self
                .dynamic_entities
                .prewarm_vanilla_banners(Some(self.banner_baker.base_cloth()));
        }
        self.config = config;
        self.sync_dynamic_atlas_to_config();
    }

    /// Sets the active 3D selection bounding box.
    /// If bounds changed, clears mesh cache to force coordinate re-centering.
    pub fn set_bounds(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
    ) -> bool {
        let bounds_changed = self
            .storage
            .set_bounds(min_x, min_y, min_z, size_x, size_y, size_z);
        let new_bounds = Some(([min_x, min_y, min_z], [size_x, size_y, size_z]));
        let bounds_differ = self.config.selection_bounds != new_bounds;
        if self.config.origin_centered {
            self.config.selection_bounds = new_bounds;
        }
        if bounds_changed || bounds_differ {
            self.section_mesh_cache.clear();
            self.world_mesh = None;
            self.used_chunk_ids.clear();
            self.storage.mark_all_sections_dirty();
        }
        bounds_changed
    }

    /// Returns the active or calculated bounding box: `(min_x, min_y, min_z, size_x, size_y, size_z)`.
    pub fn get_bounds(&self) -> (i32, i32, i32, i32, i32, i32) {
        self.storage.get_bounds()
    }

    /// Gets the blockstate identifier string at world coordinate `(x, y, z)`.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> &str {
        self.storage.get_block(x, y, z)
    }

    /// Sets the blockstate at world coordinate `(x, y, z)` and marks affected sections dirty.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state: &str, biome: Option<&str>) -> bool {
        self.storage.set_block(x, y, z, state, biome)
    }

    /// Ingests a full snapshot packet and invalidates existing section meshes.
    pub fn set_full_snapshot(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
        palette: &[String],
        grid_indices: &[u16],
        biome_palette: Option<&[String]>,
        biome_indices: Option<&[u16]>,
    ) {
        self.storage.set_full_snapshot(
            min_x,
            min_y,
            min_z,
            size_x,
            size_y,
            size_z,
            palette,
            grid_indices,
            biome_palette,
            biome_indices,
        );
        if self.config.origin_centered {
            self.config.selection_bounds = Some(([min_x, min_y, min_z], [size_x, size_y, size_z]));
        }
        self.section_mesh_cache.clear();
        self.world_mesh = None;
        self.used_chunk_ids.clear();
    }

    /// Ingests a single 16x16x16 chunk section snapshot.
    pub fn set_section_snapshot(
        &mut self,
        sec_x: i32,
        sec_y: i32,
        sec_z: i32,
        start_x: i32,
        start_y: i32,
        start_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
        palette: &[String],
        grid_indices: &[u16],
        biome_palette: Option<&[String]>,
        biome_indices: Option<&[u16]>,
    ) {
        self.storage.set_section_snapshot(
            sec_x,
            sec_y,
            sec_z,
            start_x,
            start_y,
            start_z,
            size_x,
            size_y,
            size_z,
            palette,
            grid_indices,
            biome_palette,
            biome_indices,
        );
    }

    /// Applies a batch of block delta modifications.
    pub fn apply_delta_update(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        changes: &[(i32, i32, i32, &str)],
    ) -> Vec<(i32, i32, i32, String, String)> {
        self.storage
            .apply_delta_update(min_x, min_y, min_z, changes)
    }

    /// Ingests chunk sections from an abstract `VoxelSource` provider (Anvil save,
    /// Litematica schematic, structure NBT, or procedural generator).
    pub fn ingest_from_source<S: VoxelSource + ?Sized>(
        &mut self,
        source: &mut S,
        sections: Option<&[IVec3]>,
    ) -> Result<usize, VoxelError> {
        crate::source::ingest_from_source(source, &mut self.storage, sections)
    }

    /// Exports manifest metadata (bounds, generation, known empty sections) as JSON.
    #[cfg(feature = "serde")]
    pub fn export_manifest_metadata(&self) -> serde_json::Value {
        self.storage.export_manifest_metadata()
    }

    /// Imports manifest metadata from JSON to restore persistent scene state.
    #[cfg(feature = "serde")]
    pub fn import_manifest_metadata(&mut self, data: &serde_json::Value) -> bool {
        let ok = self.storage.import_manifest_metadata(data);
        if ok {
            self.sync_selection_bounds();
        }
        ok
    }

    /// Synchronizes selection bounds if origin centering is enabled.
    fn sync_selection_bounds(&mut self) {
        if self.config.origin_centered {
            let (min_x, min_y, min_z, sz_x, sz_y, sz_z) = self.storage.get_bounds();
            if sz_x > 0 && sz_y > 0 && sz_z > 0 {
                let new_bounds = Some(([min_x, min_y, min_z], [sz_x, sz_y, sz_z]));
                if self.config.selection_bounds != new_bounds {
                    self.config.selection_bounds = new_bounds;
                    self.section_mesh_cache.clear();
                    self.world_mesh = None;
                    self.used_chunk_ids.clear();
                    self.storage.mark_all_sections_dirty();
                }
            }
        }
    }

    /// Returns a reference to the assembled unified world mesh, if available.
    pub fn get_world_mesh(&self) -> Option<&MeshData> {
        self.world_mesh.as_ref()
    }

    /// Returns a reference to the cached section meshes.
    pub fn get_section_cache(&self) -> &HashMap<IVec3, MeshData> {
        &self.section_mesh_cache
    }

    /// Returns the exact slice of Atlas Chunk IDs actually referenced by the current world mesh.
    pub fn used_chunk_ids(&self) -> &[u32] {
        &self.used_chunk_ids
    }

    /// Compacts `world_mesh` material indices into contiguous `0..N-1` and returns the
    /// mapping of original Atlas Chunk IDs.
    pub fn compact_world_mesh_materials(&mut self) -> Vec<u16> {
        if let Some(ref mut m) = self.world_mesh {
            let mapping = m.compact_materials();
            self.used_chunk_ids = mapping.iter().map(|&id| id as u32).collect();
            mapping
        } else {
            Vec::new()
        }
    }

    /// Extracts an unculled point cloud representation of the current world geometry.
    pub fn to_point_cloud(&self) -> crate::storage::VoxelPointCloud {
        self.storage.to_point_cloud(&self.config)
    }

    /// Reconstructs world storage from a point cloud and clears cached meshes.
    pub fn load_from_point_cloud(&mut self, point_cloud: &crate::storage::VoxelPointCloud) {
        self.storage.load_from_point_cloud(point_cloud);
        self.section_mesh_cache.clear();
        self.world_mesh = None;
        self.used_chunk_ids.clear();
        self.sync_selection_bounds();
    }

    /// Clears the cached section meshes and unified world mesh, and marks all storage sections dirty.
    ///
    /// Used when assets (Atlas, Models, Biomes) or mesher configurations change dynamically,
    /// ensuring subsequent remeshing rebuilds all chunk sections with the updated definitions.
    pub fn clear_cache(&mut self) {
        self.section_mesh_cache.clear();
        self.world_mesh = None;
        self.used_chunk_ids.clear();
        self.storage.mark_all_sections_dirty();
    }

    /// Hot-reloads asset databases (model DB, atlas address map, biome resolver)
    /// and purges all cached section meshes so that future meshing immediately reflects new assets.
    pub fn hot_reload_assets(
        &mut self,
        model_db: Option<Arc<BakedModelDatabase>>,
        atlas_map: Option<Arc<mtk_texture::AtlasAddressMap>>,
        biome_resolver: Option<Arc<mtk_material::BiomeResolver>>,
    ) {
        self.model_db = model_db;
        self.config.atlas_address_map = atlas_map;
        self.config.biome_resolver = biome_resolver;
        self.sync_dynamic_atlas_to_config();
        self.clear_cache();
    }

    /// Clears all sections, cache, and world mesh.
    pub fn clear(&mut self) {
        self.storage.clear();
        self.section_mesh_cache.clear();
        self.world_mesh = None;
        self.used_chunk_ids.clear();
    }

    /// Gets the block entity at world coordinate (x, y, z), if any.
    pub fn get_block_entity(&self, x: i32, y: i32, z: i32) -> Option<&mtk_model::BlockEntityData> {
        self.storage.get_block_entity(x, y, z)
    }

    /// Sets or removes the block entity at world coordinate (x, y, z).
    pub fn set_block_entity(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        data: Option<mtk_model::BlockEntityData>,
    ) {
        self.storage.set_block_entity(x, y, z, data);
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

    /// Explicitly registers a skin PNG into the dynamic atlas and updates the active address map.
    pub fn register_skin(
        &mut self,
        key: &str,
        png: &[u8],
    ) -> Result<(), mtk_texture::TextureError> {
        self.skin_resolver.register_memory_skin(key, png.to_vec());
        self.dynamic_entities.add_skin_png(key, png)?;
        let mut new_map = if let Some(ref current_map) = self.config.atlas_address_map {
            (**current_map).clone()
        } else {
            mtk_texture::AtlasAddressMap::new()
        };
        self.dynamic_entities.register_to_address_map(&mut new_map);
        self.config.atlas_address_map = Some(Arc::new(new_map));
        Ok(())
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
                        let lx = ((local_idx >> 8) & 15) as i32;
                        let ly = ((local_idx >> 4) & 15) as i32;
                        let lz = (local_idx & 15) as i32;
                        let wx = sec_coord.x * 16 + lx;
                        let wy = sec_coord.y * 16 + ly;
                        let wz = sec_coord.z * 16 + lz;
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
                                    let mut aliases = vec![key.clone()];
                                    if let Some(n) = name {
                                        let clean_n = mtk_resource::skin::clean_player_name(n);
                                        if !clean_n.is_empty() {
                                            aliases.push(format!("player_{}", clean_n));
                                            aliases.push(clean_n);
                                        }
                                    }
                                    if let Some(u) = uuid {
                                        let clean_u = mtk_resource::skin::clean_uuid(u);
                                        if !clean_u.is_empty() {
                                            aliases.push(format!("uuid_{}", clean_u));
                                            aliases.push(clean_u);
                                        }
                                    }
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

    /// Synchronous variant for specific sections: waits for network if uncached.
    pub fn ensure_dynamic_entities_resolved_sync_for_sections(&mut self, coords: &[IVec3]) -> bool {
        self.ensure_dynamic_entities_resolved_sync_for_sections_with_progress(coords, None)
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
                        let lx = ((local_idx >> 8) & 15) as i32;
                        let ly = ((local_idx >> 4) & 15) as i32;
                        let lz = (local_idx & 15) as i32;
                        let wx = sec_coord.x * 16 + lx;
                        let wy = sec_coord.y * 16 + ly;
                        let wz = sec_coord.z * 16 + lz;
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
                                    let mut aliases = vec![key.clone()];
                                    if let Some(n) = name {
                                        let clean_n = mtk_resource::skin::clean_player_name(n);
                                        if !clean_n.is_empty() {
                                            aliases.push(format!("player_{}", clean_n));
                                            aliases.push(clean_n);
                                        }
                                    }
                                    if let Some(u) = uuid {
                                        let clean_u = mtk_resource::skin::clean_uuid(u);
                                        if !clean_u.is_empty() {
                                            aliases.push(format!("uuid_{}", clean_u));
                                            aliases.push(clean_u);
                                        }
                                    }
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
                                            let mut aliases = vec![key.clone()];
                                            if let Some(n) = name {
                                                let clean_n =
                                                    mtk_resource::skin::clean_player_name(n);
                                                if !clean_n.is_empty() {
                                                    aliases.push(format!("player_{}", clean_n));
                                                    aliases.push(clean_n);
                                                }
                                            }
                                            if let Some(u) = uuid {
                                                let clean_u = mtk_resource::skin::clean_uuid(u);
                                                if !clean_u.is_empty() {
                                                    aliases.push(format!("uuid_{}", clean_u));
                                                    aliases.push(clean_u);
                                                }
                                            }
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

impl Default for VoxelWorld {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}
