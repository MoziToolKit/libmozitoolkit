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
use crate::events::SyncEvent;
use crate::protocol::{DeltaChangeEntry, Packet, ResyncReason};

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

    let expected = state.sequence + 1;
    if seq_id == expected || (state.sequence == 0 && seq_id <= expected) {
        state.sequence = seq_id;
        let affected =
            apply_delta_entries_to_world(&changes, &state.palette_map, world, state.min_pos);
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

pub(crate) fn handle_palette_update(
    epoch: u64,
    new_revision: u32,
    entries: Vec<(u16, String)>,
    state: &mut SessionState,
) {
    if epoch == state.epoch {
        state.palette_revision = new_revision;
        for (id, str_val) in entries {
            state.palette_map.insert(id, str_val);
        }
    }
}

pub(crate) fn handle_delta_entity(
    epoch: u64,
    rel_pos: IVec3,
    entity: &Option<mtk_model::BlockEntityData>,
    state: &mut SessionState,
    world: &Arc<RwLock<VoxelWorld>>,
) {
    if epoch == state.epoch {
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
pub(crate) fn apply_delta_entries_to_world(
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
