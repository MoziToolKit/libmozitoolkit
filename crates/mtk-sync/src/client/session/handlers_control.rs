use crossbeam_channel::Sender;

use super::state::{SessionState, SyncLifecycle};
use crate::client::ClientCommand;
use crate::protocol::{encode_packet, CancelReason, Packet, SyncDecision};
use crate::SyncEvent;

pub(crate) fn handle_welcome(
    server_uuid: [u8; 16],
    epoch: u64,
    server_seq: u64,
    current_min: glam::IVec3,
    current_size: glam::IVec3,
    current_state_hash: u64,
    decision: SyncDecision,
    message_or_dimension: String,
    state: &mut SessionState,
    event_sender: &Sender<SyncEvent>,
) {
    state.server_uuid = server_uuid;
    state.current_state_hash = current_state_hash;
    state.epoch = epoch;
    state.min_pos = current_min;
    state.size = current_size;
    state.dimension = message_or_dimension.clone();

    let _ = event_sender.send(SyncEvent::SelectionUpdated {
        min_pos: current_min,
        size: current_size,
    });

    let decision_str = match decision {
        SyncDecision::VerifiedSkipRebuild => "VERIFIED_SKIP_REBUILD",
        SyncDecision::FullBaselineRequired => "FULL_BASELINE_REQUIRED",
        SyncDecision::Rejected => "REJECTED",
    };

    let _ = event_sender.send(SyncEvent::Handshake {
        epoch,
        sequence: server_seq,
        decision: decision_str.to_string(),
        decision_code: decision as u8,
        dimension: message_or_dimension.clone(),
    });

    // Every WELCOME starts a brand-new synchronization generation.
    // Never inherit a partially assembled baseline or staged deltas from
    // a previous connection / selection.
    state.awaiting_baseline();
    state.sequence = server_seq;

    match decision {
        SyncDecision::VerifiedSkipRebuild => {
            // Update sequence and enter LIVE immediately without rebuilding geometry
            state.assembler = None;
            state.staging.clear();
            state.lifecycle = SyncLifecycle::Live;
            let _ = event_sender.send(SyncEvent::Verified {
                is_verified: true,
                message: "100% in sync with scene (Zero-Rebuild verified)".to_string(),
            });
        }
        SyncDecision::FullBaselineRequired => {
            state.lifecycle = SyncLifecycle::ReceivingBaseline;
            let _ = event_sender.send(SyncEvent::StreamProgress {
                stage: "sync_download".to_string(),
                current: 0,
                total: 100,
                message: "Waiting for Baseline stream...".to_string(),
            });
        }
        SyncDecision::Rejected => {
            state.assembler = None;
            state.staging.clear();
            state.lifecycle = SyncLifecycle::Disconnected;
            let _ = event_sender.send(SyncEvent::Error(format!(
                "Connection rejected by server: {}",
                message_or_dimension
            )));
        }
    }
}

pub(crate) fn handle_cancel(
    reason: CancelReason,
    state: &mut SessionState,
    event_sender: &Sender<SyncEvent>,
) {
    state.awaiting_baseline();
    let _ = event_sender.send(SyncEvent::Warning(format!(
        "Server cancelled the baseline stream ({:?}); waiting for the next snapshot...",
        reason
    )));
}

pub(crate) fn handle_ping(timestamp: u64, cmd_sender: &Sender<ClientCommand>) {
    let pong = Packet::Pong { timestamp };
    let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&pong)));
}

pub(crate) fn handle_error(
    message: String,
    state: &mut SessionState,
    event_sender: &Sender<SyncEvent>,
) {
    if matches!(
        state.lifecycle,
        SyncLifecycle::ReceivingBaseline
            | SyncLifecycle::ValidatingHash
            | SyncLifecycle::ReplayingStaged
    ) {
        state.awaiting_baseline();
    }
    let _ = event_sender.send(SyncEvent::Error(message));
}
