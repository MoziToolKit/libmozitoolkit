//! # Native WebSocket Live Sync Server (RFC v3.1 / v3.2)
//!
//! Provides an embedded, lightweight, high-performance WebSocket server for Minecraft Live Sync.
//! Host-agnostic and embeddable into Java (via JNI), Python (via PyO3), or headless CLI/tests.

pub mod config;
pub mod server;
pub(crate) mod worker;

pub use config::{generate_unique_server_uuid, ServerConfig, ServerEvent};
pub use server::LiveSyncServer;
