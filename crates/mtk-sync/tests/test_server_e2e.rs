//! End-to-end integration tests for native LiveSyncServer and LiveSyncSession.

use std::time::{Duration, Instant};

use glam::IVec3;
use mtk_sync::server::{LiveSyncServer, ServerConfig, ServerEvent};
use mtk_sync::{LiveSyncSession, SyncEvent, SyncLifecycle};

#[test]
fn test_native_server_handshake_baseline_and_delta_e2e() {
    // 1. Start Server on ephemeral port
    let config = ServerConfig {
        bind_address: "127.0.0.1:0".to_string(),
        auth_token: None,
        allow_loopback_without_token: true,
        server_uuid: [42u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(3000),
        ping_interval: Duration::from_millis(1000),
    };

    let mut server = LiveSyncServer::start(config).expect("server must start");
    let local_addr = server.local_addr().expect("local addr must be known");
    let ws_url = format!("ws://{}", local_addr);

    // 2. Set authoritative selection
    let min_pos = IVec3::new(0, 64, 0);
    let size = IVec3::new(4, 4, 4);
    let total_voxels = (size.x * size.y * size.z) as usize;

    let state_to_name = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let biome_to_name = vec!["minecraft:plains".to_string()];
    let raw_states = vec![1u32; total_voxels];
    let raw_biomes = vec![0u32; 1];

    server.set_selection(min_pos, size, "minecraft:overworld".to_string());

    // 3. Connect Client
    let mut session = LiveSyncSession::new(None, None, None);
    session
        .start(&ws_url, false, 1)
        .expect("client session should start");

    // 4. Wait for ClientConnected and BaselineRequested on Server
    let mut client_id_opt = None;
    let deadline = Instant::now() + Duration::from_secs(3);

    while Instant::now() < deadline {
        let events = server.poll_events();
        for event in events {
            if let ServerEvent::BaselineRequested { client_id, .. } = event {
                client_id_opt = Some(client_id);
            }
        }
        if client_id_opt.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let client_id = client_id_opt.expect("server must emit BaselineRequested");

    // 5. Submit Baseline from Server
    server
        .submit_baseline(
            client_id,
            min_pos,
            size,
            "minecraft:overworld".to_string(),
            &raw_states,
            &state_to_name,
            &raw_biomes,
            &biome_to_name,
            &[],
            &[],
        )
        .expect("submit_baseline should succeed");

    // 6. Client processes baseline and transitions to Live
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut live_reached = false;

    while Instant::now() < deadline {
        let _ = session.poll_events();
        if session.lifecycle() == SyncLifecycle::Live {
            live_reached = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        live_reached,
        "client must transition to SyncLifecycle::Live"
    );

    // 7. Server broadcasts a DeltaOp using submit_deltas
    server
        .submit_deltas(&[IVec3::new(0, 0, 0)], &["minecraft:air".to_string()], &[])
        .expect("submit_deltas should succeed");

    // 8. Client receives delta and updates
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut delta_received = false;

    while Instant::now() < deadline {
        let events = session.poll_events();
        for event in events {
            if let SyncEvent::DeltaApplied { change_count, .. } = event {
                if change_count == 1 {
                    delta_received = true;
                    break;
                }
            }
        }
        if delta_received {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(delta_received, "client must receive and apply delta change");

    // 9. Stop server and verify client notices disconnection
    session.stop();
    server.stop();
    assert!(!server.is_running());
}

#[test]
fn test_native_server_auth_token_protection() {
    let config = ServerConfig {
        bind_address: "127.0.0.1:0".to_string(),
        auth_token: Some("secret123".to_string()),
        allow_loopback_without_token: false,
        server_uuid: [99u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(2000),
        ping_interval: Duration::from_millis(1000),
    };

    let mut server = LiveSyncServer::start(config).expect("server must start");
    let local_addr = server.local_addr().expect("local addr");
    let ws_url = format!("ws://{}", local_addr);

    // Client with wrong token
    let mut bad_session = LiveSyncSession::new(None, None, None);
    bad_session.set_auth_token("wrong_token");
    bad_session.start(&ws_url, false, 1).expect("start client");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut rejected = false;
    while Instant::now() < deadline {
        let events = bad_session.poll_events();
        for event in events {
            if let SyncEvent::Handshake { decision_code, .. } = event {
                if decision_code == 0xFF {
                    rejected = true;
                    break;
                }
            }
        }
        if rejected {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        rejected,
        "client with invalid token must be rejected with 0xFF"
    );

    bad_session.stop();
    server.stop();
}
