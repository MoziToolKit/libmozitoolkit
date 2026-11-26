use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::discovery::InstanceMetadata;
use crate::protocol::ResyncReason;

/// Default WebSocket port for Live Sync (above dev ports, below OS ephemeral ranges).
pub const DEFAULT_WS_PORT: u16 = 21924;

/// Default WebSocket bind address for local/loopback Live Sync servers.
pub const DEFAULT_WS_BIND_ADDR: &str = "127.0.0.1:21924";

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
    /// Optional WebSocket bind address (e.g., "127.0.0.1:21924" or "127.0.0.1:0").
    /// Set to `None` to run in pure local IPC mode without binding any TCP port.
    pub bind_address: Option<String>,
    /// Enable the local IPC listener (Unix Domain Socket on Unix, Named Pipe on Windows).
    pub enable_ipc: bool,
    /// Optional logical IPC endpoint name; auto-generated when `None` and `enable_ipc` is true.
    pub ipc_name: Option<String>,
    /// Optional descriptive metadata published to local instance discovery when `enable_ipc` is true.
    pub instance_info: Option<InstanceMetadata>,
    /// Optional authentication token for clients.
    pub auth_token: Option<String>,
    /// Allow loopback WebSocket clients to connect without a token even if `auth_token` is set.
    pub allow_loopback_without_token: bool,
    /// Require `auth_token` on local IPC connections (default: `false`; user opt-in).
    pub require_token_on_ipc: bool,
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
            bind_address: Some(DEFAULT_WS_BIND_ADDR.to_string()),
            enable_ipc: false,
            ipc_name: None,
            instance_info: None,
            auth_token: None,
            allow_loopback_without_token: true,
            require_token_on_ipc: false,
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
    ClientConnected { client_id: u64, endpoint: String },
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
    /// A packet could not be encoded and was therefore not broadcast.
    ///
    /// Encoding only fails for invalid host-supplied data (an oversized identifier or a
    /// malformed UUID), which must be visible instead of silently dropping a frame.
    Warning(String),
}

#[derive(Debug)]
pub(crate) enum ServerCommand {
    SendPacket(Vec<u8>),
    Close,
}
