use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use super::state::{SessionState, SyncLifecycle};
use crate::client::ClientCommand;
use crate::events::SyncEvent;
use crate::protocol::{encode_packet, Packet, ResyncReason};

/// Sends a `RESYNC_REQUEST` at most once per second, preventing livelock storms
/// when a corrupt or stale baseline keeps arriving.
pub(crate) fn send_resync_request(
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    reason: ResyncReason,
    message: &str,
) {
    let now = Instant::now();
    if let Some(last) = state.resync_last_sent {
        if now.duration_since(last) < Duration::from_millis(900) {
            return;
        }
    }
    state.resync_last_sent = Some(now);
    state.last_progress_at = now;

    let resync_pkt = Packet::ResyncRequest {
        epoch: state.epoch,
        missing_from_seq: state.sequence,
        missing_to_seq: state.sequence + 1,
        reason,
    };
    let _ = cmd_sender.send(ClientCommand::Send(encode_packet(&resync_pkt)));
    let _ = event_sender.send(SyncEvent::StreamProgress {
        stage: "sync_request".to_string(),
        current: 0,
        total: 100,
        message: message.to_string(),
    });
}

/// Aborts the in-flight baseline and returns to the "waiting for BASELINE_BEGIN" state.
pub(crate) fn abort_baseline(
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
    reason: ResyncReason,
    message: &str,
) {
    state.awaiting_baseline();
    let _ = event_sender.send(SyncEvent::Warning(message.to_string()));
    send_resync_request(state, cmd_sender, event_sender, reason, message);
}

/// Detects stalled baselines (server crash, dropped chunk, superseded stream)
/// and recovers automatically instead of leaving the UI frozen at a mid-progress value.
pub(crate) fn check_baseline_watchdog(
    state: &mut SessionState,
    cmd_sender: &Sender<ClientCommand>,
    event_sender: &Sender<SyncEvent>,
) {
    if state.validation_failures >= 3 {
        // Repeated hash/sequence failures for the same selection: do not spin
        // forever at mid-progress. A manual refresh or a new selection resets this.
        return;
    }
    let timeout = match state.lifecycle {
        SyncLifecycle::ReceivingBaseline => Duration::from_secs(20),
        SyncLifecycle::ReplayingStaged | SyncLifecycle::ValidatingHash => Duration::from_secs(10),
        _ => return,
    };
    if state.last_progress_at.elapsed() < timeout {
        return;
    }
    abort_baseline(
        state,
        cmd_sender,
        event_sender,
        ResyncReason::SequenceGap,
        "Baseline stream stalled; requesting a fresh snapshot...",
    );
}
