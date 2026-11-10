//! # Live Sync Protocol Module (RFC v3.1)
//!
//! Binary serialization, packet definitions, xxHash64 hasher, and network codecs for Minecraft Yefira Live Sync.

mod codec;
mod constants;
mod error;
mod hasher;
mod packet;

#[cfg(test)]
pub(crate) use codec::compute_delta_batch_hash;
pub use codec::{decode_packet, encode_packet};
pub(crate) use constants::*;
pub use constants::{Opcode, SyncDecision, PROTOCOL_MAGIC, PROTOCOL_VERSION};
#[cfg(test)]
pub(crate) use error::ProtocolError;
#[cfg(test)]
pub(crate) use hasher::SparseBlockEntry;
pub use hasher::{
    compute_sparse_state_hash, compute_sparse_state_hash_for_point_cloud,
    compute_sparse_state_hash_for_storage,
};
pub use packet::{DeltaChangeEntry, Packet};
