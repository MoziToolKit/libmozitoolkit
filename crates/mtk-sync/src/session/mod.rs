//! # Live Sync Session Controller (RFC v3.1)
//!
//! Provides the primary high-level entrypoint for WebSocket connection,
//! event queue polling, VoxelWorld synchronization, and DNA mesh handoff.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use mtk_core::mesh::MeshData;
use mtk_cull::FaceCuller;
use mtk_model::baker::baked_model::BakedModelDatabase;
use mtk_voxel::storage::VoxelStorage;
use mtk_voxel::types::MesherConfig;
use mtk_voxel::VoxelWorld;

use crate::client::{ClientCommand, ClientMessage, SyncClient};
use crate::events::SyncEvent;
use crate::protocol::*;

pub mod assembler;
pub mod dispatcher;
mod hello;
#[cfg(feature = "serde")]
pub mod persisted;
pub mod staging;

use hello::{default_hello, with_auth_token};

pub use assembler::*;
pub use dispatcher::*;
#[cfg(feature = "serde")]
pub use persisted::*;
pub use staging::*;

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
        }
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
    pub fn last_profile(&self) -> Option<mtk_core::profile::ExecutionProfile> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.last_profile.clone()
    }

    /// Returns the RGBA pixel bytes (1024x1024x4) of the primary dynamic entity atlas (0xFFF0), if active.
    pub fn dynamic_entity_atlas_rgba(&self) -> Option<Vec<u8>> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.dynamic_entity_atlas_rgba().map(|s| s.to_vec())
    }

    /// Returns all active dynamic entity atlas chunk buffers owned as (chunk_id -> Vec<u8>).
    pub fn dynamic_entity_atlases(&self) -> std::collections::HashMap<u16, Vec<u8>> {
        let w = self.world.read().unwrap_or_else(|e| e.into_inner());
        w.dynamic_entity_atlases_owned()
    }

    /// Sets the on-disk cache directory for downloaded player skins.
    pub fn set_skin_cache_dir(&self, cache_dir: Option<std::path::PathBuf>) {
        let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
        w.set_skin_cache_dir(cache_dir);
    }

    /// Sends a RESYNC_REQUEST packet to request recovery / re-sync.
    pub fn request_resync(&self) -> Result<(), String> {
        let (epoch, seq) = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            // Drop any partially received baseline so the incoming snapshot can
            // be assembled from scratch; otherwise stale chunks are silently
            // rejected and the UI can stall mid-progress.
            s.awaiting_baseline();
            // Manual retry: give the selection a fresh retry budget.
            s.validation_failures = 0;
            (s.epoch, s.sequence)
        };
        let packet = Packet::ResyncRequest {
            epoch,
            missing_from_seq: seq,
            missing_to_seq: seq + 1,
            reason: ResyncReason::StateHashMismatch,
        };
        let buf = codec::encode_packet(&packet);
        if let Some(cmd) = &self.cmd_sender {
            cmd.send(ClientCommand::Send(buf))
                .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("Client not connected".to_string())
        }
    }

    /// Returns a snapshot clone of the in-memory voxel storage.
    pub fn get_storage(&self) -> VoxelStorage {
        self.world
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .storage
            .clone()
    }

    /// Manually processes a packet directly into the state machine (useful for tests and mocks).
    pub fn process_packet_direct(&self, packet: Packet) {
        let dummy_cmd_sender = self
            .cmd_sender
            .clone()
            .unwrap_or_else(|| crossbeam_channel::unbounded().0);
        let mut state_guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        handle_packet(
            packet,
            0,
            &mut state_guard,
            &self.world,
            &dummy_cmd_sender,
            &self.event_sender,
            &self.generation,
        );
    }

    /// Starts the live sync session connecting to the given WebSocket `url`,
    /// optionally passing cached session parameters for HELLO handshake negotiation.
    pub fn start_with_hello(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        initial_hello: Option<Packet>,
    ) -> Result<(), String> {
        self.stop();

        // RFC v3.1 §4.1: every HELLO must carry the shared auth token. Hosts with no
        // persisted session state pass `None`, which used to produce a zero-token
        // HELLO; token-protected servers then rejected the client with
        // "Authentication failed" even though both sides held the same secret.
        let initial_hello = match initial_hello {
            Some(hello) => with_auth_token(hello, &self.auth_token),
            None => default_hello(&self.auth_token),
        };

        // Drop any mesh/progress events left over from the previous connection so a
        // reconnecting host cannot apply a stale world mesh to the new session.
        while self.event_receiver.try_recv().is_ok() {}

        // Reset the per-connection state machine. Reusing a session (hot reconnect
        // from Blender) must never inherit a half-assembled baseline, a stale
        // epoch/sequence or palette entries from the previous selection.
        {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.reset_for_new_connection();
        }

        *self.status.write().unwrap_or_else(|e| e.into_inner()) = "CONNECTING...".to_string();
        self.is_connected.store(false, Ordering::SeqCst);

        let (msg_sender, msg_receiver) = crossbeam_channel::unbounded::<ClientMessage>();
        let msg_sender_skin = msg_sender.clone();
        {
            let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
            w.set_skin_resolved_callback(move |key| {
                let _ = msg_sender_skin.send(ClientMessage::SkinDownloaded(key));
            });
        }
        let client = SyncClient::connect(
            url.to_string(),
            msg_sender,
            auto_reconnect,
            max_reconnect_attempts,
        )?;

        let cmd_sender = client.get_cmd_sender();
        self.cmd_sender = Some(cmd_sender.clone());
        self.client = Some(client);
        self.worker_running.store(true, Ordering::SeqCst);

        let world_clone = self.world.clone();
        let event_sender = self.event_sender.clone();
        let running_clone = self.worker_running.clone();
        let is_connected_clone = self.is_connected.clone();
        let status_clone = self.status.clone();
        let generation_clone = self.generation.clone();
        let state_clone = self.state.clone();

        let handle = std::thread::spawn(move || {
            event_worker_loop(
                msg_receiver,
                cmd_sender,
                world_clone,
                event_sender,
                running_clone,
                is_connected_clone,
                status_clone,
                generation_clone,
                state_clone,
                initial_hello,
            );
        });

        self.worker_handle = Some(handle);
        Ok(())
    }

    /// Convenience start method using default HELLO parameters.
    pub fn start(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
    ) -> Result<(), String> {
        self.start_with_hello(url, auto_reconnect, max_reconnect_attempts, None)
    }

    /// Stops the live sync session cleanly and disconnects from server.
    pub fn stop(&mut self) {
        self.worker_running.store(false, Ordering::SeqCst);
        self.is_connected.store(false, Ordering::SeqCst);
        *self.status.write().unwrap_or_else(|e| e.into_inner()) = "DISCONNECTED".to_string();

        if let Some(mut client) = self.client.take() {
            client.stop();
        }

        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.cmd_sender = None;
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
        let mut latest_mesh: Option<(u64, MeshData, Option<mtk_core::profile::ExecutionProfile>)> =
            None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_events_coalesced_keeps_newest_world_mesh() {
        let session = LiveSyncSession::new(None, None, None);
        session
            .event_sender
            .send(SyncEvent::StatusChange("CONNECTED".to_string()))
            .unwrap();
        session
            .event_sender
            .send(SyncEvent::WorldMeshReady {
                mesh: MeshData::new(),
                generation: 1,
                profile: None,
            })
            .unwrap();
        session
            .event_sender
            .send(SyncEvent::WorldMeshReady {
                mesh: MeshData::new(),
                generation: 3,
                profile: None,
            })
            .unwrap();
        session
            .event_sender
            .send(SyncEvent::WorldMeshReady {
                mesh: MeshData::new(),
                generation: 2,
                profile: None,
            })
            .unwrap();

        let events = session.poll_events_coalesced();
        assert_eq!(events.len(), 2, "status + one coalesced mesh expected");
        assert!(matches!(events[0], SyncEvent::StatusChange(_)));
        match &events[1] {
            SyncEvent::WorldMeshReady { generation, .. } => assert_eq!(*generation, 3),
            other => panic!("expected newest world mesh, got {other:?}"),
        }
    }

    #[test]
    fn auth_token_is_trimmed_like_the_server_config() {
        let mut session = LiveSyncSession::new(None, None, None);
        session.set_auth_token("  padded-secret \n");
        assert_eq!(session.auth_token(), "padded-secret");

        session.set_auth_token("   ");
        assert_eq!(session.auth_token(), "");
    }
}
