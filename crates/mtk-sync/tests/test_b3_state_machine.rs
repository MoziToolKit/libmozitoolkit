//! Live-sync session state machine driven by a scripted protocol peer.
//!
//! These tests exercise the exact bug class that used to require a running Minecraft client:
//! sequence bookkeeping, staging during a baseline, entity deletions and disconnect hygiene.
//! Everything here is in-process: no server binary, no client, no assets.

mod support;

use std::time::{Duration, Instant};

use glam::IVec3;
use mtk_sync::{LiveSyncSession, Packet};
use mtk_voxel::MesherConfig;
use support::{delta_op, welcome_live, FakePeer};

const EPOCH: u64 = 9;
const MIN: IVec3 = IVec3::new(0, 64, 0);
const SIZE: IVec3 = IVec3::new(16, 16, 16);

fn start_session(peer: &FakePeer) -> LiveSyncSession {
    let mut session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);
    session
        .start_with_hello(&peer.url, false, 1, None)
        .expect("session must start");
    peer.wait_for(Duration::from_secs(5), "HELLO", |packet| {
        matches!(packet, Packet::Hello { .. })
    });
    session
}

/// Polls until the session has processed a `WELCOME` and reached `LIVE`.
///
/// The lifecycle starts at `Live` before any handshake, so waiting on it alone races the
/// background transport thread; the handshake event is the real edge.
fn wait_until_live(session: &mut LiveSyncSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen_handshake = false;
    while Instant::now() < deadline {
        for event in session.poll_events() {
            if matches!(event, mtk_sync::SyncEvent::Handshake { .. }) {
                seen_handshake = true;
            }
        }
        if seen_handshake && session.lifecycle() == mtk_sync::SyncLifecycle::Live {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "session never processed WELCOME: handshake={seen_handshake}, lifecycle={:?}, status={}",
        session.lifecycle(),
        session.status()
    );
}

fn wait_for_sequence(session: &mut LiveSyncSession, target: u64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && session.current_sequence() < target {
        session.poll_events();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn is_resync(packet: &Packet) -> bool {
    matches!(packet, Packet::ResyncRequest { .. })
}

/// Y-01: a palette update carries a sequence number, so it must advance the session sequence.
///
/// Dropping it made the *next* delta look like a gap (expected old+1, got old+3), which sent a
/// `RESYNC_REQUEST` and re-downloaded the whole selection on every "new block state" edit.
#[test]
fn palette_update_advances_the_sequence() {
    let peer = FakePeer::start();
    let mut session = start_session(&peer);

    peer.send(&[welcome_live(EPOCH, 42, MIN, SIZE)]);
    wait_until_live(&mut session);

    peer.send(&[delta_op(EPOCH, 43, 1, IVec3::new(1, 0, 1), 1)]);
    wait_for_sequence(&mut session, 43);
    assert_eq!(
        session.current_sequence(),
        43,
        "a contiguous delta must advance the sequence"
    );

    peer.send(&[Packet::PaletteUpdate {
        epoch: EPOCH,
        seq_id: 44,
        base_revision: 1,
        new_revision: 2,
        entries: vec![(2, "minecraft:stone_bricks".to_string())],
    }]);
    peer.send(&[delta_op(EPOCH, 45, 2, IVec3::new(2, 0, 2), 2)]);

    wait_for_sequence(&mut session, 45);

    let resyncs = peer.drain().into_iter().filter(is_resync).count();
    assert_eq!(
        resyncs, 0,
        "a palette update must not look like a sequence gap (sent {resyncs} resync requests)"
    );
    assert_eq!(
        session.current_sequence(),
        45,
        "palette update must advance the sequence to 44 so delta 45 is contiguous"
    );
}

/// Y-02: block entity deltas carry a sequence number too; skipping it burns a delta slot.
#[test]
fn block_entity_delta_advances_the_sequence() {
    let peer = FakePeer::start();
    let mut session = start_session(&peer);

    peer.send(&[welcome_live(EPOCH, 7, MIN, SIZE)]);
    wait_until_live(&mut session);

    peer.send(&[Packet::DeltaBlockEntity {
        epoch: EPOCH,
        seq_id: 8,
        rel_pos: IVec3::new(1, 0, 1),
        entity: Some(mtk_model::BlockEntityData::DecoratedPot {
            sherds: [const { None }; 4],
        }),
    }]);
    wait_for_sequence(&mut session, 8);
    assert_eq!(
        session.current_sequence(),
        8,
        "a block entity delta must advance the sequence"
    );

    peer.send(&[delta_op(EPOCH, 9, 1, IVec3::new(2, 0, 2), 1)]);
    wait_for_sequence(&mut session, 9);

    let resyncs = peer.drain().into_iter().filter(is_resync).count();
    assert_eq!(
        resyncs, 0,
        "the following delta must be contiguous, not a gap"
    );
    assert_eq!(session.current_sequence(), 9);
}

/// Y-03: a delta entry outside the selection must be rejected, not overflow `min_pos + rel_pos`.
///
/// `rel_pos` is part of the wire payload, so a buggy or hostile server can send anything. Adding
/// it to a world coordinate without a range check panics in debug builds and wraps in release.
#[test]
fn out_of_bounds_delta_entry_is_rejected_without_panicking() {
    let peer = FakePeer::start();
    let mut session = start_session(&peer);

    // Selection starts at x = 1 so `min.x + i32::MAX` overflows.
    let min = IVec3::new(1, 64, 0);
    peer.send(&[
        Packet::Welcome {
            negotiated_version: mtk_sync::PROTOCOL_VERSION as u16,
            server_uuid: [7u8; 16],
            epoch: EPOCH,
            server_seq: 1,
            current_min: min,
            current_size: SIZE,
            current_state_hash: 1,
            decision: mtk_sync::SyncDecision::VerifiedSkipRebuild,
            message_or_dimension: "minecraft:overworld".to_string(),
        },
        Packet::PaletteUpdate {
            epoch: EPOCH,
            seq_id: 2,
            base_revision: 0,
            new_revision: 1,
            entries: vec![
                (0, "minecraft:air".to_string()),
                (1, "minecraft:stone".to_string()),
            ],
        },
        delta_op(EPOCH, 3, 1, IVec3::new(i32::MAX, 0, 0), 1),
    ]);
    wait_until_live(&mut session);
    wait_for_sequence(&mut session, 3);

    // The session must survive the poisoned entry and keep applying real deltas.
    peer.send(&[delta_op(EPOCH, 4, 1, IVec3::new(0, 0, 0), 1)]);
    wait_for_sequence(&mut session, 4);

    assert_eq!(
        session.current_sequence(),
        4,
        "session stopped processing after an out-of-range delta (status: {})",
        session.status()
    );
    let storage = session.get_storage();
    assert_eq!(
        storage.get_block(min.x, min.y, min.z),
        "minecraft:stone",
        "the following in-range delta must still be applied"
    );
}
