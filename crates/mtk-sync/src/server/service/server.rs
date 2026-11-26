use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glam::IVec3;
use interprocess::local_socket::traits::Listener as IpcListenerTrait;

use crate::discovery::{
    default_registry_dir, generate_endpoint_id, InstanceMetadata, InstanceRegistrationGuard,
};
use crate::protocol::{
    compute_sparse_state_hash_from_raw, encode_packet, CancelReason, DeltaChangeEntry, Packet,
    ProtocolError,
};
use crate::server::baseline::{encode_baseline_from_raw, BaselineEncoderConfig, BaselineStream};
use crate::server::delta::encode_delta_op;
use crate::transport_ipc::{bind_ipc_listener, format_ipc_url};

use super::config::{generate_unique_server_uuid, ServerCommand, ServerConfig, ServerEvent};
use super::state::{lock_unpoisoned, ClientHandle, PaletteState, SelectionContext, ServerInner};
use super::worker::{run_ipc_client_worker, run_ws_client_worker};

/// The Native Live Sync Server instance (WebSocket and/or local IPC).
pub struct LiveSyncServer {
    inner: Arc<ServerInner>,
    /// Mutex so `stop` can take `&self`: JNI hands the server out as a shared `Arc`, and an
    /// in-flight call must be able to observe the stopped state without exclusive access.
    listener_threads: std::sync::Mutex<Vec<JoinHandle<()>>>,
    ipc_registration: std::sync::Mutex<Option<InstanceRegistrationGuard>>,
}

impl LiveSyncServer {
    /// Starts a new native Live Sync server (WebSocket and/or local IPC).
    pub fn start(mut config: ServerConfig) -> Result<Self, ProtocolError> {
        if config.bind_address.is_none() && !config.enable_ipc {
            return Err(ProtocolError::TruncatedPayload {
                field: "Neither bind_address nor enable_ipc is configured",
            });
        }

        if config.server_uuid == [0u8; 16] {
            config.server_uuid = generate_unique_server_uuid();
        }

        let inner = Arc::new(ServerInner::new(config.server_uuid));
        let mut listener_threads = Vec::new();
        let mut ws_bound_str: Option<String> = None;

        if let Some(ref bind_address) = config.bind_address {
            let listener =
                TcpListener::bind(bind_address).map_err(|_e| ProtocolError::TruncatedPayload {
                    field: "Failed to bind TcpListener address",
                })?;

            let local_addr =
                listener
                    .local_addr()
                    .map_err(|_| ProtocolError::TruncatedPayload {
                        field: "Failed to query local listener address",
                    })?;

            listener
                .set_nonblocking(true)
                .map_err(|_| ProtocolError::TruncatedPayload {
                    field: "Failed to set listener non-blocking",
                })?;

            ws_bound_str = Some(local_addr.to_string());
            *lock_unpoisoned(&inner.local_addr) = Some(local_addr);

            let inner_clone = inner.clone();
            let config_clone = config.clone();
            let ws_thread = thread::spawn(move || {
                while inner_clone.is_running() {
                    match listener.accept() {
                        Ok((stream, addr)) => {
                            let mut clients = lock_unpoisoned(&inner_clone.clients);
                            if clients.len() >= config_clone.max_connections {
                                drop(stream);
                                continue;
                            }

                            let client_id =
                                inner_clone.next_client_id.fetch_add(1, Ordering::Relaxed);
                            let (tx, rx) = crossbeam_channel::unbounded();
                            clients.insert(client_id, ClientHandle { tx });
                            drop(clients);

                            let worker_inner = inner_clone.clone();
                            let worker_config = config_clone.clone();
                            thread::spawn(move || {
                                run_ws_client_worker(
                                    client_id,
                                    stream,
                                    addr,
                                    rx,
                                    worker_inner,
                                    worker_config,
                                );
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                }
            });
            listener_threads.push(ws_thread);
        }

        let mut ipc_registration = None;
        if config.enable_ipc {
            let registry_dir = default_registry_dir();
            if let Err(err) = std::fs::create_dir_all(&registry_dir) {
                inner.running.store(false, Ordering::Relaxed);
                for h in listener_threads {
                    let _ = h.join();
                }
                let _ = inner.event_tx.send(ServerEvent::Warning(err.to_string()));
                return Err(ProtocolError::TruncatedPayload {
                    field: "Failed to create IPC registry directory",
                });
            }

            let endpoint_id = config
                .ipc_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(crate::transport_ipc::sanitize_endpoint_id)
                .unwrap_or_else(|| generate_endpoint_id(&config.server_uuid));

            let ipc_listener = match bind_ipc_listener(&endpoint_id, &registry_dir) {
                Ok(l) => l,
                Err(_) => {
                    inner.running.store(false, Ordering::Relaxed);
                    for h in listener_threads {
                        let _ = h.join();
                    }
                    return Err(ProtocolError::TruncatedPayload {
                        field: "Failed to bind IPC listener",
                    });
                }
            };

            let ipc_url = format_ipc_url(&endpoint_id, &registry_dir, true);
            let metadata = config.instance_info.clone().unwrap_or_default();
            let requires_token = config.require_token_on_ipc
                && config
                    .auth_token
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty());

            let guard = match InstanceRegistrationGuard::create(
                &registry_dir,
                &endpoint_id,
                ipc_url.clone(),
                config.server_uuid,
                metadata,
                requires_token,
                ws_bound_str,
            ) {
                Ok(g) => g,
                Err(_) => {
                    inner.running.store(false, Ordering::Relaxed);
                    for h in listener_threads {
                        let _ = h.join();
                    }
                    return Err(ProtocolError::TruncatedPayload {
                        field: "Failed to register local IPC instance",
                    });
                }
            };

            *lock_unpoisoned(&inner.ipc_url) = Some(ipc_url.clone());
            ipc_registration = Some(guard);

            let inner_clone = inner.clone();
            let config_clone = config.clone();
            let ipc_thread = thread::spawn(move || {
                while inner_clone.is_running() {
                    match ipc_listener.accept() {
                        Ok(stream) => {
                            let mut clients = lock_unpoisoned(&inner_clone.clients);
                            if clients.len() >= config_clone.max_connections {
                                drop(stream);
                                continue;
                            }

                            let client_id =
                                inner_clone.next_client_id.fetch_add(1, Ordering::Relaxed);
                            let (tx, rx) = crossbeam_channel::unbounded();
                            clients.insert(client_id, ClientHandle { tx });
                            drop(clients);

                            let worker_inner = inner_clone.clone();
                            let worker_config = config_clone.clone();
                            let peer_label = ipc_url.clone();
                            thread::spawn(move || {
                                run_ipc_client_worker(
                                    client_id,
                                    stream,
                                    peer_label,
                                    rx,
                                    worker_inner,
                                    worker_config,
                                );
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                }
            });
            listener_threads.push(ipc_thread);
        }

        Ok(Self {
            inner,
            listener_threads: std::sync::Mutex::new(listener_threads),
            ipc_registration: std::sync::Mutex::new(ipc_registration),
        })
    }

    /// Stops the server, closes all client connections, and terminates listeners.
    pub fn stop(&self) {
        self.inner.running.store(false, Ordering::Relaxed);

        {
            let mut clients = lock_unpoisoned(&self.inner.clients);
            for client in clients.values() {
                let _ = client.tx.send(ServerCommand::Close);
            }
            clients.clear();
        }

        self.inner.epoch.fetch_add(1, Ordering::Relaxed);
        self.inner.sequence.store(1, Ordering::Relaxed);
        *lock_unpoisoned(&self.inner.palette) = PaletteState::default();
        *lock_unpoisoned(&self.inner.selection) = None;

        let threads = {
            let mut guard = lock_unpoisoned(&self.listener_threads);
            std::mem::take(&mut *guard)
        };
        for handle in threads {
            let _ = handle.join();
        }

        // Drop IPC registration after listener exits so the socket file is safely unlinked.
        let _ = lock_unpoisoned(&self.ipc_registration).take();
        *lock_unpoisoned(&self.inner.ipc_url) = None;
    }

    /// Returns whether the server is actively running.
    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }

    /// Returns the assigned server instance UUID.
    pub fn server_uuid(&self) -> [u8; 16] {
        self.inner.server_uuid
    }

    /// Returns the number of currently connected clients.
    pub fn client_count(&self) -> usize {
        lock_unpoisoned(&self.inner.clients).len()
    }

    /// Returns the bound local WebSocket socket address, if WebSocket is enabled.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        *lock_unpoisoned(&self.inner.local_addr)
    }

    /// Returns the local IPC URL (e.g. `ipc://mtk-sync-...`), if IPC is enabled.
    pub fn ipc_url(&self) -> Option<String> {
        lock_unpoisoned(&self.inner.ipc_url).clone()
    }

    /// Updates descriptive metadata published in the local IPC discovery manifest.
    pub fn update_instance_metadata(&self, metadata: InstanceMetadata) {
        let mut guard = lock_unpoisoned(&self.ipc_registration);
        if let Some(reg) = guard.as_mut() {
            if let Err(err) = reg.update_metadata(metadata) {
                self.warn(err);
            }
        }
    }

    /// Sets the active selection bounding box.
    pub fn set_selection(&self, min_pos: IVec3, size: IVec3, dimension: String) {
        let mut sel = lock_unpoisoned(&self.inner.selection);
        let existing_hash = sel.as_ref().map(|s| s.state_hash).unwrap_or(0);
        *sel = Some(SelectionContext {
            min_pos,
            size,
            dimension,
            state_hash: existing_hash,
        });
        self.inner.sequence.fetch_add(1, Ordering::Relaxed);
    }

    /// Clears the active selection bounding box and broadcasts a Cancel frame.
    pub fn clear_selection(&self) {
        let mut sel = lock_unpoisoned(&self.inner.selection);
        *sel = None;
        let epoch = self.inner.epoch.load(Ordering::Relaxed);
        let cancel_pkt = Packet::Cancel {
            epoch,
            snapshot_uuid: [0u8; 16],
            reason: CancelReason::SelectionChanged,
        };
        if let Err(err) = self.broadcast_packet(&cancel_pkt) {
            self.warn(format!("failed to encode Cancel packet: {err}"));
        }
    }

    /// Authoritatively packages and streams a Baseline dataset to a client (or broadcasts if client_id == 0).
    pub fn submit_baseline(
        &self,
        client_id: u64,
        min_pos: IVec3,
        size: IVec3,
        dimension: String,
        voxel_indices: &[u32],
        block_palette: &[String],
        raw_biomes: &[u32],
        biome_palette: &[String],
        block_entities: &[(IVec3, String)],
    ) -> Result<u64, ProtocolError> {
        let entity_map: HashMap<IVec3, &str> = block_entities
            .iter()
            .map(|(p, s)| (*p, s.as_str()))
            .collect();

        let state_hash = compute_sparse_state_hash_from_raw(
            min_pos,
            size,
            &dimension,
            voxel_indices,
            block_palette,
            raw_biomes,
            biome_palette,
            &entity_map,
        );

        // Update selection context with computed state_hash
        {
            let mut sel = lock_unpoisoned(&self.inner.selection);
            *sel = Some(SelectionContext {
                min_pos,
                size,
                dimension: dimension.clone(),
                state_hash,
            });
        }

        // Initialize authoritative palette from baseline
        {
            let mut pal = lock_unpoisoned(&self.inner.palette);
            pal.lookup = block_palette
                .iter()
                .enumerate()
                .map(|(idx, s)| (s.clone(), idx as u32))
                .collect();
            pal.list = block_palette.to_vec();
            pal.revision = 1;
        }

        // Unique per baseline so a client assembling a newer stream drops chunks that arrive
        // late from an aborted one.
        let snapshot_uuid = self.inner.next_snapshot_uuid();
        let seq = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
        let ep = self.inner.epoch.load(Ordering::Relaxed);

        let encoder_config = BaselineEncoderConfig {
            max_chunk_size: 65536,
            compression_mode: 1, // LZ4
            compression_level: 0,
            epoch: ep,
            snapshot_uuid,
            snapshot_seq: seq,
            palette_revision: 1,
        };

        let stream = encode_baseline_from_raw(
            min_pos,
            size,
            &dimension,
            voxel_indices,
            block_palette,
            raw_biomes,
            biome_palette,
            block_entities,
            &encoder_config,
        )?;

        self.stream_baseline(client_id, &stream)?;
        Ok(state_hash)
    }

    /// Broadcasts a pre-encoded raw RFC frame to all connected clients.
    pub fn broadcast_packet_bytes(&self, bytes: Vec<u8>) {
        let clients = lock_unpoisoned(&self.inner.clients);
        for client in clients.values() {
            let _ = client.tx.send(ServerCommand::SendPacket(bytes.clone()));
        }
    }

    /// Sends a pre-encoded raw RFC frame to a specific connected client.
    pub fn send_packet_bytes(&self, client_id: u64, bytes: Vec<u8>) -> Result<(), ProtocolError> {
        let clients = lock_unpoisoned(&self.inner.clients);
        if let Some(client) = clients.get(&client_id) {
            let _ = client.tx.send(ServerCommand::SendPacket(bytes));
            Ok(())
        } else {
            Err(ProtocolError::TruncatedPayload {
                field: "client_id not found or disconnected",
            })
        }
    }

    /// Broadcasts incremental block changes and entity mutations, automatically managing the dynamic palette.
    pub fn submit_deltas(
        &self,
        rel_coords: &[IVec3],
        block_states: &[String],
        block_entities: &[(IVec3, Option<String>)],
    ) -> Result<(), ProtocolError> {
        let epoch = self.inner.epoch.load(Ordering::Relaxed);
        let mut new_palette_entries: Vec<(u16, String)> = Vec::new();
        let mut changes = Vec::with_capacity(rel_coords.len());

        {
            let mut pal = lock_unpoisoned(&self.inner.palette);
            for (i, coord) in rel_coords.iter().enumerate() {
                let state = &block_states[i];
                let state_id = if let Some(&id) = pal.lookup.get(state) {
                    id as u16
                } else {
                    let id = pal.list.len() as u16;
                    pal.list.push(state.clone());
                    pal.lookup.insert(state.clone(), id as u32);
                    new_palette_entries.push((id, state.clone()));
                    id
                };

                changes.push(DeltaChangeEntry {
                    rel_pos: *coord,
                    state_id,
                });
            }

            if !new_palette_entries.is_empty() {
                let old_rev = pal.revision;
                pal.revision += 1;
                let seq = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
                let pal_pkt = Packet::PaletteUpdate {
                    epoch,
                    seq_id: seq,
                    base_revision: old_rev,
                    new_revision: pal.revision,
                    entries: new_palette_entries,
                };
                self.broadcast_packet(&pal_pkt)?;
            }

            if !changes.is_empty() {
                for chunk in changes.chunks(16384) {
                    let seq = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
                    let delta_packets = encode_delta_op(epoch, seq, pal.revision, chunk.to_vec());
                    for pkt in &delta_packets {
                        self.broadcast_packet(pkt)?;
                    }
                }
            }
        }

        // Broadcast block entity changes
        for (coord, payload_opt) in block_entities {
            let seq = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
            let entity_data = payload_opt
                .as_deref()
                .and_then(mtk_model::BlockEntityData::from_visual_json);
            let ent_pkt = Packet::DeltaBlockEntity {
                epoch,
                seq_id: seq,
                rel_pos: *coord,
                entity: entity_data,
            };
            self.broadcast_packet(&ent_pkt)?;
        }

        Ok(())
    }

    /// Reports a non-fatal server-side problem to the host event stream.
    fn warn(&self, message: String) {
        let _ = self.inner.event_tx.send(ServerEvent::Warning(message));
    }

    /// Broadcasts an arbitrary RFC packet to all connected clients.
    pub fn broadcast_packet(&self, packet: &Packet) -> Result<(), ProtocolError> {
        let bytes = encode_packet(packet)?;
        let clients = lock_unpoisoned(&self.inner.clients);
        for client in clients.values() {
            let _ = client.tx.send(ServerCommand::SendPacket(bytes.clone()));
        }
        Ok(())
    }

    /// Streams an authoritative Baseline dataset to a specific client, or to all clients if client_id is 0.
    pub fn stream_baseline(
        &self,
        client_id: u64,
        stream: &BaselineStream,
    ) -> Result<(), ProtocolError> {
        let clients = lock_unpoisoned(&self.inner.clients);
        if client_id == 0 {
            for client in clients.values() {
                for pkt in &stream.packets {
                    let bytes = encode_packet(pkt)?;
                    let _ = client.tx.send(ServerCommand::SendPacket(bytes));
                }
            }
            return Ok(());
        }
        if let Some(client) = clients.get(&client_id) {
            for pkt in &stream.packets {
                let bytes = encode_packet(pkt)?;
                let _ = client.tx.send(ServerCommand::SendPacket(bytes));
            }
            Ok(())
        } else {
            Err(ProtocolError::TruncatedPayload {
                field: "client_id not found or disconnected",
            })
        }
    }

    /// Broadcasts an incremental Delta packet to all connected clients.
    pub fn broadcast_delta(&self, delta_packet: &Packet) -> Result<(), ProtocolError> {
        let bytes = encode_packet(delta_packet)?;
        let clients = lock_unpoisoned(&self.inner.clients);
        for client in clients.values() {
            let _ = client.tx.send(ServerCommand::SendPacket(bytes.clone()));
        }
        Ok(())
    }

    /// Polls pending server events delivered to the host environment.
    pub fn poll_events(&self) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.inner.event_rx.try_recv() {
            events.push(event);
        }
        events
    }
}

impl Drop for LiveSyncServer {
    fn drop(&mut self) {
        self.stop();
    }
}
