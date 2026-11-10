//! # Protocol Codec Implementation (RFC v3.1)
//!
//! Encodes and decodes strongly-typed `Packet`s to and from little-endian binary buffers.

mod decode;
mod decode_baseline;
mod encode;

#[cfg(test)]
pub use decode::compute_delta_batch_hash;
pub use decode::decode_packet;
pub use encode::encode_packet;
