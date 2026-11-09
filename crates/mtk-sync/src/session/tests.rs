use super::*;
use mtk_core::MeshData;

#[test]
fn poll_events_coalesced_keeps_newest_world_mesh() {
    let session = LiveSyncSession::new(None, None, None);
    session
        .event_sender
        .send(SyncEvent::StatusChange("CONNECTED".to_string()))
        .unwrap();
    session
        .event_sender
        .send(SyncEvent::WorldMeshReady {
            mesh: MeshData::new(),
            generation: 1,
            profile: None,
        })
        .unwrap();
    session
        .event_sender
        .send(SyncEvent::WorldMeshReady {
            mesh: MeshData::new(),
            generation: 3,
            profile: None,
        })
        .unwrap();
    session
        .event_sender
        .send(SyncEvent::WorldMeshReady {
            mesh: MeshData::new(),
            generation: 2,
            profile: None,
        })
        .unwrap();

    let events = session.poll_events_coalesced();
    assert_eq!(events.len(), 2, "status + one coalesced mesh expected");
    assert!(matches!(events[0], SyncEvent::StatusChange(_)));
    match &events[1] {
        SyncEvent::WorldMeshReady { generation, .. } => assert_eq!(*generation, 3),
        other => panic!("expected newest world mesh, got {other:?}"),
    }
}

#[test]
fn auth_token_is_trimmed_like_the_server_config() {
    let mut session = LiveSyncSession::new(None, None, None);
    session.set_auth_token("  padded-secret \n");
    assert_eq!(session.auth_token(), "padded-secret");

    session.set_auth_token("   ");
    assert_eq!(session.auth_token(), "");
}
