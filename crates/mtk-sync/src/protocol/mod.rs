//! # Live Sync Protocol Module (RFC v3.1)
//!
//! Binary serialization, packet definitions, xxHash64 hasher, and network codecs for Minecraft Yefira Live Sync.

mod codec;
mod constants;
mod error;
mod hasher;
mod packet;
mod quart;

pub use codec::compute_delta_batch_hash;
pub use codec::{
    choose_bits_per_item, decode_packet, encode_packet, pack_lsb_indices, unpack_lsb_indices,
};
pub(crate) use constants::*;
pub use constants::{
    BaselineAckResult, CancelReason, ErrorCode, Opcode, ResyncReason, SyncDecision,
    DELTA_VISUAL_ACTION_REMOVE, DELTA_VISUAL_ACTION_UPSERT, MAX_DELTA_BATCH_CHANGES,
    MAX_SELECTION_VOLUME, PROTOCOL_MAGIC, PROTOCOL_VERSION,
};
pub use error::ProtocolError;
pub use hasher::SparseBlockEntry;
pub use hasher::{
    compute_sparse_state_hash, compute_sparse_state_hash_for_point_cloud,
    compute_sparse_state_hash_for_storage, compute_visual_entities_hash,
};
pub(crate) use hasher::{
    direction_to_facing_byte, facing_byte_to_direction, format_uuid, parse_uuid_bits,
};
pub use packet::{DeltaChangeEntry, Packet, VisualEntityDelta};
pub use quart::QuartGrid;
