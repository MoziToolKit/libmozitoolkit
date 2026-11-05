#![forbid(unsafe_code)]
//! # mtk-sync
//!
//! **mtk-sync** provides native WebSocket Live Sync client implementations,
//! binary network protocol codecs, and session lifecycle controllers for libmtk (RFC v3.1).
//!
//! It connects directly to Minecraft streaming servers, deserializes block delta changes,
//! and synchronizes chunk data into `mtk-voxel` storage containers with DNA zero-copy handoff.

pub mod client;
pub mod events;
pub mod protocol;
pub mod session;

pub use events::{normalize_stream_progress, SyncEvent};
pub use protocol::{
    compute_canonical_state_hash, decode_packet, encode_packet, BaselineAckResult, CancelReason,
    DeltaChangeEntry, ErrorCode, Opcode, Packet, ProtocolError, ResyncReason, SyncDecision,
    PROTOCOL_MAGIC, PROTOCOL_VERSION,
};
pub use session::{LiveSyncSession, SyncLifecycle};
