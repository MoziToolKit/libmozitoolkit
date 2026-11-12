#![forbid(unsafe_code)]
//! # mtk-sync
//!
//! **mtk-sync** provides native WebSocket Live Sync client implementations,
//! binary network protocol codecs, and session lifecycle controllers for libmtk (RFC v3.1).
//!
//! It connects directly to Minecraft streaming servers, deserializes block delta changes,
//! and synchronizes chunk data into `mtk-voxel` storage containers with DNA zero-copy handoff.

mod client;
mod events;
mod protocol;
mod session;

#[cfg(test)]
mod test_baseline_alignment;
#[cfg(test)]
mod test_decode_stress;
#[cfg(test)]
mod test_delta_seam_welding;
#[cfg(test)]
mod test_protocol;
#[cfg(test)]
mod test_sync_session;

pub use events::{normalize_stream_progress, SyncEvent};
pub use protocol::{
    compute_sparse_state_hash, compute_sparse_state_hash_for_point_cloud,
    compute_sparse_state_hash_for_storage, compute_visual_entities_hash, decode_packet,
    encode_packet, DeltaChangeEntry, Opcode, Packet, SyncDecision, VisualEntityDelta,
    DELTA_VISUAL_ACTION_REMOVE, DELTA_VISUAL_ACTION_UPSERT, PROTOCOL_MAGIC, PROTOCOL_VERSION,
};
pub use session::{LiveSyncSession, SyncLifecycle};
