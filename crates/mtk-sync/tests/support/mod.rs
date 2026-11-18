//! Scripted protocol peer for session-level tests.
//!
//! Talking to a real Minecraft client is slow, manual and not reproducible. The session state
//! machine only depends on the wire protocol, so a test can own both ends: this peer accepts a
//! single WebSocket connection, replays an exact packet script and records everything the client
//! sends back (`HELLO`, `ACK`, `RESYNC_REQUEST`, ...).
//!
//! Assertions then read like the protocol RFC: "a palette update must advance the sequence" or
//! "a gap must produce one resync request".

#![allow(dead_code)] // Each integration test binary uses a different subset.

use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use mtk_sync::{decode_packet, encode_packet, Packet};
use tungstenite::Message;

/// Commands the test sends to the peer thread.
pub enum PeerCommand {
    /// Encode and send these packets in order.
    Send(Vec<Packet>),
    /// Close the WebSocket and stop serving.
    Close,
}

/// Handle to a scripted peer running on its own thread.
pub struct FakePeer {
    /// `ws://127.0.0.1:PORT` the session should connect to.
    pub url: String,
    commands: Sender<PeerCommand>,
    received: Receiver<Packet>,
    join: Option<JoinHandle<()>>,
}

impl FakePeer {
    /// Binds an ephemeral port and starts serving in the background.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        let (commands, command_rx) = channel();
        let (received_tx, received) = channel();

        let join = thread::spawn(move || serve(listener, command_rx, received_tx));

        Self {
            url: format!("ws://127.0.0.1:{port}"),
            commands,
            received,
            join: Some(join),
        }
    }

    /// Sends packets to the connected client.
    pub fn send(&self, packets: &[Packet]) {
        self.commands
            .send(PeerCommand::Send(packets.to_vec()))
            .expect("peer thread must be alive");
    }

    /// Waits until a packet matching `predicate` arrives from the client.
    pub fn wait_for(
        &self,
        timeout: Duration,
        what: &str,
        predicate: impl Fn(&Packet) -> bool,
    ) -> Packet {
        let deadline = Instant::now() + timeout;
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match self.received.recv_timeout(Duration::from_millis(20)) {
                Ok(packet) => {
                    if predicate(&packet) {
                        return packet;
                    }
                    seen.push(format!("{packet:?}"));
                }
                Err(_) => continue,
            }
        }
        panic!("timed out waiting for {what}; client sent: {seen:?}");
    }

    /// Drains everything the client sent so far (non-blocking).
    pub fn drain(&self) -> Vec<Packet> {
        let mut packets = Vec::new();
        while let Ok(packet) = self.received.try_recv() {
            packets.push(packet);
        }
        packets
    }

    /// Closes the connection from the server side.
    pub fn close(&self) {
        let _ = self.commands.send(PeerCommand::Close);
    }

    /// True while the peer thread is serving the current connection.
    pub fn is_serving(&self) -> bool {
        self.join.as_ref().is_some_and(|join| !join.is_finished())
    }
}

impl Drop for FakePeer {
    fn drop(&mut self) {
        let _ = self.commands.send(PeerCommand::Close);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn serve(listener: TcpListener, commands: Receiver<PeerCommand>, received: Sender<Packet>) {
    let Ok((stream, _)) = listener.accept() else {
        return;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
    let Ok(mut socket) = tungstenite::accept(stream) else {
        return;
    };
    let _ = socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(20)));

    loop {
        // Outbound script.
        loop {
            match commands.try_recv() {
                Ok(PeerCommand::Send(packets)) => {
                    for packet in packets {
                        let bytes = encode_packet(&packet).expect("test packet must encode");
                        if socket.send(Message::Binary(bytes.into())).is_err() {
                            return;
                        }
                    }
                }
                Ok(PeerCommand::Close) => {
                    let _ = socket.close(None);
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        // Inbound traffic.
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                if let Ok(packet) = decode_packet(&bytes) {
                    let _ = received.send(packet);
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return,
        }
    }
}

/// A `WELCOME` that drops the client straight into `LIVE` without a baseline round trip.
pub fn welcome_live(epoch: u64, server_seq: u64, min: glam::IVec3, size: glam::IVec3) -> Packet {
    Packet::Welcome {
        negotiated_version: mtk_sync::PROTOCOL_VERSION as u16,
        server_uuid: [7u8; 16],
        epoch,
        server_seq,
        current_min: min,
        current_size: size,
        current_state_hash: 0xdead_beef,
        decision: mtk_sync::SyncDecision::VerifiedSkipRebuild,
        message_or_dimension: "minecraft:overworld".to_string(),
    }
}

/// Builds a single-entry `DELTA_OP`.
pub fn delta_op(
    epoch: u64,
    seq_id: u64,
    palette_revision: u32,
    rel_pos: glam::IVec3,
    state_id: u16,
) -> Packet {
    let changes = vec![mtk_sync::DeltaChangeEntry { rel_pos, state_id }];
    Packet::DeltaOp {
        epoch,
        seq_id,
        palette_revision,
        count: changes.len() as u16,
        // The client verifies the batch checksum; a scripted delta must be as valid as a real one.
        batch_xxhash64: mtk_sync::compute_delta_batch_hash(&changes),
        changes,
    }
}
