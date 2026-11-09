use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

use crossbeam_channel::Sender;
use mtk_voxel::VoxelWorld;

use super::assembler::BaselineAssembler;
use super::dispatcher::lock_world_write;
use super::handlers_delta::apply_delta_entries_to_world;
use super::state::{SessionState, SyncLifecycle};
use super::watchdog::{abort_baseline, send_resync_request};
use crate::client::ClientCommand;
use crate::events::SyncEvent;
use crate::protocol::{
    encode_packet, BaselineAckResult, Packet, ResyncReason, MAX_SELECTION_VOLUME,
};

pub(crate) fn handle_baseline_begin(
    packet: &Packet,
    epoch: u64,
    snapshot_uuid: [u8; 16],
    snapshot_seq: u64,
    min_pos: glam::IVec3,
    size: glam::IVec3,
    state_hash: u64,
    block_palette: &[String],
    total_chunks: u32,
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
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

    match BaselineAssembler::new(packet) {
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

pub(crate) fn handle_baseline_chunk(
    packet: &Packet,
    epoch: u64,
    snapshot_uuid: [u8; 16],
    chunk_index: u32,
    total_chunks: u32,
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
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

    match assembler.process_chunk(packet) {
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

pub(crate) fn handle_baseline_entities(
    epoch: u64,
    snapshot_uuid: [u8; 16],
    entities: Vec<(glam::IVec3, mtk_model::BlockEntityData)>,
    state: &mut SessionState,
) {
    if let Some(assembler) = state.assembler.as_mut() {
        if epoch == assembler.epoch && snapshot_uuid == assembler.snapshot_uuid {
            assembler.block_entities.extend(entities);
        }
    }
}

pub(crate) fn handle_baseline_end(
    packet: &Packet,
    epoch: u64,
    snapshot_uuid: [u8; 16],
    snapshot_seq: u64,
    total_chunks_sent: u32,
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    generation_atomic: &Arc<AtomicU64>,
) {
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
        match assembler.finalize(packet) {
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
                        "Baseline validation failed {:?} times for this selection; click Refresh or change the Minecraft selection to retry.",
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
