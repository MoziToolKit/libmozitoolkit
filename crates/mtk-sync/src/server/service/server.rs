use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use glam::IVec3;

use crate::protocol::{
    compute_sparse_state_hash_from_raw, encode_packet, CancelReason, DeltaChangeEntry, Packet,
    ProtocolError, VisualEntityDelta,
};
use crate::server::baseline::{encode_baseline_from_raw, BaselineEncoderConfig, BaselineStream};
use crate::server::delta::encode_delta_op;

use super::config::{generate_unique_server_uuid, ServerCommand, ServerConfig, ServerEvent};
use super::state::{ClientHandle, PaletteState, SelectionContext, ServerInner};
use super::worker::run_client_worker;

/// The Native Live Sync WebSocket Server instance.
pub struct LiveSyncServer {
    inner: Arc<ServerInner>,
    listener_thread: Option<JoinHandle<()>>,
}

impl LiveSyncServer {
    /// Starts a new native WebSocket Live Sync server.
    pub fn start(mut config: ServerConfig) -> Result<Self, ProtocolError> {
        if config.server_uuid == [0u8; 16] {
            config.server_uuid = generate_unique_server_uuid();
        }

        let listener = TcpListener::bind(&config.bind_address).map_err(|_e| {
            ProtocolError::TruncatedPayload {
                field: "Failed to bind TcpListener address",
            }
        })?;

        let local_addr = listener
            .local_addr()
            .map_err(|_| ProtocolError::TruncatedPayload {
                field: "Failed to query local listener address",
            })?;

        listener
            .set_nonblocking(true)
            .map_err(|_| ProtocolError::TruncatedPayload {
                field: "Failed to set listener non-blocking",
            })?;

        let inner = Arc::new(ServerInner::new(config.server_uuid));
        *inner.local_addr.lock().unwrap() = Some(local_addr);

        let inner_clone = inner.clone();
        let config_clone = config.clone();

        let listener_thread = thread::spawn(move || {
            let next_client_id = AtomicU64::new(1);

            while inner_clone.is_running() {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        let mut clients = match inner_clone.clients.lock() {
                            Ok(c) => c,
                            Err(_) => break,
                        };

                        if clients.len() >= config_clone.max_connections {
                            drop(stream);
                            continue;
                        }

                        let client_id = next_client_id.fetch_add(1, Ordering::Relaxed);
                        let (tx, rx) = crossbeam_channel::unbounded();

                        clients.insert(client_id, ClientHandle { tx });
                        drop(clients);

                        let worker_inner = inner_clone.clone();
                        let worker_config = config_clone.clone();

                        thread::spawn(move || {
                            run_client_worker(
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

        Ok(Self {
            inner,
            listener_thread: Some(listener_thread),
        })
    }

    /// Stops the server, closes all client connections, and terminates the listener.
    pub fn stop(&mut self) {
        self.inner.running.store(false, Ordering::Relaxed);

        if let Ok(mut clients) = self.inner.clients.lock() {
            for client in clients.values() {
                let _ = client.tx.send(ServerCommand::Close);
            }
            clients.clear();
        }

        self.inner.epoch.fetch_add(1, Ordering::Relaxed);
        self.inner.sequence.store(1, Ordering::Relaxed);
        if let Ok(mut pal) = self.inner.palette.lock() {
            *pal = PaletteState::default();
        }
        if let Ok(mut sel) = self.inner.selection.lock() {
            *sel = None;
        }

        if let Some(handle) = self.listener_thread.take() {
            let _ = handle.join();
        }
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
        self.inner.clients.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Returns the bound local socket address.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        *self.inner.local_addr.lock().unwrap()
    }

    /// Sets the active selection bounding box.
    pub fn set_selection(&self, min_pos: IVec3, size: IVec3, dimension: String) {
        let mut sel = self.inner.selection.lock().unwrap();
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
        let mut sel = self.inner.selection.lock().unwrap();
        *sel = None;
        let epoch = self.inner.epoch.load(Ordering::Relaxed);
        let cancel_pkt = Packet::Cancel {
            epoch,
            snapshot_uuid: [0u8; 16],
            reason: CancelReason::SelectionChanged,
        };
        self.broadcast_packet(&cancel_pkt);
    }

    /// Returns the combined set of subscribed visual entity types from all connected clients.
    pub fn subscribed_visual_entities(&self) -> Vec<String> {
        let mut set = std::collections::HashSet::new();
        if let Ok(subs) = self.inner.client_subscriptions.lock() {
            for list in subs.values() {
                for s in list {
                    set.insert(s.clone());
                }
            }
        }
        let mut res: Vec<String> = set.into_iter().collect();
        res.sort();
        res
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
        visual_entities: &[mtk_model::EntityInstance],
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
            let mut sel = self.inner.selection.lock().unwrap();
            *sel = Some(SelectionContext {
                min_pos,
                size,
                dimension: dimension.clone(),
                state_hash,
            });
        }

        // Initialize authoritative palette from baseline
        {
            let mut pal = self.inner.palette.lock().unwrap();
            pal.lookup = block_palette
                .iter()
                .enumerate()
                .map(|(idx, s)| (s.clone(), idx as u32))
                .collect();
            pal.list = block_palette.to_vec();
            pal.revision = 1;
        }

        let mut snapshot_uuid = [0u8; 16];
        let now = Instant::now().elapsed().as_nanos();
        let seq = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
        let ep = self.inner.epoch.load(Ordering::Relaxed);
        snapshot_uuid[..8].copy_from_slice(&(now as u64).to_le_bytes());
        snapshot_uuid[8..].copy_from_slice(&((now >> 64) as u64).to_le_bytes());

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
            visual_entities,
            &encoder_config,
        )?;

        self.stream_baseline(client_id, &stream)?;
        Ok(state_hash)
    }

    /// Broadcasts an incremental Visual Entity change (Upsert or Remove).
    pub fn submit_visual_entity_delta(&self, delta: VisualEntityDelta) {
        let epoch = self.inner.epoch.load(Ordering::Relaxed);
        let seq_id = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
        let pkt = Packet::DeltaVisualEntity {
            epoch,
            seq_id,
            delta,
        };
        self.broadcast_packet(&pkt);
    }

    /// Broadcasts a pre-encoded raw RFC frame to all connected clients.
    pub fn broadcast_packet_bytes(&self, bytes: Vec<u8>) {
        if let Ok(clients) = self.inner.clients.lock() {
            for client in clients.values() {
                let _ = client.tx.send(ServerCommand::SendPacket(bytes.clone()));
            }
        }
    }

    /// Sends a pre-encoded raw RFC frame to a specific connected client.
    pub fn send_packet_bytes(&self, client_id: u64, bytes: Vec<u8>) -> Result<(), ProtocolError> {
        let clients = self.inner.clients.lock().unwrap();
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
            let mut pal = self.inner.palette.lock().unwrap();
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
                self.broadcast_packet(&pal_pkt);
            }

            if !changes.is_empty() {
                for chunk in changes.chunks(16384) {
                    let seq = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
                    let delta_packets = encode_delta_op(epoch, seq, pal.revision, chunk.to_vec());
                    for pkt in &delta_packets {
                        self.broadcast_packet(pkt);
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
            let ent_pkt = Packet::DeltaEntity {
                epoch,
                seq_id: seq,
                rel_pos: *coord,
                entity: entity_data,
            };
            self.broadcast_packet(&ent_pkt);
        }

        Ok(())
    }

    /// Broadcasts an arbitrary RFC packet to all connected clients.
    pub fn broadcast_packet(&self, packet: &Packet) {
        let bytes = encode_packet(packet);
        if let Ok(clients) = self.inner.clients.lock() {
            for client in clients.values() {
                let _ = client.tx.send(ServerCommand::SendPacket(bytes.clone()));
            }
        }
    }

    /// Streams an authoritative Baseline dataset to a specific client, or to all clients if client_id is 0.
    pub fn stream_baseline(
        &self,
        client_id: u64,
        stream: &BaselineStream,
    ) -> Result<(), ProtocolError> {
        let clients = self.inner.clients.lock().unwrap();
        if client_id == 0 {
            for client in clients.values() {
                for pkt in &stream.packets {
                    let bytes = encode_packet(pkt);
                    let _ = client.tx.send(ServerCommand::SendPacket(bytes));
                }
            }
            return Ok(());
        }
        if let Some(client) = clients.get(&client_id) {
            for pkt in &stream.packets {
                let bytes = encode_packet(pkt);
                let _ = client.tx.send(ServerCommand::SendPacket(bytes));
            }
            Ok(())
        } else {
            Err(ProtocolError::TruncatedPayload {
                field: "client_id not found or disconnected",
            })
        }
    }

    /// Broadcasts an incremental Delta or Visual Entity packet to all connected clients.
    pub fn broadcast_delta(&self, delta_packet: &Packet) {
        let bytes = encode_packet(delta_packet);
        let clients = self.inner.clients.lock().unwrap();
        for client in clients.values() {
            let _ = client.tx.send(ServerCommand::SendPacket(bytes.clone()));
        }
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
