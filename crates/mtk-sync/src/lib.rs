#![forbid(unsafe_code)]
//! # mtk-sync
//!
//! **mtk-sync** provides native WebSocket Live Sync client implementations,
//! authoritative baseline/delta stream encoders, binary network protocol codecs,
//! and session lifecycle controllers for libmtk (RFC v3.1 / v3.2).
//!
//! It connects directly to Minecraft streaming servers or acts as an authoritative
//! stream transmitter, deserializes and serializes block delta changes,
//! and synchronizes chunk data into `mtk-voxel` storage containers with zero-copy handoff.

pub mod client;
pub mod protocol;
pub mod server;

#[doc(hidden)]
pub mod unstable {
    pub mod testing {
        pub use crate::client::session::{check_debounced_remesh, SessionState};
    }
}

pub use client::{
    normalize_stream_progress, BaselineAssembler, LiveSyncSession, SyncEvent, SyncLifecycle,
};
pub use protocol::{
    compute_sparse_state_hash, compute_sparse_state_hash_for_point_cloud,
    compute_sparse_state_hash_for_storage, compute_sparse_state_hash_from_raw,
    compute_visual_entities_hash, decode_packet, encode_packet, voxel_index_to_relative_coords,
    DeltaChangeEntry, Opcode, Packet, QuartGrid, SparseBlockEntry, SyncDecision, VisualEntityDelta,
    DELTA_VISUAL_ACTION_REMOVE, DELTA_VISUAL_ACTION_UPSERT, PROTOCOL_MAGIC, PROTOCOL_VERSION,
};
pub use server::{
    decide_handshake, encode_baseline_from_raw, encode_baseline_from_storage, encode_delta_op,
    BaselineEncoderConfig, BaselineStream, HandshakeContext, HandshakeResult, LiveSyncServer,
    ServerConfig, ServerEvent,
};
