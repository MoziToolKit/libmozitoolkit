//! # Live Sync Protocol Module (RFC v3.1)
//!
//! Binary serialization, packet definitions, xxHash64 hasher, and network codecs for Minecraft Yefira Live Sync.

mod codec;
mod constants;
mod error;
mod hasher;
mod packet;

pub use codec::{compute_delta_batch_hash, decode_packet, encode_packet};
pub(crate) use constants::*;
pub use constants::{
    BaselineAckResult, CancelReason, ErrorCode, Opcode, ResyncReason, SyncDecision, PROTOCOL_MAGIC,
    PROTOCOL_VERSION,
};
pub use error::ProtocolError;
pub use hasher::{
    compute_canonical_state_hash, compute_sparse_state_hash,
    compute_sparse_state_hash_for_point_cloud, compute_sparse_state_hash_for_storage,
    SparseBlockEntry, SPARSE_CANONICAL_VERSION,
};
pub use packet::{DeltaChangeEntry, Packet};
