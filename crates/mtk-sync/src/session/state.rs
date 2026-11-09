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
pub(crate) struct SessionState {
    pub(crate) lifecycle: SyncLifecycle,
    pub(crate) epoch: u64,
    pub(crate) sequence: u64,
    pub(crate) server_uuid: [u8; 16],
    /// Snapshot / world canonical state hash last confirmed by the server.
    pub(crate) current_state_hash: u64,
    pub(crate) min_pos: IVec3,
    pub(crate) size: IVec3,
    pub(crate) dimension: String,
    pub(crate) palette_revision: u32,
    pub(crate) palette_map: HashMap<u16, String>,
    pub(crate) staging: DeltaStagingBuffer,
    pub(crate) assembler: Option<BaselineAssembler>,
    pub(crate) last_mesh_time: Option<Instant>,
    pub(crate) pending_dirty_rebuild: bool,
    /// Timestamp of the most recent delta event received (for channel quiet debounce).
    pub(crate) last_delta_at: Option<Instant>,
    /// Timestamp when world first became dirty since last mesh emission.
    pub(crate) dirty_since: Option<Instant>,
    /// Last time any baseline/connection progress was observed (baseline watchdog).
    pub(crate) last_progress_at: Instant,
    /// Rate-limiter so a corrupt stream cannot trigger a RESYNC storm.
    pub(crate) resync_last_sent: Option<Instant>,
    /// Consecutive baseline validation failures for the current selection. After a
    /// few retries we stop auto-resyncing and wait for an explicit refresh / new
    /// selection instead of spinning forever.
    pub(crate) validation_failures: u32,
}

impl SessionState {
    pub(crate) fn new() -> Self {
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
