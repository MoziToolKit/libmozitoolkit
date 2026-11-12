//! # Live Sync Session Controller (RFC v3.1)
//!
//! Provides the primary high-level entrypoint for WebSocket connection,
//! event queue polling, VoxelWorld synchronization, and DNA mesh handoff.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use mtk_core::MeshData;
use mtk_cull::FaceCuller;
use mtk_model::BakedModelDatabase;
use mtk_voxel::{MesherConfig, VoxelStorage, VoxelWorld};

use crate::client::{ClientCommand, SyncClient};
use crate::events::SyncEvent;

mod assembler;
mod connection;
mod dispatcher;
mod handlers_baseline;
mod handlers_control;
mod handlers_delta;
mod hello;
mod staging;
mod state;
mod watchdog;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use assembler::BaselineAssembler;
#[cfg(test)]
pub(crate) use dispatcher::check_debounced_remesh;
pub(crate) use state::SessionState;
pub use state::SyncLifecycle;

/// High-level Live Sync Session managing WebSocket streaming, VoxelWorld scene synchronization, and event queues.
pub struct LiveSyncSession {
    /// In-memory 3D Voxel World scene engine managing storage, meshing, and cache.
    pub world: Arc<RwLock<VoxelWorld>>,

    client: Option<SyncClient>,
    event_sender: Sender<SyncEvent>,
    event_receiver: Receiver<SyncEvent>,

    worker_running: Arc<AtomicBool>,
    is_connected: Arc<AtomicBool>,
    status: Arc<RwLock<String>>,
    worker_handle: Option<JoinHandle<()>>,
    generation: Arc<AtomicU64>,
    state: Arc<Mutex<SessionState>>,
    cmd_sender: Option<Sender<ClientCommand>>,
    /// Shared secret sent in every HELLO (RFC v3.1 §4.1, trimmed).
    auth_token: String,
    /// Configured continuous visual entities subscribed during HELLO handshake (RFC v3.2).
    /// When None, defaults to `mtk_model::supported_visual_entity_ids()`.
    subscribed_entities: Option<Vec<String>>,
}

impl LiveSyncSession {
    /// Creates a new `LiveSyncSession` with an always-unified world mesh.
    pub fn new(
        config: Option<MesherConfig>,
        culler: Option<FaceCuller>,
        model_db: Option<Arc<BakedModelDatabase>>,
    ) -> Self {
        let (event_sender, event_receiver) = crossbeam_channel::unbounded::<SyncEvent>();
        let world = VoxelWorld::new(config, culler, model_db);

        Self {
            world: Arc::new(RwLock::new(world)),
            client: None,
            event_sender,
            event_receiver,
            worker_running: Arc::new(AtomicBool::new(false)),
            is_connected: Arc::new(AtomicBool::new(false)),
            status: Arc::new(RwLock::new("DISCONNECTED".to_string())),
            worker_handle: None,
            generation: Arc::new(AtomicU64::new(1)),
            state: Arc::new(Mutex::new(SessionState::new())),
            cmd_sender: None,
            auth_token: String::new(),
            subscribed_entities: None,
        }
    }

    /// Sets the continuous visual entities to subscribe to during HELLO handshake (RFC v3.2).
    ///
    /// Pass `None` to restore default behavior (subscribes to all supported visual entities
    /// declared in `mtk-model`).
    pub fn set_subscribed_entities(&mut self, entities: Option<Vec<String>>) {
        self.subscribed_entities = entities;
    }

    /// Returns the currently configured subscribed visual entity IDs, or the default list from `mtk-model`.
    pub fn subscribed_entities(&self) -> Vec<String> {
        self.subscribed_entities
            .clone()
            .unwrap_or_else(crate::session::hello::default_subscribed_visual_entities)
    }

    /// Sets the shared secret attached to every HELLO handshake (RFC v3.1 §4.1).
    ///
    /// The token is trimmed to match the Yefira configuration parser. An empty
    /// token is valid for loopback-only servers; token-protected servers reject
    /// the connection with `REJECTED("Authentication failed")`.
    pub fn set_auth_token(&mut self, token: impl Into<String>) {
        self.auth_token = token.into().trim().to_string();
    }

    /// Returns the currently configured HELLO auth token (trimmed).
    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    /// Sets or replaces the baked model database.
    pub fn set_model_db(&mut self, model_db: Option<Arc<BakedModelDatabase>>) {
        self.world
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .set_model_db(model_db);
    }

    /// Clears the cached section meshes and unified world mesh on the underlying VoxelWorld.
    pub fn clear_cache(&self) {
        self.world
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear_cache();
    }

    /// Sets or updates the active mesher configuration on the underlying VoxelWorld.
    pub fn set_config(&self, config: MesherConfig) {
        self.world
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .set_config(config);
    }

    /// Hot-reloads mesher configuration and model database on the underlying VoxelWorld.
    pub fn hot_reload(
        &self,
        config: Option<MesherConfig>,
        model_db: Option<Arc<BakedModelDatabase>>,
    ) {
        let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
        if let Some(cfg) = config {
            w.set_config(cfg);
        }
        if let Some(db) = model_db {
            w.set_model_db(Some(db));
        }
        w.clear_cache();
    }

    /// Returns whether the background sync session worker is actively running.
    pub fn is_active(&self) -> bool {
        self.worker_running.load(Ordering::SeqCst)
    }

    /// Returns whether the session is actively connected to the server.
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    /// Returns the current connection status string ("CONNECTED", "CONNECTING...", "DISCONNECTED", etc.).
    pub fn status(&self) -> String {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Returns current client generation ID.
    pub fn client_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Returns current session sequence number.
    pub fn current_sequence(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sequence
    }

    /// Returns current server epoch.
    pub fn current_epoch(&self) -> u64 {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).epoch
    }

    /// Returns the last server-confirmed canonical world state hash.
    pub fn current_state_hash(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current_state_hash
    }

    /// Returns the server instance UUID as a 32-character lowercase hex string.
    pub fn server_uuid_hex(&self) -> String {
        let uuid = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .server_uuid;
        uuid.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Returns current session lifecycle.
    pub fn lifecycle(&self) -> SyncLifecycle {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .lifecycle
    }

    /// Meshes the entire active VoxelStorage volume and returns a unified `MeshData`.
    pub fn get_world_mesh(&self) -> MeshData {
        let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
        let _ = w.rebuild_all();
        w.get_world_mesh().cloned().unwrap_or_default()
    }

    /// Execution profile of the most recent rebuild operation, if any.
    pub fn last_profile(&self) -> Option<mtk_core::ExecutionProfile> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.last_profile.clone()
    }

    /// Returns all active dynamic entity atlas chunk buffers owned as (chunk_id -> Vec<u8>).
    pub fn dynamic_entity_atlases(&self) -> std::collections::HashMap<u16, Vec<u8>> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.dynamic_entity_atlases_owned()
    }

    /// Returns dynamic entity atlases if any dynamic entity chunk is referenced by the world geometry.
    pub fn dynamic_entity_atlases_if_used(
        &self,
    ) -> Option<std::collections::HashMap<u16, Vec<u8>>> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        if w.has_dynamic_entity_chunks() {
            Some(w.dynamic_entity_atlases_owned())
        } else {
            None
        }
    }

    /// Returns dynamic entity slot allocations `key -> (chunk_id, slot_idx)`.
    pub fn dynamic_entity_slots(&self) -> std::collections::HashMap<String, (u16, u16)> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.dynamic_entity_slots()
    }

    /// Returns dynamic entity atlases and slot mapping if any dynamic entity chunk is referenced by world geometry.
    pub fn dynamic_entity_atlases_and_slots_if_used(
        &self,
    ) -> (
        Option<std::collections::HashMap<u16, Vec<u8>>>,
        Option<std::collections::HashMap<String, (u16, u16)>>,
    ) {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.dynamic_entity_atlases_if_used()
    }

    /// Sets the on-disk cache directory for downloaded player skins.
    pub fn set_skin_cache_dir(&self, cache_dir: Option<std::path::PathBuf>) {
        let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
        w.set_skin_cache_dir(cache_dir);
    }

    /// Returns a snapshot clone of the in-memory voxel storage.
    pub fn get_storage(&self) -> VoxelStorage {
        self.world
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .storage
            .clone()
    }

    /// Restores voxel storage from an unculled point cloud without re-meshing.
    pub fn load_from_point_cloud(&self, point_cloud: &mtk_voxel::VoxelPointCloud) {
        let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
        w.load_from_point_cloud(point_cloud);
    }

    /// Polls all pending synchronization events from the background Rust engine (non-blocking).
    pub fn poll_events(&self) -> Vec<SyncEvent> {
        let mut events = Vec::new();
        while let Ok(evt) = self.event_receiver.try_recv() {
            events.push(evt);
        }
        events
    }

    /// Polls events and collapses multiple `WorldMeshReady` payloads to the newest generation.
    ///
    /// Hosts that cannot render every intermediate mesh (Blender main-thread timers,
    /// C4D message pumps) should use this to avoid queue choke and wasted DNA copies.
    /// Non-mesh events keep their arrival order; the newest mesh is appended last.
    pub fn poll_events_coalesced(&self) -> Vec<SyncEvent> {
        let events = self.poll_events();
        let mut latest_mesh: Option<(u64, MeshData, Option<mtk_core::ExecutionProfile>)> = None;
        let mut out = Vec::with_capacity(events.len());
        for event in events {
            match event {
                SyncEvent::WorldMeshReady {
                    mesh,
                    generation,
                    profile,
                } => {
                    let should_replace = latest_mesh
                        .as_ref()
                        .map(|(current, _, _)| generation >= *current)
                        .unwrap_or(true);
                    if should_replace {
                        latest_mesh = Some((generation, mesh, profile));
                    }
                }
                other => out.push(other),
            }
        }
        if let Some((generation, mesh, profile)) = latest_mesh {
            out.push(SyncEvent::WorldMeshReady {
                mesh,
                generation,
                profile,
            });
        }
        out
    }
}

impl Drop for LiveSyncSession {
    fn drop(&mut self) {
        self.stop();
    }
}
