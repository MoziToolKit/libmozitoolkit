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

use crate::protocol::{decode_packet, Packet};

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
