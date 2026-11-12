//! Regression tests for RFC v3.1 HELLO authentication (RFC §4.1).
//!
//! The Yefira server rejects any HELLO whose `auth_token` does not match its
//! configured secret. Blender never uses the cached fast path (native
//! `VoxelStorage` is not persisted into `.blend`), so the *default* HELLO must
//! carry the token; otherwise an identical token on both sides still fails with
//! `REJECTED("Authentication failed")`.

use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use mtk_sync::{decode_packet, LiveSyncSession, Packet};
use mtk_voxel::MesherConfig;
use tungstenite::Message;

/// Serves exactly one WebSocket connection and returns the first HELLO it receives.
fn serve_one_hello(listener: TcpListener) -> Packet {
    let (stream, _) = listener.accept().expect("client must connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set server read timeout");
    let mut socket = tungstenite::accept(stream).expect("websocket handshake");

    loop {
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                if let Ok(Packet::Hello { .. }) = decode_packet(&bytes) {
                    return decode_packet(&bytes).expect("decode HELLO");
                }
            }
            Ok(Message::Close(_)) => panic!("client closed before sending HELLO"),
            Ok(_) => {}
            Err(e) => panic!("server failed to read HELLO: {e}"),
        }
    }
}

fn bind_listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    (listener, format!("ws://127.0.0.1:{port}"))
}

fn assert_hello_token(packet: Packet, expected: &str) {
    match packet {
        Packet::Hello { auth_token, .. } => assert_eq!(auth_token, expected),
        other => panic!("expected HELLO, got {other:?}"),
    }
}

#[test]
fn default_hello_sends_configured_auth_token() {
    let (listener, url) = bind_listener();
    let server = thread::spawn(move || serve_one_hello(listener));

    let mut session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);
    session.set_auth_token("  s3cret-token  ");
    session
        .start_with_hello(&url, false, 1, None)
        .expect("session must start");

    let hello = server.join().expect("server thread");
    assert_hello_token(hello, "s3cret-token");
    session.stop();
}

#[test]
fn cached_hello_keeps_cache_and_receives_auth_token() {
    let (listener, url) = bind_listener();
    let server = thread::spawn(move || serve_one_hello(listener));

    let cached_hello = Packet::Hello {
        min_version: 3,
        max_version: 3,
        capabilities: 0x03,
        flags: 0,
        cached_world_uuid: [7u8; 16],
        cached_epoch: 11,
        cached_seq: 22,
        cached_min: glam::IVec3::new(-1, 0, 1),
        cached_size: glam::IVec3::new(16, 16, 16),
        cached_state_hash: 0xDEAD_BEEF,
        auth_token: String::new(),
        cached_dimension: "minecraft:the_nether".to_string(),
        subscribed_visual_entities: Vec::new(),
    };

    let mut session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);
    session.set_auth_token("cached-secret");
    session
        .start_with_hello(&url, false, 1, Some(cached_hello))
        .expect("session must start");

    match server.join().expect("server thread") {
        Packet::Hello {
            auth_token,
            cached_state_hash,
            cached_epoch,
            cached_dimension,
            ..
        } => {
            assert_eq!(auth_token, "cached-secret");
            assert_eq!(cached_state_hash, 0xDEAD_BEEF);
            assert_eq!(cached_epoch, 11);
            assert_eq!(cached_dimension, "minecraft:the_nether");
        }
        other => panic!("expected HELLO, got {other:?}"),
    }
    session.stop();
}
