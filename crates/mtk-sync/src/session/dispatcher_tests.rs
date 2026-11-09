use super::*;

fn expect_resync(rx: &Receiver<ClientCommand>) {
    match rx.try_recv().expect("expected a RESYNC_REQUEST command") {
        ClientCommand::Send(bytes) => match decode_packet(&bytes).expect("valid packet") {
            Packet::ResyncRequest { reason, .. } => {
                assert_eq!(reason, ResyncReason::SequenceGap);
            }
            other => panic!("unexpected packet: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn test_baseline_watchdog_recovers_from_stalled_stream() {
    let mut state = SessionState::new();
    state.lifecycle = SyncLifecycle::ReceivingBaseline;
    state.epoch = 1;
    state.sequence = 42;
    state.last_progress_at = Instant::now() - Duration::from_secs(120);

    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (ev_tx, ev_rx) = crossbeam_channel::unbounded();

    check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);

    assert_eq!(state.lifecycle, SyncLifecycle::ReceivingBaseline);
    assert_eq!(state.sequence, 42);
    expect_resync(&cmd_rx);
    assert!(ev_rx.try_recv().is_ok());
}

#[test]
fn test_baseline_watchdog_is_rate_limited() {
    let mut state = SessionState::new();
    state.lifecycle = SyncLifecycle::ReceivingBaseline;
    state.last_progress_at = Instant::now() - Duration::from_secs(120);

    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (ev_tx, _ev_rx) = crossbeam_channel::unbounded();

    check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);
    // Immediately stall again: the second recovery attempt must be suppressed.
    state.last_progress_at = Instant::now() - Duration::from_secs(120);
    check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);

    expect_resync(&cmd_rx);
    assert!(
        cmd_rx.try_recv().is_err(),
        "resync requests must be rate limited to avoid a livelock"
    );
}

#[test]
fn test_live_state_is_never_touched_by_watchdog() {
    let mut state = SessionState::new();
    state.lifecycle = SyncLifecycle::Live;
    state.last_progress_at = Instant::now() - Duration::from_secs(3600);

    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (ev_tx, _ev_rx) = crossbeam_channel::unbounded();

    check_baseline_watchdog(&mut state, &cmd_tx, &ev_tx);
    assert!(cmd_rx.try_recv().is_err());
    assert_eq!(state.lifecycle, SyncLifecycle::Live);
}
