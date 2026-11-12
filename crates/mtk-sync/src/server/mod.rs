//! # Live Sync Server Domain
//!
//! Authoritative world state scanner, baseline stream encoder, chunker,
//! and delta broadcaster for RFC v3.1 / v3.2.

pub mod baseline;
pub mod decision;
pub mod delta;

pub use baseline::{
    encode_baseline_from_raw, encode_baseline_from_storage, BaselineEncoderConfig, BaselineStream,
};
pub use decision::{decide_handshake, HandshakeContext, HandshakeResult};
pub use delta::encode_delta_op;
