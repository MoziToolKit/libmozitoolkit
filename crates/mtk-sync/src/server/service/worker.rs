use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::Receiver;
use interprocess::local_socket::Stream as IpcStream;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::WebSocket;

use crate::protocol::{
    decode_packet, encode_packet, BaselineAckResult, ErrorCode, Packet, ResyncReason, SyncDecision,
};
use crate::server::decision::decide_handshake;
use crate::transport_ipc::{
    configure_accepted_ipc_stream, write_ipc_frame, IpcFrameReader, IpcReadError,
};

use super::config::{ServerCommand, ServerEvent};
use super::state::ServerInner;

pub(crate) enum PeerTransport {
    Ws {
        ws: WebSocket<TcpStream>,
    },
    Ipc {
        stream: IpcStream,
        reader: IpcFrameReader,
    },
}

enum PeerRead {
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    WouldBlock,
    Closed,
}

impl PeerTransport {
    fn from_tcp(stream: TcpStream) -> Option<Self> {
        let mut ws_config = WebSocketConfig::default();
        ws_config.max_message_size = Some(512 * 1024);
        ws_config.max_frame_size = Some(512 * 1024);

        let _ = stream.set_read_timeout(Some(Duration::from_millis(15)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(5000)));

        let ws = tungstenite::accept_with_config(stream, Some(ws_config)).ok()?;
        Some(Self::Ws { ws })
    }

    fn from_ipc(stream: IpcStream) -> Self {
        configure_accepted_ipc_stream(&stream);
        Self::Ipc {
            stream,
            reader: IpcFrameReader::new(),
        }
    }

    fn is_ipc(&self) -> bool {
        matches!(self, Self::Ipc { .. })
    }

    fn send_binary(&mut self, bytes: Vec<u8>) -> Result<(), ()> {
        match self {
            Self::Ws { ws } => {
                if ws
                    .write(tungstenite::Message::Binary(bytes.into()))
                    .is_err()
                    || ws.flush().is_err()
                {
                    Err(())
                } else {
                    Ok(())
                }
            }
            Self::Ipc { stream, .. } => write_ipc_frame(stream, &bytes).map_err(|_| ()),
        }
    }

    fn send_pong(&mut self, payload: Vec<u8>) -> Result<(), ()> {
        match self {
            Self::Ws { ws } => ws
                .write(tungstenite::Message::Pong(payload.into()))
                .map_err(|_| ()),
            Self::Ipc { .. } => Ok(()),
        }
    }

    fn send_heartbeat(&mut self) -> Result<(), ()> {
        match self {
            Self::Ws { ws } => {
                if ws
                    .write(tungstenite::Message::Ping(Vec::new().into()))
                    .is_err()
                    || ws.flush().is_err()
                {
                    Err(())
                } else {
                    Ok(())
                }
            }
            Self::Ipc { stream, .. } => {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let bytes = encode_packet(&Packet::Ping { timestamp: ts }).map_err(|_| ())?;
                write_ipc_frame(stream, &bytes).map_err(|_| ())
            }
        }
    }

    fn read_frame(&mut self) -> PeerRead {
        match self {
            Self::Ws { ws } => match ws.read() {
                Ok(tungstenite::Message::Binary(bytes)) => PeerRead::Binary(bytes.to_vec()),
                Ok(tungstenite::Message::Ping(payload)) => PeerRead::Ping(payload.to_vec()),
                Ok(tungstenite::Message::Pong(_)) => PeerRead::Pong,
                Ok(tungstenite::Message::Close(_)) => PeerRead::Closed,
                Ok(_) => PeerRead::WouldBlock,
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    PeerRead::WouldBlock
                }
                Err(_) => PeerRead::Closed,
            },
            Self::Ipc { stream, reader } => match reader.poll_frame(stream) {
                Ok(Some(bytes)) => PeerRead::Binary(bytes),
                Ok(None) => PeerRead::WouldBlock,
                Err(IpcReadError::Closed)
                | Err(IpcReadError::InvalidFrameSize(_))
                | Err(IpcReadError::Io(_)) => PeerRead::Closed,
            },
        }
    }

    fn close(self) {
        if let Self::Ws { mut ws } = self {
            let _ = ws.close(None);
        }
    }
}

pub(crate) fn run_ws_client_worker(
    client_id: u64,
    stream: TcpStream,
    addr: std::net::SocketAddr,
    cmd_rx: Receiver<ServerCommand>,
    inner: Arc<ServerInner>,
) {
    let Some(transport) = PeerTransport::from_tcp(stream) else {
        inner.remove_client(client_id);
        return;
    };
    run_client_worker(
        client_id,
        transport,
        addr.to_string(),
        addr.ip().is_loopback(),
        cmd_rx,
        inner,
    );
}

pub(crate) fn run_ipc_client_worker(
    client_id: u64,
    stream: IpcStream,
    endpoint: String,
    cmd_rx: Receiver<ServerCommand>,
    inner: Arc<ServerInner>,
) {
    let transport = PeerTransport::from_ipc(stream);
    run_client_worker(client_id, transport, endpoint, true, cmd_rx, inner);
}

fn run_client_worker(
    client_id: u64,
    mut transport: PeerTransport,
    endpoint: String,
    is_loopback: bool,
    cmd_rx: Receiver<ServerCommand>,
    inner: Arc<ServerInner>,
) {
    let is_ipc = transport.is_ipc();

    // Step 1: Handshake Negotiation (Must receive Packet::Hello within timeout)
    let start_time = Instant::now();
    let mut handshake_ok = false;

    while start_time.elapsed() < inner.handshake_timeout && inner.is_transport_running(is_ipc) {
        match transport.read_frame() {
            PeerRead::Binary(bytes) => match decode_packet(&bytes) {
                Ok(hello_pkt @ Packet::Hello { .. }) => {
                    let ctx = inner.get_handshake_context(is_ipc, is_loopback);
                    match decide_handshake(&hello_pkt, &ctx) {
                        Ok(res) => {
                            let Ok(welcome_bytes) = encode_packet(&res.welcome_packet) else {
                                transport.close();
                                inner.remove_client(client_id);
                                return;
                            };
                            if transport.send_binary(welcome_bytes).is_err() {
                                break;
                            }

                            if res.decision == SyncDecision::Rejected {
                                transport.close();
                                inner.remove_client(client_id);
                                return;
                            }

                            let _ = inner.event_tx.send(ServerEvent::ClientConnected {
                                client_id,
                                endpoint: endpoint.clone(),
                            });

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
                            if let Ok(err_bytes) = encode_packet(&Packet::Error {
                                epoch: inner.epoch.load(Ordering::Relaxed),
                                error_code: ErrorCode::UnknownOpcode,
                                message: "Malformed handshake".to_string(),
                            }) {
                                let _ = transport.send_binary(err_bytes);
                            }
                            transport.close();
                            inner.remove_client(client_id);
                            return;
                        }
                    }
                }
                _ => {
                    if let Ok(err_bytes) = encode_packet(&Packet::Error {
                        epoch: inner.epoch.load(Ordering::Relaxed),
                        error_code: ErrorCode::UnknownOpcode,
                        message: "Expected HELLO packet".to_string(),
                    }) {
                        let _ = transport.send_binary(err_bytes);
                    }
                    transport.close();
                    inner.remove_client(client_id);
                    return;
                }
            },
            PeerRead::Ping(payload) => {
                let _ = transport.send_pong(payload);
            }
            PeerRead::Pong => {}
            PeerRead::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            PeerRead::Closed => {
                inner.remove_client(client_id);
                return;
            }
        }
    }

    if !handshake_ok {
        transport.close();
        inner.remove_client(client_id);
        return;
    }

    // Step 2: Active Connection Processing Loop
    let mut last_ping = Instant::now();
    let mut last_activity = Instant::now();

    while inner.is_transport_running(is_ipc) {
        let mut did_work = false;

        // A. Drain outbound messages to client
        while let Ok(cmd) = cmd_rx.try_recv() {
            did_work = true;
            match cmd {
                ServerCommand::SendPacket(bytes) => {
                    if transport.send_binary(bytes).is_err() {
                        transport.close();
                        let _ = inner.event_tx.send(ServerEvent::ClientDisconnected {
                            client_id,
                            reason: "Write failed".to_string(),
                        });
                        inner.remove_client(client_id);
                        return;
                    }
                }
                ServerCommand::Close => {
                    transport.close();
                    inner.remove_client(client_id);
                    return;
                }
            }
        }

        // B. Drain incoming frames from client
        let mut close_connection = false;
        loop {
            match transport.read_frame() {
                PeerRead::Binary(bytes) => {
                    did_work = true;
                    last_activity = Instant::now();
                    if let Ok(pkt) = decode_packet(&bytes) {
                        match pkt {
                            Packet::Ping { timestamp } => {
                                let pong_pkt = Packet::Pong { timestamp };
                                match encode_packet(&pong_pkt) {
                                    Ok(pong_bytes) => {
                                        if transport.send_binary(pong_bytes).is_err() {
                                            close_connection = true;
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        let _ = inner.event_tx.send(ServerEvent::Warning(format!(
                                            "failed to encode PONG: {err}"
                                        )));
                                        close_connection = true;
                                        break;
                                    }
                                }
                            }
                            Packet::Pong { .. } => {}
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
                                close_connection = true;
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
                        if let Ok(err_bytes) = encode_packet(&err_pkt) {
                            let _ = transport.send_binary(err_bytes);
                        }
                        close_connection = true;
                        break;
                    }
                }
                PeerRead::Ping(payload) => {
                    did_work = true;
                    last_activity = Instant::now();
                    let _ = transport.send_pong(payload);
                }
                PeerRead::Pong => {
                    did_work = true;
                    last_activity = Instant::now();
                }
                PeerRead::WouldBlock => break,
                PeerRead::Closed => {
                    close_connection = true;
                    break;
                }
            }
        }

        if close_connection {
            break;
        }

        // C. Heartbeat Ping dispatch
        if last_ping.elapsed() >= inner.ping_interval {
            last_ping = Instant::now();
            if transport.send_heartbeat().is_err() {
                break;
            }
        }

        // D. Timeout defense (15 seconds without activity)
        if last_activity.elapsed() >= Duration::from_secs(15) {
            break;
        }

        if !did_work {
            thread::sleep(Duration::from_millis(if is_ipc { 2 } else { 5 }));
        }
    }

    transport.close();
    let _ = inner.event_tx.send(ServerEvent::ClientDisconnected {
        client_id,
        reason: if is_ipc {
            "IPC closed".to_string()
        } else {
            "Socket closed".to_string()
        },
    });
    inner.remove_client(client_id);
}
