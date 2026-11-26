//! End-to-end integration tests for native LiveSyncServer and LiveSyncSession.

use std::time::{Duration, Instant};

use glam::IVec3;
use mtk_sync::server::{LiveSyncServer, ServerConfig, ServerEvent};
use mtk_sync::{
    discover_local_instances, InstanceMetadata, LiveSyncSession, SyncEvent, SyncLifecycle,
};

#[test]
fn test_native_server_handshake_baseline_and_delta_e2e() {
    // 1. Start Server on ephemeral port
    let config = ServerConfig {
        bind_address: Some("127.0.0.1:0".to_string()),
        enable_ipc: false,
        ipc_name: None,
        instance_info: None,
        auth_token: None,
        allow_loopback_without_token: true,
        require_token_on_ipc: false,
        server_uuid: [42u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(3000),
        ping_interval: Duration::from_millis(1000),
    };

    let server = LiveSyncServer::start(config).expect("server must start");
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
        bind_address: Some("127.0.0.1:0".to_string()),
        enable_ipc: false,
        ipc_name: None,
        instance_info: None,
        auth_token: Some("secret123".to_string()),
        allow_loopback_without_token: false,
        require_token_on_ipc: false,
        server_uuid: [99u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(2000),
        ping_interval: Duration::from_millis(1000),
    };

    let server = LiveSyncServer::start(config).expect("server must start");
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

#[test]
fn test_ipc_only_server_discovery_baseline_delta_and_optional_token() {
    let config = ServerConfig {
        bind_address: None,
        enable_ipc: true,
        ipc_name: None,
        instance_info: Some(InstanceMetadata {
            world_name: "E2E IPC World".to_string(),
            mc_version: "1.21.4".to_string(),
            loader: "Fabric".to_string(),
            dimension: "minecraft:overworld".to_string(),
        }),
        auth_token: Some("ws_only_secret".to_string()),
        allow_loopback_without_token: false,
        require_token_on_ipc: false,
        server_uuid: [77u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(3000),
        ping_interval: Duration::from_millis(500),
    };

    let server = LiveSyncServer::start(config).expect("IPC-only server must start");
    assert!(
        server.local_addr().is_none(),
        "IPC-only server must not bind any TCP port"
    );
    let ipc_url = server.ipc_url().expect("ipc_url must be populated");

    // Discovery finds our running instance and reflects metadata updates.
    let instances = discover_local_instances();
    let mine = instances
        .iter()
        .find(|inst| inst.ipc_url == ipc_url)
        .expect("discover_local_instances must include running IPC server");
    assert_eq!(mine.world_name, "E2E IPC World");
    assert!(!mine.requires_token);

    server.update_instance_metadata(InstanceMetadata {
        world_name: "E2E IPC World".to_string(),
        mc_version: "1.21.4".to_string(),
        loader: "Fabric".to_string(),
        dimension: "minecraft:the_end".to_string(),
    });
    let instances_after = discover_local_instances();
    let mine_after = instances_after
        .iter()
        .find(|inst| inst.ipc_url == ipc_url)
        .expect("updated instance must still be discovered");
    assert_eq!(mine_after.dimension, "minecraft:the_end");

    // Connect without auth token (allowed because require_token_on_ipc == false).
    let min_pos = IVec3::new(0, 64, 0);
    let size = IVec3::new(4, 4, 4);
    let total_voxels = (size.x * size.y * size.z) as usize;
    let state_to_name = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let biome_to_name = vec!["minecraft:plains".to_string()];
    let raw_states = vec![1u32; total_voxels];
    let raw_biomes = vec![0u32; 1];

    server.set_selection(min_pos, size, "minecraft:the_end".to_string());

    let mut session = LiveSyncSession::new(None, None, None);
    session
        .start(&ipc_url, false, 1)
        .expect("IPC client session should start");

    let mut client_id_opt = None;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        for event in server.poll_events() {
            if let ServerEvent::BaselineRequested { client_id, .. } = event {
                client_id_opt = Some(client_id);
            }
        }
        if client_id_opt.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let client_id = client_id_opt.expect("IPC server must emit BaselineRequested");

    server
        .submit_baseline(
            client_id,
            min_pos,
            size,
            "minecraft:the_end".to_string(),
            &raw_states,
            &state_to_name,
            &raw_biomes,
            &biome_to_name,
            &[],
        )
        .expect("IPC baseline submit must succeed");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let _ = session.poll_events();
        if session.lifecycle() == SyncLifecycle::Live {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(session.lifecycle(), SyncLifecycle::Live);

    server
        .submit_deltas(&[IVec3::new(0, 0, 0)], &["minecraft:air".to_string()], &[])
        .expect("IPC delta submit must succeed");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut delta_applied = false;
    while Instant::now() < deadline {
        for event in session.poll_events() {
            if let SyncEvent::DeltaApplied { change_count, .. } = event {
                if change_count == 1 {
                    delta_applied = true;
                }
            }
        }
        if delta_applied {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(delta_applied, "IPC client must apply delta update");

    session.stop();
    server.stop();
    assert!(
        !discover_local_instances()
            .iter()
            .any(|inst| inst.ipc_url == ipc_url),
        "stopped IPC server must be removed from discovery"
    );
}

#[test]
fn test_ipc_require_token_and_dual_stack_broadcast() {
    let config = ServerConfig {
        bind_address: Some("127.0.0.1:0".to_string()),
        enable_ipc: true,
        ipc_name: None,
        instance_info: None,
        auth_token: Some("ipc-secret".to_string()),
        allow_loopback_without_token: false,
        require_token_on_ipc: true,
        server_uuid: [88u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(2000),
        ping_interval: Duration::from_millis(1000),
    };

    let server = LiveSyncServer::start(config).expect("dual-stack server must start");
    let ws_url = format!("ws://{}", server.local_addr().expect("ws addr"));
    let ipc_url = server.ipc_url().expect("ipc url");

    // 1. IPC client without token must be rejected when require_token_on_ipc=true.
    let mut bad_ipc = LiveSyncSession::new(None, None, None);
    bad_ipc.start(&ipc_url, false, 1).expect("start bad ipc");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut rejected = false;
    while Instant::now() < deadline {
        for ev in bad_ipc.poll_events() {
            if let SyncEvent::Handshake {
                decision_code: 0xFF,
                ..
            } = ev
            {
                rejected = true;
            }
        }
        if rejected {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(
        rejected,
        "IPC client missing token must be rejected when require_token_on_ipc=true"
    );
    bad_ipc.stop();

    // 2. Concurrent IPC + WS clients with valid token both connect.
    let mut good_ipc = LiveSyncSession::new(None, None, None);
    good_ipc.set_auth_token("ipc-secret");
    good_ipc.start(&ipc_url, false, 1).expect("start good ipc");

    let mut good_ws = LiveSyncSession::new(None, None, None);
    good_ws.set_auth_token("ipc-secret");
    good_ws.start(&ws_url, false, 1).expect("start good ws");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if server.client_count() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(
        server.client_count(),
        2,
        "both IPC and WS clients must be connected concurrently"
    );

    good_ipc.stop();
    good_ws.stop();
    server.stop();
}

#[test]
fn test_decoupled_ws_lifecycle_preserves_ipc_client() {
    let config = ServerConfig {
        bind_address: None,
        enable_ipc: true,
        ipc_name: None,
        instance_info: Some(InstanceMetadata {
            world_name: "DecoupledWorld".to_string(),
            mc_version: "1.21.4".to_string(),
            loader: "Fabric".to_string(),
            dimension: "minecraft:overworld".to_string(),
        }),
        auth_token: None,
        allow_loopback_without_token: true,
        require_token_on_ipc: false,
        server_uuid: [91u8; 16],
        max_connections: 4,
        handshake_timeout: Duration::from_millis(2000),
        ping_interval: Duration::from_millis(1000),
    };

    let server = LiveSyncServer::start(config).expect("IPC-only server must start");
    assert!(server.is_ipc_running());
    assert!(!server.is_ws_running());
    let ipc_url = server.ipc_url().expect("ipc url");

    // 1. Connect IPC client before WS is ever started
    let mut ipc_client = LiveSyncSession::new(None, None, None);
    ipc_client.start(&ipc_url, false, 1).expect("start ipc");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let _ = ipc_client.poll_events();
        if server.ipc_client_count() == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(server.ipc_client_count(), 1);
    assert_eq!(server.ws_client_count(), 0);

    // 2. Dynamically attach WS listener and update auth without disturbing IPC
    server.update_auth(Some("dynamic-secret".to_string()), false, false);
    server
        .start_ws("127.0.0.1:0")
        .expect("start_ws must succeed");
    assert!(server.is_ws_running());
    let ws_addr = server.local_addr().expect("ws addr must exist");
    let ws_url = format!("ws://{}", ws_addr);

    assert!(
        discover_local_instances()
            .iter()
            .any(|inst| inst.ipc_url == ipc_url
                && inst.ws_bind_address.as_deref() == Some(&ws_addr.to_string())
                && !inst.requires_token),
        "discovery must reflect dynamic ws_bind_address and IPC token flag"
    );

    // Connect WS client using updated dynamic token
    let mut ws_client = LiveSyncSession::new(None, None, None);
    ws_client.set_auth_token("dynamic-secret");
    ws_client.start(&ws_url, false, 1).expect("start ws");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let _ = ws_client.poll_events();
        if server.ws_client_count() == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(server.ipc_client_count(), 1);
    assert_eq!(server.ws_client_count(), 1);

    // 3. Stop WS only — IPC client must remain connected!
    server.stop_ws();
    assert!(!server.is_ws_running());
    assert!(server.is_ipc_running());
    assert!(server.local_addr().is_none());
    assert_eq!(server.ws_client_count(), 0);
    assert_eq!(
        server.ipc_client_count(),
        1,
        "IPC client must stay connected when WS stops"
    );
    assert!(
        discover_local_instances()
            .iter()
            .any(|inst| inst.ipc_url == ipc_url && inst.ws_bind_address.is_none()),
        "discovery must clear ws_bind_address when WS stops"
    );

    ws_client.stop();
    ipc_client.stop();
    server.stop();
}

fn parse_uuid_hex_16(hex: &str) -> [u8; 16] {
    assert_eq!(hex.len(), 32, "expected 32-char uuid hex, got '{hex}'");
    let mut out = [0u8; 16];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("valid hex byte");
    }
    out
}

fn run_fast_reconnect_verified_skip_rebuild_case(url: &str, server: &LiveSyncServer) {
    let min_pos = IVec3::new(0, 64, 0);
    let size = IVec3::new(4, 4, 4);
    let total_voxels = (size.x * size.y * size.z) as usize;

    // Deliberately use a non-canonical palette order where "minecraft:air" is NOT index 0
    // to prove server palette state initializes from the compacted stream palette.
    let state_to_name = vec![
        "minecraft:dirt".to_string(),
        "minecraft:stone".to_string(),
        "minecraft:air".to_string(),
    ];
    let biome_to_name = vec!["minecraft:plains".to_string()];
    let mut raw_states = vec![2u32; total_voxels];
    raw_states[0] = 0; // (0, 0, 0) -> minecraft:dirt
    raw_states[1] = 1; // (1, 0, 0) -> minecraft:stone
    let raw_biomes = vec![0u32; 1];

    server.set_selection(min_pos, size, "minecraft:overworld".to_string());

    // 1. Initial client connects and receives full baseline.
    let mut client1 = LiveSyncSession::new(None, None, None);
    client1.start(url, false, 1).expect("client1 start");

    let mut client1_id = None;
    let mut server_events_log = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        for ev in server.poll_events() {
            server_events_log.push(ev.clone());
            if let ServerEvent::BaselineRequested { client_id, .. } = ev {
                client1_id = Some(client_id);
            }
        }
        if client1_id.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let client1_id = client1_id.expect("server must request baseline for client1");

    let submit_res = server.submit_baseline(
        client1_id,
        min_pos,
        size,
        "minecraft:overworld".to_string(),
        &raw_states,
        &state_to_name,
        &raw_biomes,
        &biome_to_name,
        &[],
    );
    if let Err(err) = submit_res {
        let extra_server_events = server.poll_events();
        let c1_events = client1.poll_events();
        panic!(
            "submit_baseline failed for url={url} client1_id={client1_id}: {err:?}; server_events={server_events_log:?}; extra_server_events={extra_server_events:?}; client1_events={c1_events:?}"
        );
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut client1_events = Vec::new();
    while Instant::now() < deadline {
        client1_events.extend(client1.poll_events());
        if client1.lifecycle() == SyncLifecycle::Live {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(
        client1.lifecycle(),
        SyncLifecycle::Live,
        "client1 did not reach Live for url={url}; events={client1_events:?}; server_events={:?}",
        server.poll_events()
    );

    let uuid_bytes = parse_uuid_hex_16(&client1.server_uuid_hex());
    let epoch = client1.current_epoch();
    let seq = client1.current_sequence();
    let state_hash = client1.current_state_hash();
    assert_ne!(state_hash, 0, "baseline state_hash must be non-zero");
    assert!(seq > 0, "baseline sequence must be non-zero");

    client1.stop();
    // Drain server events from client1 before reconnecting.
    std::thread::sleep(Duration::from_millis(30));
    let _ = server.poll_events();

    // 2. Second client reconnects with cached state -> must hit 0ms VERIFIED_SKIP_REBUILD.
    let mut client2 = LiveSyncSession::new(None, None, None);
    client2
        .start_with_cache(
            url,
            false,
            1,
            uuid_bytes,
            epoch,
            seq,
            min_pos,
            size,
            state_hash,
            "minecraft:overworld".to_string(),
        )
        .expect("client2 start_with_cache");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut handshake_code = None;
    let mut saw_verified = false;
    while Instant::now() < deadline {
        for ev in client2.poll_events() {
            match ev {
                SyncEvent::Handshake { decision_code, .. } => handshake_code = Some(decision_code),
                SyncEvent::Verified { is_verified, .. } => saw_verified = is_verified,
                _ => {}
            }
        }
        if handshake_code.is_some() && saw_verified && client2.lifecycle() == SyncLifecycle::Live {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }

    assert_eq!(
        handshake_code,
        Some(0x00),
        "fast reconnect must negotiate VERIFIED_SKIP_REBUILD (0x00)"
    );
    assert!(saw_verified, "client2 must emit SyncEvent::Verified(true)");
    assert_eq!(client2.lifecycle(), SyncLifecycle::Live);

    for ev in server.poll_events() {
        assert!(
            !matches!(ev, ServerEvent::BaselineRequested { .. }),
            "server must not emit BaselineRequested on VerifiedSkipRebuild, got {ev:?}"
        );
    }

    // 3. Submit a delta referencing an existing baseline block ("minecraft:stone").
    //    If VerifiedSkipRebuild didn't restore palette_map or server palette IDs drifted,
    //    this would fall back to minecraft:air or dirt instead of minecraft:stone.
    server
        .submit_deltas(
            &[IVec3::new(0, 0, 0)],
            &["minecraft:stone".to_string()],
            &[],
        )
        .expect("submit_deltas must succeed");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut delta_applied = false;
    while Instant::now() < deadline {
        for ev in client2.poll_events() {
            if let SyncEvent::DeltaApplied { change_count, .. } = ev {
                if change_count == 1 {
                    delta_applied = true;
                }
            }
        }
        if delta_applied {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(
        delta_applied,
        "client2 must apply delta after VerifiedSkipRebuild"
    );
    assert_eq!(
        client2
            .get_storage()
            .get_block(min_pos.x, min_pos.y, min_pos.z),
        "minecraft:stone",
        "fast-reconnected client must resolve existing palette entries in subsequent deltas"
    );
    client2.stop();

    // 4. After delta mutation, stale cached state must fall back to FullBaselineRequired (0x02).
    let mut client3 = LiveSyncSession::new(None, None, None);
    client3
        .start_with_cache(
            url,
            false,
            1,
            uuid_bytes,
            epoch,
            seq,
            min_pos,
            size,
            state_hash,
            "minecraft:overworld".to_string(),
        )
        .expect("client3 start_with_cache");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut client3_decision = None;
    while Instant::now() < deadline {
        for ev in client3.poll_events() {
            if let SyncEvent::Handshake { decision_code, .. } = ev {
                client3_decision = Some(decision_code);
            }
        }
        if client3_decision.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(
        client3_decision,
        Some(0x02),
        "stale cached state after delta must require FullBaselineRequired (0x02)"
    );
    client3.stop();

    // Wait for client3's server worker to unregister and drain its leftover events
    // before reusing the same server instance for the next transport case.
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.client_count() > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = server.poll_events();
}

#[test]
fn test_fast_reconnect_verified_skip_rebuild_and_subsequent_delta_ws_and_ipc() {
    let config = ServerConfig {
        bind_address: Some("127.0.0.1:0".to_string()),
        enable_ipc: true,
        ipc_name: None,
        instance_info: None,
        auth_token: None,
        allow_loopback_without_token: true,
        require_token_on_ipc: false,
        server_uuid: [0x55; 16],
        max_connections: 8,
        handshake_timeout: Duration::from_millis(3000),
        ping_interval: Duration::from_millis(1000),
    };

    let server = LiveSyncServer::start(config).expect("dual-stack server must start");
    let ws_url = format!("ws://{}", server.local_addr().expect("ws addr"));
    let ipc_url = server.ipc_url().expect("ipc url");

    run_fast_reconnect_verified_skip_rebuild_case(&ws_url, &server);
    run_fast_reconnect_verified_skip_rebuild_case(&ipc_url, &server);

    server.stop();
}
