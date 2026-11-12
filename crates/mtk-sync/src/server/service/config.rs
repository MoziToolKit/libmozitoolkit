use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::protocol::ResyncReason;

/// Generates a unique 128-bit server instance UUID based on nanosecond timestamp and process ID.
pub fn generate_unique_server_uuid() -> [u8; 16] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(&(now as u64).to_le_bytes());
    uuid[8..12].copy_from_slice(&((now >> 64) as u32).to_le_bytes());
    uuid[12..16].copy_from_slice(&pid.to_le_bytes());
    uuid
}

/// Server configuration parameters.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Bind address (e.g., "127.0.0.1:8765" or "127.0.0.1:0" for ephemeral port).
    pub bind_address: String,
    /// Optional authentication token for remote clients.
    pub auth_token: Option<String>,
    /// Allow loopback clients to connect without a token even if auth_token is set.
    pub allow_loopback_without_token: bool,
    /// Unique 128-bit server instance UUID. If all zeros, a unique UUID is generated automatically.
    pub server_uuid: [u8; 16],
    /// Maximum concurrent client connections (default: 8).
    pub max_connections: usize,
    /// Handshake timeout before dropping unauthenticated connections.
    pub handshake_timeout: Duration,
    /// Interval between keepalive Ping frames.
    pub ping_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8765".to_string(),
            auth_token: None,
            allow_loopback_without_token: true,
            server_uuid: generate_unique_server_uuid(),
            max_connections: 8,
            handshake_timeout: Duration::from_millis(5000),
            ping_interval: Duration::from_millis(5000),
        }
    }
}

/// Events emitted by the server to the host environment (Java / Python / CLI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// A new client has connected and passed RFC handshake.
    ClientConnected { client_id: u64, addr: SocketAddr },
    /// A client has disconnected or timed out.
    ClientDisconnected { client_id: u64, reason: String },
    /// A client requires a full Baseline snapshot.
    BaselineRequested {
        client_id: u64,
        epoch: u64,
        snapshot_seq: u64,
    },
    /// A client requested a RESYNC gap recovery.
    ClientResync {
        client_id: u64,
        reason: ResyncReason,
    },
}

#[derive(Debug)]
pub(crate) enum ServerCommand {
    SendPacket(Vec<u8>),
    Close,
}
