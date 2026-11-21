//! # Native WebSocket Live Sync Client
//!
//! Dedicated background thread client using `tungstenite` with auto-reconnect and packet routing.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use tungstenite::protocol::Message;
use url::Url;

use crate::protocol::{decode_packet, encode_packet, Packet, ProtocolError};

/// Encodes `packet` and queues it for the transport thread.
///
/// The encoding error is returned so callers can surface it (an oversized identifier or a
/// malformed UUID is host data, not a transport failure). The channel send itself stays
/// best-effort because the transport thread may already be gone.
pub(crate) fn queue_packet(
    sender: &Sender<ClientCommand>,
    packet: &Packet,
) -> Result<(), ProtocolError> {
    let bytes = encode_packet(packet)?;
    let _ = sender.send(ClientCommand::Send(bytes));
    Ok(())
}

/// Commands sent from the session controller to the network client thread.
#[derive(Debug)]
pub(crate) enum ClientCommand {
    /// Send raw binary packet to server.
    Send(Vec<u8>),
    /// Terminate connection and worker thread.
    Stop,
}

/// Network client status messages.
#[derive(Debug, Clone)]
pub(crate) enum ClientMessage {
    Status(String),
    /// Non-fatal transport diagnostics (e.g. a single undecodable frame).
    /// Must NOT be interpreted as a connection status change.
    Warning(String),
    /// The active socket was lost; the client thread is reconnecting.
    ConnectionLost,
    PacketReceived {
        packet: Packet,
        bytes: usize,
    },
    /// A background player skin texture has finished downloading.
    SkinDownloaded(String),
    Disconnected,
}

/// Manages the background WebSocket transport thread.
pub(crate) struct SyncClient {
    running: Arc<AtomicBool>,
    cmd_sender: Sender<ClientCommand>,
    active_tcp: Arc<Mutex<Option<TcpStream>>>,
    thread_handle: Option<JoinHandle<()>>,
}

impl SyncClient {
    /// Spawns a new background WebSocket client connecting to `url`.
    pub(crate) fn connect(
        url_str: String,
        msg_sender: Sender<ClientMessage>,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
    ) -> Result<Self, String> {
        let _parsed = Url::parse(&url_str)
            .map_err(|e| format!("Invalid WebSocket URL '{}': {}", url_str, e))?;

        let running = Arc::new(AtomicBool::new(true));
        let active_tcp = Arc::new(Mutex::new(None));
        let (cmd_sender, cmd_receiver) = crossbeam_channel::unbounded::<ClientCommand>();

        let running_clone = running.clone();
        let active_tcp_clone = active_tcp.clone();
        let handle = thread::spawn(move || {
            Self::run_loop(
                url_str,
                msg_sender,
                cmd_receiver,
                running_clone,
                active_tcp_clone,
                auto_reconnect,
                max_reconnect_attempts,
            );
        });

        Ok(Self {
            running,
            cmd_sender,
            active_tcp,
            thread_handle: Some(handle),
        })
    }

    /// Returns a cloned command sender to allow worker threads to send commands to the server.
    pub(crate) fn get_cmd_sender(&self) -> Sender<ClientCommand> {
        self.cmd_sender.clone()
    }

    /// Stops the client thread cleanly and immediately shuts down the underlying TCP connection.
    pub(crate) fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_sender.send(ClientCommand::Stop);
        if let Ok(mut guard) = self.active_tcp.lock() {
            if let Some(stream) = guard.take() {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    fn drain_stale_commands(cmd_receiver: &Receiver<ClientCommand>) -> bool {
        while let Ok(cmd) = cmd_receiver.try_recv() {
            if matches!(cmd, ClientCommand::Stop) {
                return true;
            }
        }
        false
    }

    fn run_loop(
        url_str: String,
        msg_sender: Sender<ClientMessage>,
        cmd_receiver: Receiver<ClientCommand>,
        running: Arc<AtomicBool>,
        active_tcp: Arc<Mutex<Option<TcpStream>>>,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
    ) {
        let mut attempts = 0;

        while running.load(Ordering::Relaxed) {
            if attempts > 0 {
                let _ = msg_sender.send(ClientMessage::Status(format!(
                    "RECONNECTING... ({}/{})",
                    attempts, max_reconnect_attempts
                )));
            } else {
                let _ = msg_sender.send(ClientMessage::Status("CONNECTING...".to_string()));
            }

            match tungstenite::connect(&url_str) {
                Ok((mut socket, _response)) => {
                    if Self::drain_stale_commands(&cmd_receiver) {
                        let _ = socket.close(None);
                        let _ = msg_sender.send(ClientMessage::Disconnected);
                        return;
                    }
                    attempts = 0;
                    let _ = msg_sender.send(ClientMessage::Status("CONNECTED".to_string()));

                    // Set underlying TCP stream read timeout, enable nodelay, and store cloned handle for instant shutdown
                    if let tungstenite::stream::MaybeTlsStream::Plain(ref s) = socket.get_ref() {
                        let _ = s.set_read_timeout(Some(Duration::from_millis(100)));
                        let _ = s.set_nodelay(true);
                        if let Ok(cloned) = s.try_clone() {
                            if let Ok(mut guard) = active_tcp.lock() {
                                *guard = Some(cloned);
                            }
                        }
                    }

                    while running.load(Ordering::Relaxed) {
                        let mut connection_lost = false;

                        // Check for outgoing commands
                        while let Ok(cmd) = cmd_receiver.try_recv() {
                            match cmd {
                                ClientCommand::Send(data) => {
                                    if let Err(e) = socket.send(Message::Binary(data.into())) {
                                        let _ = msg_sender.send(ClientMessage::Status(format!(
                                            "Send error: {}",
                                            e
                                        )));
                                        connection_lost = true;
                                        break;
                                    }
                                }
                                ClientCommand::Stop => {
                                    let _ = socket.close(None);
                                    if let Ok(mut guard) = active_tcp.lock() {
                                        if let Some(s) = guard.take() {
                                            let _ = s.shutdown(std::net::Shutdown::Both);
                                        }
                                    }
                                    let _ = msg_sender.send(ClientMessage::Disconnected);
                                    return;
                                }
                            }
                        }

                        if connection_lost {
                            break;
                        }

                        // Read incoming messages
                        match socket.read() {
                            Ok(Message::Binary(bin)) => {
                                let bytes = bin.len();
                                match decode_packet(&bin) {
                                    Ok(packet) => {
                                        let _ = msg_sender
                                            .send(ClientMessage::PacketReceived { packet, bytes });
                                    }
                                    Err(e) => {
                                        // A corrupt frame must never tear down the
                                        // session or flip the UI to disconnected.
                                        let _ = msg_sender.send(ClientMessage::Warning(format!(
                                            "Packet decode error: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Ok(Message::Ping(payload)) => {
                                let _ = socket.send(Message::Pong(payload));
                            }
                            Ok(Message::Close(_)) => {
                                let _ = msg_sender.send(ClientMessage::Status(
                                    "Server closed the connection".to_string(),
                                ));
                                break;
                            }
                            Ok(_) => {}
                            Err(tungstenite::Error::Io(ref e))
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    || e.kind() == std::io::ErrorKind::TimedOut
                                    || e.kind() == std::io::ErrorKind::Interrupted =>
                            {
                                // Normal timeout: continue poll loop
                                continue;
                            }
                            Err(e) => {
                                if !running.load(Ordering::Relaxed) {
                                    break;
                                }
                                let _ = msg_sender.send(ClientMessage::Status(format!(
                                    "Connection error: {}",
                                    e
                                )));
                                break;
                            }
                        }
                    }

                    // Clear active TCP reference on disconnect/loop exit
                    if let Ok(mut guard) = active_tcp.lock() {
                        *guard = None;
                    }

                    if Self::drain_stale_commands(&cmd_receiver) {
                        let _ = msg_sender.send(ClientMessage::Disconnected);
                        return;
                    }

                    if running.load(Ordering::Relaxed) && auto_reconnect {
                        let _ = msg_sender.send(ClientMessage::ConnectionLost);
                    }
                }
                Err(e) => {
                    attempts += 1;
                    if !auto_reconnect || attempts > max_reconnect_attempts {
                        let _ = msg_sender.send(ClientMessage::Status(format!(
                            "DISCONNECTED (Connection failed: {})",
                            e
                        )));
                        break;
                    }
                    let _ = msg_sender.send(ClientMessage::Status(format!(
                        "Connection failed: {}. Retrying ({}/{})...",
                        e, attempts, max_reconnect_attempts
                    )));
                }
            }

            if running.load(Ordering::Relaxed)
                && auto_reconnect
                && attempts <= max_reconnect_attempts
            {
                // Non-blocking backoff: wake up immediately if Stop command received
                match cmd_receiver.recv_timeout(Duration::from_millis(1500)) {
                    Ok(ClientCommand::Stop) => {
                        let _ = msg_sender.send(ClientMessage::Disconnected);
                        return;
                    }
                    Ok(ClientCommand::Send(_)) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        let _ = msg_sender.send(ClientMessage::Disconnected);
                        return;
                    }
                }
            } else {
                break;
            }
        }

        let _ = msg_sender.send(ClientMessage::Disconnected);
    }
}

impl Drop for SyncClient {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Instant;

    #[test]
    fn reconnect_drains_stale_commands_queued_while_disconnected() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        let (allow_second_tx, allow_second_rx) = mpsc::channel::<()>();
        let (received_tx, received_rx) = mpsc::channel::<Vec<u8>>();

        let server_thread = thread::spawn(move || {
            // 1. Accept first connection and immediately close it to trigger reconnect.
            let (stream1, _) = listener.accept().expect("accept conn 1");
            let mut ws1 = tungstenite::accept(stream1).expect("ws accept 1");
            let _ = ws1.close(None);
            drop(ws1);

            // Wait until the test finishes queueing a stale command while disconnected.
            allow_second_rx.recv().expect("gate for conn 2");

            // 2. Accept second connection and capture the first binary frame sent on it.
            let (stream2, _) = listener.accept().expect("accept conn 2");
            let mut ws2 = tungstenite::accept(stream2).expect("ws accept 2");
            let _ = ws2.get_mut().set_read_timeout(Some(Duration::from_secs(3)));
            loop {
                match ws2.read() {
                    Ok(Message::Binary(bin)) => {
                        let _ = received_tx.send(bin.to_vec());
                        break;
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(e) => panic!("unexpected ws2 read error: {e}"),
                }
            }
        });

        let (msg_tx, msg_rx) = crossbeam_channel::unbounded();
        let mut client =
            SyncClient::connect(format!("ws://{addr}"), msg_tx, true, 5).expect("connect");
        let cmd_tx = client.get_cmd_sender();

        // Wait for ConnectionLost from connection 1.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_lost = false;
        while Instant::now() < deadline {
            if let Ok(ClientMessage::ConnectionLost) =
                msg_rx.recv_timeout(Duration::from_millis(50))
            {
                saw_lost = true;
                break;
            }
        }
        assert!(saw_lost, "expected ConnectionLost after conn 1 closed");

        // Queue a stale command while disconnected, then allow conn 2 to accept.
        cmd_tx
            .send(ClientCommand::Send(b"STALE".to_vec()))
            .expect("send stale");
        allow_second_tx.send(()).expect("unlock conn 2");

        // Wait for CONNECTED on connection 2, then send a fresh command.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_connected_2 = false;
        while Instant::now() < deadline {
            if let Ok(ClientMessage::Status(s)) = msg_rx.recv_timeout(Duration::from_millis(50)) {
                if s == "CONNECTED" {
                    saw_connected_2 = true;
                    break;
                }
            }
        }
        assert!(saw_connected_2, "expected CONNECTED on conn 2");

        cmd_tx
            .send(ClientCommand::Send(b"FRESH".to_vec()))
            .expect("send fresh");

        let first_frame = received_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("conn 2 must receive a frame");
        assert_eq!(
            first_frame, b"FRESH",
            "stale command queued while disconnected leaked into the new connection"
        );

        client.stop();
        server_thread.join().expect("server thread join");
    }
}
