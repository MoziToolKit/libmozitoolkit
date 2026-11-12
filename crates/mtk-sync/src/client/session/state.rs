use std::collections::HashMap;
use std::time::Instant;

use glam::IVec3;

use super::assembler::BaselineAssembler;
use super::staging::DeltaStagingBuffer;

/// Sync session lifecycle states (RFC Section 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncLifecycle {
    Disconnected,
    Connecting,
    Negotiating,
    ReceivingBaseline,
    ValidatingHash,
    ReplayingStaged,
    BuildingMesh,
    Live,
}

/// Internal session state data container.
#[doc(hidden)]
pub struct SessionState {
    pub lifecycle: SyncLifecycle,
    pub epoch: u64,
    pub sequence: u64,
    pub server_uuid: [u8; 16],
    /// Snapshot / world canonical state hash last confirmed by the server.
    pub current_state_hash: u64,
    pub min_pos: IVec3,
    pub size: IVec3,
    pub dimension: String,
    pub palette_revision: u32,
    pub palette_map: HashMap<u16, String>,
    pub(crate) staging: DeltaStagingBuffer,
    pub assembler: Option<BaselineAssembler>,
    pub last_mesh_time: Option<Instant>,
    pub pending_dirty_rebuild: bool,
    /// Timestamp of the most recent delta event received (for channel quiet debounce).
    pub last_delta_at: Option<Instant>,
    /// Timestamp when world first became dirty since last mesh emission.
    pub dirty_since: Option<Instant>,
    /// Last time any baseline/connection progress was observed (baseline watchdog).
    pub last_progress_at: Instant,
    /// Rate-limiter so a corrupt stream cannot trigger a RESYNC storm.
    pub resync_last_sent: Option<Instant>,
    /// Consecutive baseline validation failures for the current selection. After a
    /// few retries we stop auto-resyncing and wait for an explicit refresh / new
    /// selection instead of spinning forever.
    pub validation_failures: u32,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            lifecycle: SyncLifecycle::Connecting,
            epoch: 0,
            sequence: 0,
            server_uuid: [0u8; 16],
            current_state_hash: 0,
            min_pos: IVec3::ZERO,
            size: IVec3::ZERO,
            dimension: "minecraft:overworld".to_string(),
            palette_revision: 0,
            palette_map: HashMap::new(),
            staging: DeltaStagingBuffer::new(),
            assembler: None,
            last_mesh_time: None,
            pending_dirty_rebuild: false,
            last_delta_at: None,
            dirty_since: None,
            last_progress_at: Instant::now(),
            resync_last_sent: None,
            validation_failures: 0,
        }
    }

    /// Completely resets per-connection state (called for every new `start()`).
    pub(crate) fn reset_for_new_connection(&mut self) {
        *self = Self::new();
    }

    /// Drops any partially assembled baseline and waits for a fresh BASELINE_BEGIN.
    pub(crate) fn awaiting_baseline(&mut self) {
        self.assembler = None;
        self.staging.clear();
        self.lifecycle = SyncLifecycle::ReceivingBaseline;
        self.last_progress_at = Instant::now();
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}
