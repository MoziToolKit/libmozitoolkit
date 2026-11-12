//! # Live Sync Events (RFC v3.1)
//!
//! Event types dispatched from the Rust background sync thread to host applications.

use glam::IVec3;
use mtk_core::MeshData;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// High-level event produced by `LiveSyncSession`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SyncEvent {
    /// Connection lifecycle status has changed.
    StatusChange(String),

    /// Minecraft selection bounding box updated.
    SelectionUpdated { min_pos: IVec3, size: IVec3 },

    /// Handshake synchronization metadata.
    Handshake {
        epoch: u64,
        sequence: u64,
        decision: String,
        /// Raw RFC v3.1 decision code (0x00 VERIFIED_SKIP_REBUILD, 0x01 reserved,
        /// 0x02 FULL_BASELINE_REQUIRED, 0xFF REJECTED).
        decision_code: u8,
        dimension: String,
    },

    /// A unified, merged world mesh containing the entire active volume has been assembled.
    WorldMeshReady {
        mesh: MeshData,
        generation: u64,
        profile: Option<mtk_core::ExecutionProfile>,
    },

    /// Stream batch progress update.
    StreamProgress {
        stage: String,
        current: usize,
        total: usize,
        message: String,
    },

    /// Stream batch completed.
    StreamFinished { built_sections: usize },

    /// Incremental delta modifications applied.
    DeltaApplied {
        change_count: usize,
        affected_sections: Vec<IVec3>,
    },

    /// Continuous visual entities have been updated (created, modified, or removed).
    VisualEntitiesUpdated {
        upserted: Vec<mtk_model::EntityInstance>,
        deleted_uuids: Vec<String>,
    },

    /// Live sync validation completed.
    Verified { is_verified: bool, message: String },

    /// Warning or non-fatal error.
    Warning(String),

    /// Fatal or recoverable error message.
    Error(String),
}

/// Normalize a raw stream-progress report into a monotonic 0..100 percentage.
///
/// The dispatcher reports download progress as `current / total`, and meshing
/// progress as an already-weighted 50..100 `current`. Centralizing the phase
/// weights here prevents every host from re-deriving (and accidentally
/// double-counting) the same two-phase mapping.
pub fn normalize_stream_progress(stage: &str, current: usize, total: usize) -> f32 {
    let fraction = if total > 0 {
        (current as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let percent = match stage {
        "sync_request" => 0.0,
        "sync_download" => fraction * 50.0,
        "sync_meshing" => fraction * 100.0,
        _ => fraction * 100.0,
    };
    percent.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::normalize_stream_progress;

    #[test]
    fn progress_phases_are_normalized_without_double_weighting() {
        assert_eq!(normalize_stream_progress("sync_request", 0, 100), 0.0);
        assert_eq!(normalize_stream_progress("sync_download", 0, 200), 0.0);
        assert_eq!(normalize_stream_progress("sync_download", 100, 200), 25.0);
        assert_eq!(normalize_stream_progress("sync_download", 200, 200), 50.0);
        // The dispatcher already reports meshing as 50..100, so it must not be
        // offset by 50 again.
        assert_eq!(normalize_stream_progress("sync_meshing", 50, 100), 50.0);
        assert_eq!(normalize_stream_progress("sync_meshing", 75, 100), 75.0);
        assert_eq!(normalize_stream_progress("sync_meshing", 100, 100), 100.0);
        assert_eq!(normalize_stream_progress("other", 1, 4), 25.0);
    }
}
