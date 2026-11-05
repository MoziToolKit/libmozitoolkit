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
mod test_delta_seam_welding;

pub use events::{normalize_stream_progress, SyncEvent};
pub use protocol::{
    compute_canonical_state_hash, compute_delta_batch_hash, compute_sparse_state_hash,
    compute_sparse_state_hash_for_point_cloud, compute_sparse_state_hash_for_storage,
    decode_packet, encode_packet, BaselineAckResult, CancelReason, DeltaChangeEntry, ErrorCode,
    Opcode, Packet, ProtocolError, ResyncReason, SparseBlockEntry, SyncDecision, PROTOCOL_MAGIC,
    PROTOCOL_VERSION, SPARSE_CANONICAL_VERSION,
};
pub use session::{LiveSyncSession, SyncLifecycle};
