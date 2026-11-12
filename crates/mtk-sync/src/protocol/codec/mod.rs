//! # Protocol Codec Implementation (RFC v3.1)
//!
//! Encodes and decodes strongly-typed `Packet`s to and from little-endian binary buffers.

pub mod bitpack;
mod decode;
mod decode_baseline;
mod encode;

pub use bitpack::{choose_bits_per_item, pack_lsb_indices, unpack_lsb_indices};
pub use decode::compute_delta_batch_hash;
pub use decode::decode_packet;
pub use encode::encode_packet;
