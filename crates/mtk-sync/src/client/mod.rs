//! # Live Sync Client Domain
//!
//! Encapsulates WebSocket network transport, handshake lifecycle,
//! baseline stream assembly, and delta synchronization for clients (RFC v3.1 / v3.2).

pub(crate) mod events;
pub mod session;
pub(crate) mod transport;

pub use events::{normalize_stream_progress, SyncEvent};
pub use session::{BaselineAssembler, LiveSyncSession, SyncLifecycle};
pub(crate) use transport::{ClientCommand, ClientMessage, SyncClient};
