use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

use crossbeam_channel::Sender;
use glam::IVec3;
use mtk_voxel::VoxelWorld;

use super::dispatcher::lock_world_write;
use super::state::{SessionState, SyncLifecycle};
use super::watchdog::{abort_baseline, send_resync_request};
use crate::client::ClientCommand;
use crate::protocol::{DeltaChangeEntry, Packet, ResyncReason};
use crate::SyncEvent;

pub(crate) fn handle_delta_blocks(
    packet: Packet,
    bytes_len: usize,
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
    let (epoch, seq_id) = match &packet {
        Packet::DeltaOp { epoch, seq_id, .. } => (*epoch, *seq_id),
        _ => return,
    };

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

    let changes = match packet {
        Packet::DeltaOp { changes, .. } => changes,
        _ => return,
    };

    match advance_sequence(state, seq_id) {
        SeqOutcome::Contiguous | SeqOutcome::Duplicate => {
            let (affected, rejected) = apply_delta_entries_to_world(
                &changes,
                &state.palette_map,
                world,
                state.min_pos,
                state.size,
            );
            if rejected > 0 {
                let _ = event_sender.send(SyncEvent::Warning(format!(
                    "Dropped {rejected} delta entries outside the selection ({}x{}x{} at {}, {}, {})",
                    state.size.x, state.size.y, state.size.z, state.min_pos.x, state.min_pos.y,
                    state.min_pos.z
                )));
            }
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
        }
        SeqOutcome::Gap { expected } => {
            // Recover from a clean snapshot instead of silently diverging from the server.
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
}

pub(crate) fn handle_palette_update(
    epoch: u64,
    seq_id: u64,
    new_revision: u32,
    entries: Vec<(u16, String)>,
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
    if epoch != state.epoch {
        return;
    }

    // The palette payload is self-describing (id -> name) and additive, so a replay is harmless.
    // The sequence still has to move, otherwise the next delta reads as a gap.
    match advance_sequence(state, seq_id) {
        SeqOutcome::Contiguous | SeqOutcome::Duplicate => {
            state.palette_revision = new_revision;
            for (id, str_val) in entries {
                state.palette_map.insert(id, str_val);
            }
        }
        SeqOutcome::Gap { expected } => {
            state.awaiting_baseline();
            send_resync_request(
                state,
                cmd_sender,
                event_sender,
                ResyncReason::SequenceGap,
                &format!(
                    "Palette update sequence gap (expected {}, got {}); resynchronizing...",
                    expected, seq_id
                ),
            );
        }
    }
}

pub(crate) fn handle_delta_entity(
    epoch: u64,
    seq_id: u64,
    rel_pos: IVec3,
    entity: &Option<mtk_model::BlockEntityData>,
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
    if epoch == state.epoch {
        match advance_sequence(state, seq_id) {
            SeqOutcome::Contiguous | SeqOutcome::Duplicate => {}
            SeqOutcome::Gap { expected } => {
                state.awaiting_baseline();
                send_resync_request(
                    state,
                    cmd_sender,
                    event_sender,
                    ResyncReason::SequenceGap,
                    &format!(
                        "Block entity sequence gap (expected {}, got {}); resynchronizing...",
                        expected, seq_id
                    ),
                );
                return;
            }
        }
        if rel_pos.x < 0
            || rel_pos.x >= state.size.x
            || rel_pos.y < 0
            || rel_pos.y >= state.size.y
            || rel_pos.z < 0
            || rel_pos.z >= state.size.z
        {
            return;
        }
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
            sec.set_block_entity(local_idx, entity.clone());
            w.storage.dirty_sections.insert(sec_coord);
            if state.dirty_since.is_none() {
                state.dirty_since = Some(Instant::now());
            }
            state.last_delta_at = Some(Instant::now());
            state.pending_dirty_rebuild = true;
        }
    }
}

/// Applies a list of delta entries into VoxelWorld storage.
///
/// Returns the touched sections plus the number of entries rejected for landing outside the
/// selection. `rel_pos` comes straight off the wire, so it must be range checked before it is
/// added to a world coordinate: an out-of-range value overflows `min_pos + rel_pos` and used to
/// panic the session worker thread, wedging the client in `ReceivingBaseline` forever.
pub(crate) fn apply_delta_entries_to_world(
    changes: &[DeltaChangeEntry],
    palette_map: &HashMap<u16, String>,
    world: &Arc<RwLock<VoxelWorld>>,
    min_pos: IVec3,
    size: IVec3,
) -> (Vec<IVec3>, usize) {
    let mut string_changes: Vec<(i32, i32, i32, &str)> = Vec::with_capacity(changes.len());
    let mut rejected = 0usize;
    for entry in changes {
        let rel = entry.rel_pos;
        let inside = rel.x >= 0
            && rel.y >= 0
            && rel.z >= 0
            && rel.x < size.x
            && rel.y < size.y
            && rel.z < size.z;
        if !inside {
            rejected += 1;
            continue;
        }
        let abs_x = min_pos.x + rel.x;
        let abs_y = min_pos.y + rel.y;
        let abs_z = min_pos.z + rel.z;
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
    (affected, rejected)
}

/// Applies continuous visual entity updates or removals into VoxelWorld storage.
pub(crate) fn handle_delta_visual_entity(
    epoch: u64,
    seq_id: u64,
    delta: &crate::protocol::VisualEntityDelta,
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
    event_sender: &Sender<SyncEvent>,
) {
    if state.epoch != 0 && epoch != state.epoch {
        return;
    }
    if seq_id > state.sequence {
        state.sequence = seq_id;
    }

    let mut upserted = Vec::new();
    let mut deleted_uuids = Vec::new();

    {
        let mut w = lock_world_write(world);
        match delta {
            crate::protocol::VisualEntityDelta::Remove { uuid } => {
                if !uuid.is_empty() {
                    w.storage
                        .entities
                        .retain(|e| e.uuid.as_deref() != Some(uuid));
                    deleted_uuids.push(uuid.clone());
                }
            }
            crate::protocol::VisualEntityDelta::Upsert(entity) => {
                let uuid_str = entity.uuid.as_deref().unwrap_or("");
                let mut found = false;
                if !uuid_str.is_empty() {
                    for existing in &mut w.storage.entities {
                        if existing.uuid.as_deref() == Some(uuid_str) {
                            *existing = entity.clone();
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    w.storage.entities.push(entity.clone());
                }
                upserted.push(entity.clone());
            }
        }
    }

    if !upserted.is_empty() || !deleted_uuids.is_empty() {
        let _ = event_sender.send(SyncEvent::VisualEntitiesUpdated {
            upserted,
            deleted_uuids,
        });
    }
}

/// Verdict on the sequence number carried by a sequenced server packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeqOutcome {
    /// Exactly the next packet; the session sequence was advanced.
    Contiguous,
    /// Already applied (replay overlap); payload is idempotent and the sequence stays put.
    Duplicate,
    /// One or more packets are missing; the caller must resynchronize.
    Gap { expected: u64 },
}

/// Advances the session sequence for a sequenced packet.
///
/// Every sequenced packet type (delta, palette, block entity) shares one counter: if one of them
/// silently skips the bookkeeping, the *next* delta looks like a gap and the client re-downloads
/// the whole selection.
pub(crate) fn advance_sequence(state: &mut SessionState, seq_id: u64) -> SeqOutcome {
    let expected = state.sequence + 1;
    if seq_id == expected || (state.sequence == 0 && seq_id <= expected) {
        state.sequence = seq_id;
        SeqOutcome::Contiguous
    } else if seq_id < expected {
        SeqOutcome::Duplicate
    } else {
        SeqOutcome::Gap { expected }
    }
}

#[cfg(test)]
mod tests {
    use super::{advance_sequence, SeqOutcome};
    use crate::client::session::state::SessionState;

    #[test]
    fn sequence_advances_only_on_the_next_packet() {
        let mut state = SessionState {
            sequence: 41,
            ..Default::default()
        };
        assert_eq!(advance_sequence(&mut state, 42), SeqOutcome::Contiguous);
        assert_eq!(state.sequence, 42);
        assert_eq!(advance_sequence(&mut state, 42), SeqOutcome::Duplicate);
        assert_eq!(state.sequence, 42);
        assert_eq!(
            advance_sequence(&mut state, 45),
            SeqOutcome::Gap { expected: 43 }
        );
        assert_eq!(state.sequence, 42, "a gap must not move the sequence");
    }
}
