//! # Sync Session Dispatcher and State Machine (RFC v3.1 Section 5)
//!
//! Event-driven background worker thread managing handshake negotiation,
//! progressive baseline chunk assembly, snapshot barrier staging, gap detection,
//! and parallel Rayon world meshing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use glam::IVec3;
use mtk_voxel::VoxelWorld;

use super::assembler::BaselineAssembler;
use super::staging::DeltaStagingBuffer;
use crate::client::{ClientCommand, ClientMessage};
use crate::events::SyncEvent;
use crate::protocol::*;

/// Sync session lifecycle states (RFC Section 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncLifecycle {
    Disconnected,
    Connecting,
    Negotiating,
    ReceivingBaseline,
    ValidatingHash,
    ReplayingStaged,
    BuildingMesh,
    Live,
}

/// Internal session state data container.
pub(crate) struct SessionState {
    pub(crate) lifecycle: SyncLifecycle,
    pub(crate) epoch: u64,
    pub(crate) sequence: u64,
    pub(crate) server_uuid: [u8; 16],
    /// Snapshot / world canonical state hash last confirmed by the server.
    pub(crate) current_state_hash: u64,
    pub(crate) min_pos: IVec3,
    pub(crate) size: IVec3,
    pub(crate) dimension: String,
    pub(crate) palette_revision: u32,
    pub(crate) palette_map: HashMap<u16, String>,
    pub(crate) staging: DeltaStagingBuffer,
    pub(crate) assembler: Option<BaselineAssembler>,
    pub(crate) last_mesh_time: Option<Instant>,
    pub(crate) pending_dirty_rebuild: bool,
    /// Timestamp of the most recent delta event received (for channel quiet debounce).
    pub(crate) last_delta_at: Option<Instant>,
    /// Timestamp when world first became dirty since last mesh emission.
    pub(crate) dirty_since: Option<Instant>,
    /// Last time any baseline/connection progress was observed (baseline watchdog).
    pub(crate) last_progress_at: Instant,
    /// Rate-limiter so a corrupt stream cannot trigger a RESYNC storm.
    pub(crate) resync_last_sent: Option<Instant>,
    /// Consecutive baseline validation failures for the current selection. After a
    /// few retries we stop auto-resyncing and wait for an explicit refresh / new
    /// selection instead of spinning forever.
    pub(crate) validation_failures: u32,
}

impl SessionState {
    pub(crate) fn new() -> Self {
        Self {
            lifecycle: SyncLifecycle::Connecting,
            epoch: 0,
            sequence: 0,
            server_uuid: [0u8; 16],
            current_state_hash: 0,
            min_pos: IVec3::ZERO,
            size: IVec3::ZERO,
            dimension: "minecraft:overworld".to_string(),
            palette_revision: 0,
            palette_map: HashMap::new(),
            staging: DeltaStagingBuffer::new(),
            assembler: None,
            last_mesh_time: None,
            pending_dirty_rebuild: false,
            last_delta_at: None,
            dirty_since: None,
            last_progress_at: Instant::now(),
            resync_last_sent: None,
            validation_failures: 0,
        }
    }

    /// Completely resets per-connection state (called for every new `start()`).
    pub(crate) fn reset_for_new_connection(&mut self) {
        *self = Self::new();
    }

    /// Drops any partially assembled baseline and waits for a fresh BASELINE_BEGIN.
    pub(crate) fn awaiting_baseline(&mut self) {
        self.assembler = None;
        self.staging.clear();
        self.lifecycle = SyncLifecycle::ReceivingBaseline;
        self.last_progress_at = Instant::now();
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Acquires the session state mutex, transparently recovering from a poisoned lock
/// (a single malformed packet must never permanently wedge the session).
fn lock_state(state: &Arc<Mutex<SessionState>>) -> std::sync::MutexGuard<'_, SessionState> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Acquires the voxel world write lock, recovering from poison instead of panicking.
fn lock_world_write(
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

/// Sends a `RESYNC_REQUEST` at most once per second, preventing livelock storms
/// when a corrupt or stale baseline keeps arriving.
fn send_resync_request(
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    reason: ResyncReason,
    message: &str,
) {
    let now = Instant::now();
    if let Some(last) = state.resync_last_sent {
        if now.duration_since(last) < Duration::from_millis(900) {
            return;
        }
    }
    state.resync_last_sent = Some(now);
    state.last_progress_at = now;

    let resync_pkt = Packet::ResyncRequest {
        epoch: state.epoch,
        missing_from_seq: state.sequence,
        missing_to_seq: state.sequence + 1,
        reason,
    };
    let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&resync_pkt)));
    let _ = event_sender.send(SyncEvent::StreamProgress {
        stage: "sync_request".to_string(),
        current: 0,
        total: 100,
        message: message.to_string(),
    });
}

/// Aborts the in-flight baseline and returns to the "waiting for BASELINE_BEGIN" state.
fn abort_baseline(
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    reason: ResyncReason,
    message: &str,
) {
    state.awaiting_baseline();
    let _ = event_sender.send(SyncEvent::Warning(message.to_string()));
    send_resync_request(state, cmd_sender, event_sender, reason, message);
}

/// Detects stalled baselines (server crash, dropped chunk, superseded stream)
/// and recovers automatically instead of leaving the UI frozen at a mid-progress value.
fn check_baseline_watchdog(
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
    if state.validation_failures >= 3 {
        // Repeated hash/sequence failures for the same selection: do not spin
        // forever at mid-progress. A manual refresh or a new selection resets this.
        return;
    }
    let timeout = match state.lifecycle {
        SyncLifecycle::ReceivingBaseline => Duration::from_secs(20),
        SyncLifecycle::ReplayingStaged | SyncLifecycle::ValidatingHash => Duration::from_secs(10),
        _ => return,
    };
    if state.last_progress_at.elapsed() < timeout {
        return;
    }
    abort_baseline(
        state,
        cmd_sender,
        event_sender,
        ResyncReason::SequenceGap,
        "Baseline stream stalled; requesting a fresh snapshot...",
    );
}

/// Checks and performs debounced incremental dirty remeshing.
pub(crate) fn check_debounced_remesh(
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
            negotiated_version: _,
            server_uuid,
            epoch,
            server_seq,
            current_min,
            current_size,
            current_state_hash,
            decision,
            message_or_dimension,
        } => {
            state.server_uuid = server_uuid;
            state.current_state_hash = current_state_hash;
            state.epoch = epoch;
            state.min_pos = current_min;
            state.size = current_size;
            state.dimension = message_or_dimension.clone();

            let _ = event_sender.send(SyncEvent::SelectionUpdated {
                min_pos: current_min,
                size: current_size,
            });

            let decision_str = match decision {
                SyncDecision::VerifiedSkipRebuild => "VERIFIED_SKIP_REBUILD",
                SyncDecision::FullBaselineRequired => "FULL_BASELINE_REQUIRED",
                SyncDecision::Rejected => "REJECTED",
            };

            let _ = event_sender.send(SyncEvent::Handshake {
                epoch,
                sequence: server_seq,
                decision: decision_str.to_string(),
                decision_code: decision as u8,
                dimension: message_or_dimension.clone(),
            });

            // Every WELCOME starts a brand-new synchronization generation.
            // Never inherit a partially assembled baseline or staged deltas from
            // a previous connection / selection.
            state.awaiting_baseline();
            state.sequence = server_seq;

            match decision {
                SyncDecision::VerifiedSkipRebuild => {
                    // Update sequence and enter LIVE immediately without rebuilding geometry
                    state.assembler = None;
                    state.staging.clear();
                    state.lifecycle = SyncLifecycle::Live;
                    let _ = event_sender.send(SyncEvent::Verified {
                        is_verified: true,
                        message: "100% in sync with scene (Zero-Rebuild verified)".to_string(),
                    });
                }
                SyncDecision::FullBaselineRequired => {
                    state.lifecycle = SyncLifecycle::ReceivingBaseline;
                    let _ = event_sender.send(SyncEvent::StreamProgress {
                        stage: "sync_download".to_string(),
                        current: 0,
                        total: 100,
                        message: "Waiting for Baseline stream...".to_string(),
                    });
                }
                SyncDecision::Rejected => {
                    state.assembler = None;
                    state.staging.clear();
                    state.lifecycle = SyncLifecycle::Disconnected;
                    let _ = event_sender.send(SyncEvent::Error(format!(
                        "Connection rejected by server: {}",
                        message_or_dimension
                    )));
                }
            }
        }

        Packet::BaselineBegin {
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
            // Ignore stale baselines from a previous server epoch.
            if state.epoch != 0 && epoch != state.epoch {
                return;
            }
            if state.min_pos != min_pos || state.size != size {
                // A genuinely new selection gets a fresh retry budget.
                state.validation_failures = 0;
            }
            state.epoch = epoch;
            state.current_state_hash = state_hash;
            state.min_pos = min_pos;
            state.size = size;
            state.sequence = snapshot_seq;
            state.last_progress_at = Instant::now();
            let _ = event_sender.send(SyncEvent::SelectionUpdated { min_pos, size });

            // Deliberately keep staged deltas: those with seq <= snapshot_seq are
            // discarded by the replay barrier, while deltas that occurred after the
            // server captured this snapshot must survive and be replayed on top.
            // A partially received previous baseline is discarded though.
            state.palette_map.clear();
            for (idx, p) in block_palette.iter().enumerate() {
                state.palette_map.insert(idx as u16, p.clone());
            }

            match BaselineAssembler::new(&packet) {
                Ok(assembler) => {
                    state.assembler = Some(assembler);
                    state.lifecycle = SyncLifecycle::ReceivingBaseline;
                    let _ = event_sender.send(SyncEvent::StreamProgress {
                        stage: "sync_download".to_string(),
                        current: 0,
                        total: total_chunks as usize,
                        message: format!("Receiving Baseline stream (0/{})", total_chunks),
                    });
                }
                Err(reason) => {
                    let cancel_pkt = Packet::Cancel {
                        epoch: state.epoch,
                        snapshot_uuid,
                        reason,
                    };
                    let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&cancel_pkt)));
                    state.assembler = None;
                    state.lifecycle = SyncLifecycle::ReceivingBaseline;

                    let volume = (size.x as i64) * (size.y as i64) * (size.z as i64);
                    let detail = if volume > MAX_SELECTION_VOLUME as i64 {
                        format!(
                            "Selection volume {} exceeds the {} voxel protocol limit; shrink the selection and reconnect.",
                            volume, MAX_SELECTION_VOLUME
                        )
                    } else {
                        "Invalid baseline bounds received from server.".to_string()
                    };
                    let _ = event_sender.send(SyncEvent::Error(detail));
                }
            }
        }

        Packet::BaselineChunk {
            epoch,
            snapshot_uuid,
            chunk_index,
            total_chunks,
            ..
        } => {
            let Some(ref mut assembler) = state.assembler else {
                // A chunk without a matching BASELINE_BEGIN belongs to a superseded
                // stream; ignore it instead of triggering a resync storm.
                return;
            };

            // Silently drop chunks from a stale snapshot (e.g. the user moved the
            // selection while the previous large baseline was still in flight).
            if epoch != assembler.epoch
                || snapshot_uuid != assembler.snapshot_uuid
                || total_chunks != assembler.total_chunks
            {
                return;
            }

            match assembler.process_chunk(&packet) {
                Ok(()) => {
                    state.last_progress_at = Instant::now();
                    let _ = event_sender.send(SyncEvent::StreamProgress {
                        stage: "sync_download".to_string(),
                        current: (chunk_index + 1) as usize,
                        total: total_chunks as usize,
                        message: format!("Receiving chunk {}/{}", chunk_index + 1, total_chunks),
                    });
                }
                Err(reason) => {
                    abort_baseline(
                        state,
                        cmd_sender,
                        event_sender,
                        reason,
                        "Baseline chunk sequence corrupted; requesting a fresh snapshot...",
                    );
                }
            }
        }

        Packet::BaselineEntities {
            epoch,
            snapshot_uuid,
            entities,
        } => {
            if let Some(assembler) = state.assembler.as_mut() {
                if epoch == assembler.epoch && snapshot_uuid == assembler.snapshot_uuid {
                    assembler.block_entities.extend(entities);
                }
            }
        }

        Packet::BaselineEnd {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            total_chunks_sent,
            ..
        } => {
            let Some(assembler) = state.assembler.as_ref() else {
                // BASELINE_END without a matching BEGIN: either a duplicate from a
                // superseded stream or the result of a cancelled/oversized baseline.
                return;
            };

            // Ignore stale snapshot ends instead of resyncing forever.
            if epoch != assembler.epoch
                || snapshot_uuid != assembler.snapshot_uuid
                || snapshot_seq != assembler.snapshot_seq
                || total_chunks_sent != assembler.total_chunks
            {
                return;
            }

            state.lifecycle = SyncLifecycle::ValidatingHash;
            state.last_progress_at = Instant::now();
            let _ = event_sender.send(SyncEvent::StreamProgress {
                stage: "sync_meshing".to_string(),
                current: 50,
                total: 100,
                message: "Validating state hash and assembling voxels...".to_string(),
            });

            if let Some(assembler) = state.assembler.take() {
                match assembler.finalize(&packet) {
                    Ok(assembled_storage) => {
                        state.validation_failures = 0;
                        // 1. Sync assembled VoxelStorage to VoxelWorld
                        {
                            let mut w = lock_world_write(world);
                            w.sync_from_storage(&assembled_storage);
                        }

                        // 2. Commit Baseline ACK
                        let ack_pkt = Packet::BaselineAck {
                            epoch: state.epoch,
                            snapshot_uuid,
                            committed_seq: snapshot_seq,
                            result_code: BaselineAckResult::SuccessCommitted,
                        };
                        let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&ack_pkt)));

                        // 3. Replay in-flight staged deltas (S + 1 .. latest)
                        state.lifecycle = SyncLifecycle::ReplayingStaged;
                        let mut replay_gap = false;
                        match state.staging.drain_replaying_after_snapshot(snapshot_seq) {
                            Ok(replayed_packets) => {
                                let mut current_seq = snapshot_seq;
                                for pkt in replayed_packets {
                                    if let Packet::DeltaOp {
                                        seq_id, changes, ..
                                    } = pkt
                                    {
                                        current_seq = seq_id;
                                        apply_delta_entries_to_world(
                                            &changes,
                                            &state.palette_map,
                                            world,
                                            state.min_pos,
                                        );
                                    }
                                }
                                state.sequence = current_seq;
                            }
                            Err(_) => {
                                replay_gap = true;
                            }
                        }

                        // 4. Build unified MeshData using parallel Rayon mesher,
                        //    streaming real per-section progress to the host.
                        state.lifecycle = SyncLifecycle::BuildingMesh;
                        state.last_progress_at = Instant::now();
                        let _ = event_sender.send(SyncEvent::StreamProgress {
                            stage: "sync_meshing".to_string(),
                            current: 50,
                            total: 100,
                            message: "Building unified world mesh...".to_string(),
                        });

                        let last_pct = AtomicUsize::new(50);
                        let progress_cb = |report: mtk_core::ProgressReport| {
                            let pct = (report.current * 50)
                                .checked_div(report.total)
                                .map(|v| v + 50)
                                .unwrap_or(100);
                            // Rayon workers may report out of order; only emit
                            // monotonically increasing progress.
                            let prev = last_pct.fetch_max(pct, Ordering::SeqCst);
                            if pct > prev {
                                let _ = event_sender.send(SyncEvent::StreamProgress {
                                    stage: "sync_meshing".to_string(),
                                    current: pct.min(99),
                                    total: 100,
                                    message: report.message.clone(),
                                });
                            }
                        };

                        let (world_mesh, built_sections, profile) = {
                            let mut w = lock_world_write(world);
                            let _ = w.rebuild_all_with_progress(Some(&progress_cb));
                            let mesh = w.get_world_mesh().cloned().unwrap_or_default();
                            let count = w.storage.sections.len();
                            let profile = w.last_profile.clone();
                            (mesh, count, profile)
                        };

                        let gen = generation_atomic.fetch_add(1, Ordering::SeqCst) + 1;
                        let _ = event_sender.send(SyncEvent::WorldMeshReady {
                            mesh: world_mesh,
                            generation: gen,
                            profile,
                        });
                        let _ = event_sender.send(SyncEvent::StreamFinished { built_sections });

                        if replay_gap {
                            state.awaiting_baseline();
                            send_resync_request(
                                state,
                                cmd_sender,
                                event_sender,
                                ResyncReason::SequenceGap,
                                "Deltas during baseline were not contiguous; verifying against a fresh snapshot...",
                            );
                        } else {
                            state.lifecycle = SyncLifecycle::Live;
                            state.last_progress_at = Instant::now();
                            // Deltas may have arrived while the world mesh was built.
                            // Apply them now and schedule a debounced dirty remesh.
                            if !state.staging.is_empty() {
                                if let Ok(replayed_packets) =
                                    state.staging.drain_replaying_after_snapshot(state.sequence)
                                {
                                    for pkt in replayed_packets {
                                        if let Packet::DeltaOp {
                                            seq_id, changes, ..
                                        } = pkt
                                        {
                                            state.sequence = seq_id;
                                            apply_delta_entries_to_world(
                                                &changes,
                                                &state.palette_map,
                                                world,
                                                state.min_pos,
                                            );
                                            if state.dirty_since.is_none() {
                                                state.dirty_since = Some(Instant::now());
                                            }
                                            state.last_delta_at = Some(Instant::now());
                                            state.pending_dirty_rebuild = true;
                                        }
                                    }
                                } else {
                                    state.pending_dirty_rebuild = false;
                                    state.awaiting_baseline();
                                    send_resync_request(
                                        state,
                                        cmd_sender,
                                        event_sender,
                                        ResyncReason::SequenceGap,
                                        "Live delta gap detected after snapshot; requesting resync...",
                                    );
                                }
                            }
                        }
                    }
                    Err(resync_reason) => {
                        let ack_pkt = Packet::BaselineAck {
                            epoch: state.epoch,
                            snapshot_uuid,
                            committed_seq: snapshot_seq,
                            result_code: BaselineAckResult::ValidationFailed,
                        };
                        let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&ack_pkt)));

                        state.validation_failures = state.validation_failures.saturating_add(1);
                        if state.validation_failures >= 3 {
                            state.awaiting_baseline();
                            let _ = event_sender.send(SyncEvent::Error(format!(
                                "Baseline validation failed {:?} times for this selection;                                  click Refresh or change the Minecraft selection to retry.",
                                state.validation_failures
                            )));
                        } else {
                            // Hash mismatch / truncated stream: drop the baseline and
                            // re-request with a rate limit so the UI cannot freeze at 75%.
                            abort_baseline(
                                state,
                                cmd_sender,
                                event_sender,
                                resync_reason,
                                &format!(
                                    "Baseline validation failed ({:?}); requesting a fresh snapshot...",
                                    resync_reason
                                ),
                            );
                        }
                    }
                }
            }
        }

        Packet::DeltaOp {
            epoch,
            seq_id,
            count: _,
            ref changes,
            ..
        } => {
            if epoch != state.epoch {
                return;
            }

            // Deltas that arrive while a baseline is being assembled/validated or
            // while the world mesh is being built must be staged, never dropped.
            if state.lifecycle != SyncLifecycle::Live {
                if state.staging.push(seq_id, packet, bytes_len).is_err() {
                    abort_baseline(
                        state,
                        cmd_sender,
                        event_sender,
                        ResyncReason::StateHashMismatch,
                        "Delta staging buffer overflow; requesting a fresh snapshot...",
                    );
                }
                return;
            }

            let expected = state.sequence + 1;
            if seq_id == expected || (state.sequence == 0 && seq_id <= expected) {
                state.sequence = seq_id;
                let affected =
                    apply_delta_entries_to_world(changes, &state.palette_map, world, state.min_pos);
                if state.dirty_since.is_none() {
                    state.dirty_since = Some(Instant::now());
                }
                state.last_delta_at = Some(Instant::now());
                state.pending_dirty_rebuild = true;
                state.last_progress_at = Instant::now();
                let _ = event_sender.send(SyncEvent::DeltaApplied {
                    change_count: changes.len(),
                    affected_sections: affected,
                });
            } else if seq_id < expected {
                // Duplicate / already applied delta (e.g. replay overlap). Ignore.
            } else {
                // Sequence gap detected: recover from a clean snapshot instead of
                // silently diverging from the server.
                state.awaiting_baseline();
                send_resync_request(
                    state,
                    cmd_sender,
                    event_sender,
                    ResyncReason::SequenceGap,
                    &format!(
                        "Delta sequence gap (expected {}, got {}); resynchronizing...",
                        expected, seq_id
                    ),
                );
            }
        }

        Packet::PaletteUpdate {
            epoch,
            new_revision,
            entries,
            ..
        } => {
            if epoch == state.epoch {
                state.palette_revision = new_revision;
                for (id, str_val) in entries {
                    state.palette_map.insert(id, str_val);
                }
            }
        }

        Packet::DeltaEntity {
            epoch,
            rel_pos,
            ref entity,
            ..
        } => {
            if epoch == state.epoch {
                let wx = state.min_pos.x + rel_pos.x;
                let wy = state.min_pos.y + rel_pos.y;
                let wz = state.min_pos.z + rel_pos.z;
                let sec_coord = IVec3::new(wx >> 4, wy >> 4, wz >> 4);
                let mut w = lock_world_write(world);
                if let Some(sec) = w.storage.sections.get_mut(&sec_coord) {
                    let blx = (wx & 15) as usize;
                    let bly = (wy & 15) as usize;
                    let blz = (wz & 15) as usize;
                    let local_idx = mtk_voxel::block_index(blx, bly, blz) as u16;
                    let ent = if let Some(mut ed) = entity.clone() {
                        if let mtk_model::BlockEntityData::Banner {
                            ref mut base_color, ..
                        } = ed
                        {
                            let state_str = sec.get_local_state(blx, bly, blz);
                            if let Some(resolved_color) =
                                mtk_model::banner::extract_banner_color_from_state(state_str)
                            {
                                *base_color = resolved_color.to_string();
                            }
                        }
                        Some(ed)
                    } else {
                        None
                    };
                    sec.set_block_entity(local_idx, ent);
                    w.storage.dirty_sections.insert(sec_coord);
                    if state.dirty_since.is_none() {
                        state.dirty_since = Some(Instant::now());
                    }
                    state.last_delta_at = Some(Instant::now());
                    state.pending_dirty_rebuild = true;
                }
            }
        }

        Packet::Cancel { reason, .. } => {
            // Server aborted the current snapshot (selection change / resource limit).
            state.awaiting_baseline();
            let _ = event_sender.send(SyncEvent::Warning(format!(
                "Server cancelled the baseline stream ({:?}); waiting for the next snapshot...",
                reason
            )));
        }

        Packet::Ping { timestamp } => {
            let pong = Packet::Pong { timestamp };
            let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&pong)));
        }

        Packet::Error { message, .. } => {
            if matches!(
                state.lifecycle,
                SyncLifecycle::ReceivingBaseline
                    | SyncLifecycle::ValidatingHash
                    | SyncLifecycle::ReplayingStaged
            ) {
                state.awaiting_baseline();
            }
            let _ = event_sender.send(SyncEvent::Error(message));
        }

        _ => {}
    }
}

/// Applies a list of delta entries into VoxelWorld storage.
fn apply_delta_entries_to_world(
    changes: &[DeltaChangeEntry],
    palette_map: &HashMap<u16, String>,
    world: &Arc<RwLock<VoxelWorld>>,
    min_pos: IVec3,
) -> Vec<IVec3> {
    let mut string_changes: Vec<(i32, i32, i32, &str)> = Vec::with_capacity(changes.len());
    for entry in changes {
        let abs_x = min_pos.x + entry.rel_pos.x;
        let abs_y = min_pos.y + entry.rel_pos.y;
        let abs_z = min_pos.z + entry.rel_pos.z;
        let state_str = palette_map
            .get(&entry.state_id)
            .map(|s| s.as_str())
            .unwrap_or("minecraft:air");
        string_changes.push((abs_x, abs_y, abs_z, state_str));
    }

    let mut w = lock_world_write(world);
    w.apply_delta_update(min_pos.x, min_pos.y, min_pos.z, &string_changes);

    let mut affected = Vec::new();
    for &(x, y, z, _) in &string_changes {
        let sec = IVec3::new(x >> 4, y >> 4, z >> 4);
        if !affected.contains(&sec) {
            affected.push(sec);
        }
    }
    affected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_resync(rx: &Receiver<ClientCommand>) {
        match rx.try_recv().expect("expected a RESYNC_REQUEST command") {
            ClientCommand::Send(bytes) => match decode_packet(&bytes).expect("valid packet") {
                Packet::ResyncRequest { reason, .. } => {
                    assert_eq!(reason, ResyncReason::SequenceGap);
                }
                other => panic!("unexpected packet: {:?}", other),
            },
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn test_baseline_watchdog_recovers_from_stalled_stream() {
        let mut state = SessionState::new();
        state.lifecycle = SyncLifecycle::ReceivingBaseline;
        state.epoch = 1;
        state.sequence = 42;
        state.last_progress_at = Instant::now() - Duration::from_secs(120);

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();

        check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);

        assert_eq!(state.lifecycle, SyncLifecycle::ReceivingBaseline);
        assert_eq!(state.sequence, 42);
        expect_resync(&cmd_rx);
        assert!(ev_rx.try_recv().is_ok());
    }

    #[test]
    fn test_baseline_watchdog_is_rate_limited() {
        let mut state = SessionState::new();
        state.lifecycle = SyncLifecycle::ReceivingBaseline;
        state.last_progress_at = Instant::now() - Duration::from_secs(120);

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (ev_tx, _ev_rx) = crossbeam_channel::unbounded();

        check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);
        // Immediately stall again: the second recovery attempt must be suppressed.
        state.last_progress_at = Instant::now() - Duration::from_secs(120);
        check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);

        expect_resync(&cmd_rx);
        assert!(
            cmd_rx.try_recv().is_err(),
            "resync requests must be rate limited to avoid a livelock"
        );
    }

    #[test]
    fn test_live_state_is_never_touched_by_watchdog() {
        let mut state = SessionState::new();
        state.lifecycle = SyncLifecycle::Live;
        state.last_progress_at = Instant::now() - Duration::from_secs(3600);

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (ev_tx, _ev_rx) = crossbeam_channel::unbounded();

        check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);
        assert!(cmd_rx.try_recv().is_err());
        assert_eq!(state.lifecycle, SyncLifecycle::Live);
    }
}
