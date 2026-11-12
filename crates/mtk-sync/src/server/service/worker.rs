use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use tungstenite::protocol::WebSocketConfig;

use crate::protocol::{
    decode_packet, encode_packet, BaselineAckResult, ErrorCode, Packet, ResyncReason, SyncDecision,
};
use crate::server::decision::decide_handshake;

use super::config::{ServerCommand, ServerConfig, ServerEvent};
use super::server::ServerInner;

pub(crate) fn run_client_worker(
    client_id: u64,
    stream: TcpStream,
    addr: SocketAddr,
    cmd_rx: Receiver<ServerCommand>,
    inner: Arc<ServerInner>,
    config: ServerConfig,
) {
    let mut ws_config = WebSocketConfig::default();
    ws_config.max_message_size = Some(512 * 1024);
    ws_config.max_frame_size = Some(512 * 1024);

    let _ = stream.set_read_timeout(Some(Duration::from_millis(15)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(5000)));

    let mut ws = match tungstenite::accept_with_config(stream, Some(ws_config)) {
        Ok(w) => w,
        Err(_) => {
            inner.remove_client(client_id);
            return;
        }
    };

    // Step 1: Handshake Negotiation (Must receive Packet::Hello within timeout)
    let start_time = Instant::now();
    let mut handshake_ok = false;

    while start_time.elapsed() < config.handshake_timeout && inner.is_running() {
        match ws.read() {
            Ok(tungstenite::Message::Binary(bytes)) => match decode_packet(&bytes) {
                Ok(hello_pkt @ Packet::Hello { .. }) => {
                    let ctx = inner.get_handshake_context(&config, addr);
                    match decide_handshake(&hello_pkt, &ctx) {
                        Ok(res) => {
                            let welcome_bytes = encode_packet(&res.welcome_packet);
                            if ws
                                .write(tungstenite::Message::Binary(welcome_bytes.into()))
                                .is_err()
                                || ws.flush().is_err()
                            {
                                break;
                            }

                            if res.decision == SyncDecision::Rejected {
                                let _ = ws.close(None);
                                inner.remove_client(client_id);
                                return;
                            }

                            let _ = inner
                                .event_tx
                                .send(ServerEvent::ClientConnected { client_id, addr });

                            if res.decision == SyncDecision::FullBaselineRequired {
                                let _ = inner.event_tx.send(ServerEvent::BaselineRequested {
                                    client_id,
                                    epoch: ctx.epoch,
                                    snapshot_seq: ctx.sequence,
                                });
                            }

                            handshake_ok = true;
                            break;
                        }
                        Err(_) => {
                            let err_bytes = encode_packet(&Packet::Error {
                                epoch: inner.epoch.load(Ordering::Relaxed),
                                error_code: ErrorCode::UnknownOpcode,
                                message: "Malformed handshake".to_string(),
                            });
                            let _ = ws.write(tungstenite::Message::Binary(err_bytes.into()));
                            let _ = ws.flush();
                            let _ = ws.close(None);
                            inner.remove_client(client_id);
                            return;
                        }
                    }
                }
                _ => {
                    let err_bytes = encode_packet(&Packet::Error {
                        epoch: inner.epoch.load(Ordering::Relaxed),
                        error_code: ErrorCode::UnknownOpcode,
                        message: "Expected HELLO packet".to_string(),
                    });
                    let _ = ws.write(tungstenite::Message::Binary(err_bytes.into()));
                    let _ = ws.flush();
                    let _ = ws.close(None);
                    inner.remove_client(client_id);
                    return;
                }
            },
            Ok(tungstenite::Message::Ping(payload)) => {
                let _ = ws.write(tungstenite::Message::Pong(payload));
            }
            Ok(tungstenite::Message::Close(_)) => {
                inner.remove_client(client_id);
                return;
            }
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                inner.remove_client(client_id);
                return;
            }
            _ => {}
        }
    }

    if !handshake_ok {
        let _ = ws.close(None);
        inner.remove_client(client_id);
        return;
    }

    // Step 2: Active Connection Processing Loop
    let mut last_ping = Instant::now();
    let mut last_activity = Instant::now();

    while inner.is_running() {
        // A. Drain outbound messages to client
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                ServerCommand::SendPacket(bytes) => {
                    if ws
                        .write(tungstenite::Message::Binary(bytes.into()))
                        .is_err()
                        || ws.flush().is_err()
                    {
                        break;
                    }
                }
                ServerCommand::Close => {
                    let _ = ws.close(None);
                    inner.remove_client(client_id);
                    return;
                }
            }
        }

        // B. Read incoming frames from client
        match ws.read() {
            Ok(tungstenite::Message::Binary(bytes)) => {
                last_activity = Instant::now();
                if let Ok(pkt) = decode_packet(&bytes) {
                    match pkt {
                        Packet::Ping { timestamp } => {
                            let pong_pkt = Packet::Pong { timestamp };
                            let pong_bytes = encode_packet(&pong_pkt);
                            let _ = ws.write(tungstenite::Message::Binary(pong_bytes.into()));
                            let _ = ws.flush();
                        }
                        Packet::BaselineAck { result_code, .. } => {
                            if result_code != BaselineAckResult::SuccessCommitted {
                                let _ = inner.event_tx.send(ServerEvent::ClientResync {
                                    client_id,
                                    reason: ResyncReason::StateHashMismatch,
                                });
                            }
                        }
                        Packet::ResyncRequest { reason, .. } => {
                            let _ = inner
                                .event_tx
                                .send(ServerEvent::ClientResync { client_id, reason });
                        }
                        Packet::Cancel { .. } => {
                            break;
                        }
                        _ => {}
                    }
                } else {
                    let err_pkt = Packet::Error {
                        epoch: inner.epoch.load(Ordering::Relaxed),
                        error_code: ErrorCode::UnknownOpcode,
                        message: "Malformed packet".to_string(),
                    };
                    let err_bytes = encode_packet(&err_pkt);
                    let _ = ws.write(tungstenite::Message::Binary(err_bytes.into()));
                    let _ = ws.flush();
                    break;
                }
            }
            Ok(tungstenite::Message::Ping(payload)) => {
                last_activity = Instant::now();
                let _ = ws.write(tungstenite::Message::Pong(payload));
            }
            Ok(tungstenite::Message::Pong(_)) => {
                last_activity = Instant::now();
            }
            Ok(tungstenite::Message::Close(_)) => break,
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout slice, continue
            }
            Err(_) => break,
            _ => {}
        }

        // C. Heartbeat Ping dispatch
        if last_ping.elapsed() >= config.ping_interval {
            last_ping = Instant::now();
            if ws
                .write(tungstenite::Message::Ping(Vec::new().into()))
                .is_err()
                || ws.flush().is_err()
            {
                break;
            }
        }

        // D. Timeout defense (15 seconds without activity)
        if last_activity.elapsed() >= Duration::from_secs(15) {
            break;
        }

        thread::sleep(Duration::from_millis(5));
    }

    let _ = inner.event_tx.send(ServerEvent::ClientDisconnected {
        client_id,
        reason: "Socket closed".to_string(),
    });
    inner.remove_client(client_id);
}
