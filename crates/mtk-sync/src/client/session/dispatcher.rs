//! # Sync Session Dispatcher and State Machine (RFC v3.1 Section 5)
//!
//! Event-driven background worker thread managing handshake negotiation,
//! progressive baseline chunk assembly, snapshot barrier staging, gap detection,
//! and parallel Rayon world meshing.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use mtk_voxel::VoxelWorld;

use super::state::{SessionState, SyncLifecycle};
use super::watchdog::{check_baseline_watchdog, send_resync_request};
use crate::client::{ClientCommand, ClientMessage};
use crate::protocol::*;
use crate::SyncEvent;

/// Acquires the session state mutex, transparently recovering from a poisoned lock
/// (a single malformed packet must never permanently wedge the session).
pub(crate) fn lock_state(
    state: &Arc<Mutex<SessionState>>,
) -> std::sync::MutexGuard<'_, SessionState> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Acquires the voxel world write lock, recovering from poison instead of panicking.
pub(crate) fn lock_world_write(
    world: &Arc<RwLock<VoxelWorld>>,
) -> std::sync::RwLockWriteGuard<'_, VoxelWorld> {
    world.write().unwrap_or_else(|e| e.into_inner())
}

/// Ensures the host always observes a terminal status even if the worker thread
/// exits unexpectedly (panic / channel close), so the UI can never stay stuck on
/// a stale "CONNECTED" state.
struct WorkerExitGuard {
    running: Arc<AtomicBool>,
    is_connected: Arc<AtomicBool>,
    status_lock: Arc<RwLock<String>>,
    event_sender: Sender<SyncEvent>,
}

impl Drop for WorkerExitGuard {
    fn drop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            self.is_connected.store(false, Ordering::SeqCst);
            {
                let mut guard = self.status_lock.write().unwrap_or_else(|e| e.into_inner());
                *guard = "DISCONNECTED".to_string();
            }
            let _ = self
                .event_sender
                .send(SyncEvent::StatusChange("DISCONNECTED".to_string()));
        }
    }
}

/// Background worker loop processing incoming network messages and driving the sync state machine.
pub(crate) fn event_worker_loop(
    msg_receiver: Receiver<ClientMessage>,
    cmd_sender: Sender<ClientCommand>,
    world: Arc<RwLock<VoxelWorld>>,
    event_sender: Sender<SyncEvent>,
    running: Arc<AtomicBool>,
    is_connected: Arc<AtomicBool>,
    status_lock: Arc<RwLock<String>>,
    generation_atomic: Arc<AtomicU64>,
    state_lock: Arc<Mutex<SessionState>>,
    initial_hello: Packet,
) {
    let _exit_guard = WorkerExitGuard {
        running: running.clone(),
        is_connected: is_connected.clone(),
        status_lock: status_lock.clone(),
        event_sender: event_sender.clone(),
    };

    while running.load(Ordering::Relaxed) {
        match msg_receiver.recv_timeout(Duration::from_millis(15)) {
            Ok(msg) => {
                let should_continue = dispatch_single_client_message(
                    msg,
                    &state_lock,
                    &status_lock,
                    &is_connected,
                    &running,
                    &world,
                    &cmd_sender,
                    &event_sender,
                    &generation_atomic,
                    &initial_hello,
                );
                if !should_continue {
                    break;
                }

                // Drain all immediately available queued messages from the network channel
                // before evaluating debounced remeshing (ensures companion DeltaEntity packets
                // are applied in the same cycle as DeltaOp).
                while let Ok(next_msg) = msg_receiver.try_recv() {
                    let cont = dispatch_single_client_message(
                        next_msg,
                        &state_lock,
                        &status_lock,
                        &is_connected,
                        &running,
                        &world,
                        &cmd_sender,
                        &event_sender,
                        &generation_atomic,
                        &initial_hello,
                    );
                    if !cont {
                        break;
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut state = lock_state(&state_lock);
            check_baseline_watchdog(&mut state, &cmd_sender, &event_sender);
            check_debounced_remesh(&mut state, &world, &event_sender, &generation_atomic);
        }));
        if outcome.is_err() {
            let mut state = lock_state(&state_lock);
            state.awaiting_baseline();
            let _ = event_sender.send(SyncEvent::Error(
                "Internal sync error during remeshing".to_string(),
            ));
        }
    }
}

/// Dispatches a single client message, updating session state or routing network frames.
/// Returns false if the connection was terminated.
#[allow(clippy::too_many_arguments)]
fn dispatch_single_client_message(
    msg: ClientMessage,
    state_lock: &Arc<Mutex<SessionState>>,
    status_lock: &Arc<RwLock<String>>,
    is_connected: &Arc<AtomicBool>,
    running: &Arc<AtomicBool>,
    world: &Arc<RwLock<VoxelWorld>>,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    generation_atomic: &Arc<AtomicU64>,
    initial_hello: &Packet,
) -> bool {
    match msg {
        ClientMessage::Status(status) => {
            let conn = status == "CONNECTED";
            is_connected.store(conn, Ordering::SeqCst);
            {
                let mut guard = status_lock.write().unwrap_or_else(|e| e.into_inner());
                *guard = status.clone();
            }
            let _ = event_sender.send(SyncEvent::StatusChange(status));

            if conn {
                {
                    let mut state = lock_state(state_lock);
                    state.lifecycle = SyncLifecycle::Negotiating;
                    state.last_progress_at = Instant::now();
                }
                // Every worker start supplies a HELLO carrying the session auth token.
                let hello_pkt = initial_hello.clone();
                let bytes = encode_packet(&hello_pkt);
                let _ = cmd_sender.send(ClientCommand::Send(bytes));
            }
            true
        }
        ClientMessage::Warning(message) => {
            let _ = event_sender.send(SyncEvent::Warning(message));
            true
        }
        ClientMessage::ConnectionLost => {
            is_connected.store(false, Ordering::SeqCst);
            {
                let mut state = lock_state(state_lock);
                state.awaiting_baseline();
                state.lifecycle = SyncLifecycle::Connecting;
            }
            true
        }
        ClientMessage::Disconnected => {
            is_connected.store(false, Ordering::SeqCst);
            {
                let mut guard = status_lock.write().unwrap_or_else(|e| e.into_inner());
                *guard = "DISCONNECTED".to_string();
            }
            running.store(false, Ordering::SeqCst);
            {
                let mut state = lock_state(state_lock);
                state.awaiting_baseline();
                state.lifecycle = SyncLifecycle::Disconnected;
            }
            let _ = event_sender.send(SyncEvent::StatusChange("DISCONNECTED".to_string()));
            false
        }
        ClientMessage::SkinDownloaded(_key) => {
            let mut w = lock_world_write(world);
            if w.ensure_dynamic_entities_resolved() {
                let mut state = lock_state(state_lock);
                state.pending_dirty_rebuild = true;
                if state.dirty_since.is_none() {
                    state.dirty_since = Some(Instant::now());
                }
                state.last_delta_at = Some(Instant::now());
            }
            true
        }
        ClientMessage::PacketReceived { packet, bytes } => {
            let mut state = lock_state(state_lock);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handle_packet(
                    packet,
                    bytes,
                    &mut state,
                    world,
                    cmd_sender,
                    event_sender,
                    generation_atomic,
                );
            }));
            if outcome.is_err() {
                state.awaiting_baseline();
                let _ = event_sender.send(SyncEvent::Error(
                    "Internal sync error while processing a packet".to_string(),
                ));
                send_resync_request(
                    &mut state,
                    cmd_sender,
                    event_sender,
                    ResyncReason::ChunkCorrupted,
                    "Internal sync error; requesting a fresh snapshot...",
                );
            }
            true
        }
    }
}

/// Checks and performs debounced incremental dirty remeshing.
#[doc(hidden)]
pub fn check_debounced_remesh(
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
    event_sender: &Sender<SyncEvent>,
    generation_atomic: &Arc<AtomicU64>,
) {
    if !state.pending_dirty_rebuild || state.lifecycle != SyncLifecycle::Live {
        return;
    }

    let now = Instant::now();
    let quiet_elapsed = state
        .last_delta_at
        .map(|t| now.duration_since(t))
        .unwrap_or(Duration::from_millis(100));
    let dirty_elapsed = state
        .dirty_since
        .map(|t| now.duration_since(t))
        .unwrap_or(Duration::from_millis(100));

    // True debounce window: wait for 30ms of channel silence after the latest delta packet
    // (allows companion DELTA_ENTITY packets and consecutive rapid block edits to settle).
    // If continuous editing lasts > 100ms, flush to maintain interactive visual feedback.
    let is_quiet = quiet_elapsed >= Duration::from_millis(30);
    let max_deadline_hit = dirty_elapsed >= Duration::from_millis(100);

    let min_mesh_interval_passed = state
        .last_mesh_time
        .is_none_or(|t| now.duration_since(t) >= Duration::from_millis(50));

    if (is_quiet || max_deadline_hit) && min_mesh_interval_passed {
        state.pending_dirty_rebuild = false;
        state.dirty_since = None;
        state.last_mesh_time = Some(now);

        let mut w = lock_world_write(world);
        let _ = w.rebuild_dirty_in_place();
        if let Some(wm) = w.get_world_mesh().cloned() {
            let gen = generation_atomic.fetch_add(1, Ordering::SeqCst) + 1;
            let profile = w.last_profile.clone();
            let _ = event_sender.send(SyncEvent::WorldMeshReady {
                mesh: wm,
                generation: gen,
                profile,
            });
        }
    }
}

/// Dispatches an individual packet against the session state machine.
pub(crate) fn handle_packet(
    packet: Packet,
    bytes_len: usize,
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    generation_atomic: &Arc<AtomicU64>,
) {
    match packet {
        Packet::Welcome {
            server_uuid,
            epoch,
            server_seq,
            current_min,
            current_size,
            current_state_hash,
            decision,
            message_or_dimension,
            ..
        } => {
            super::handlers_control::handle_welcome(
                server_uuid,
                epoch,
                server_seq,
                current_min,
                current_size,
                current_state_hash,
                decision,
                message_or_dimension,
                state,
                event_sender,
            );
        }
        ref begin @ Packet::BaselineBegin {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            min_pos,
            size,
            state_hash,
            ref block_palette,
            total_chunks,
            ..
        } => {
            super::handlers_baseline::handle_baseline_begin(
                begin,
                epoch,
                snapshot_uuid,
                snapshot_seq,
                min_pos,
                size,
                state_hash,
                block_palette,
                total_chunks,
                state,
                cmd_sender,
                event_sender,
            );
        }
        ref chunk @ Packet::BaselineChunk {
            epoch,
            snapshot_uuid,
            chunk_index,
            total_chunks,
            ..
        } => {
            super::handlers_baseline::handle_baseline_chunk(
                chunk,
                epoch,
                snapshot_uuid,
                chunk_index,
                total_chunks,
                state,
                cmd_sender,
                event_sender,
            );
        }
        Packet::BaselineEntities {
            epoch,
            snapshot_uuid,
            entities,
        } => {
            super::handlers_baseline::handle_baseline_entities(
                epoch,
                snapshot_uuid,
                entities,
                state,
            );
        }
        Packet::BaselineVisualEntities {
            epoch,
            entities_hash,
            ref entities,
            ..
        } => {
            super::handlers_baseline::handle_baseline_visual_entities(
                epoch,
                entities_hash,
                entities,
                state,
            );
        }
        ref end @ Packet::BaselineEnd {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            total_chunks_sent,
            ..
        } => {
            super::handlers_baseline::handle_baseline_end(
                end,
                epoch,
                snapshot_uuid,
                snapshot_seq,
                total_chunks_sent,
                state,
                world,
                cmd_sender,
                event_sender,
                generation_atomic,
            );
        }
        packet @ Packet::DeltaOp { .. } => {
            super::handlers_delta::handle_delta_blocks(
                packet,
                bytes_len,
                state,
                world,
                cmd_sender,
                event_sender,
            );
        }
        Packet::PaletteUpdate {
            epoch,
            new_revision,
            entries,
            ..
        } => {
            super::handlers_delta::handle_palette_update(epoch, new_revision, entries, state);
        }
        Packet::DeltaEntity {
            epoch,
            rel_pos,
            ref entity,
            ..
        } => {
            super::handlers_delta::handle_delta_entity(epoch, rel_pos, entity, state, world);
        }
        Packet::DeltaVisualEntity {
            epoch,
            seq_id,
            ref delta,
        } => {
            super::handlers_delta::handle_delta_visual_entity(
                epoch,
                seq_id,
                delta,
                state,
                world,
                event_sender,
            );
        }
        Packet::Cancel { reason, .. } => {
            super::handlers_control::handle_cancel(reason, state, event_sender);
        }
        Packet::Ping { timestamp } => {
            super::handlers_control::handle_ping(timestamp, cmd_sender);
        }
        Packet::Error { message, .. } => {
            super::handlers_control::handle_error(message, state, event_sender);
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "dispatcher_tests.rs"]
mod tests;
