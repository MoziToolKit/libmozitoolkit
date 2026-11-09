use std::sync::atomic::Ordering;

use crate::client::{ClientCommand, ClientMessage, SyncClient};
use crate::protocol::*;

use super::dispatcher::{event_worker_loop, handle_packet};
use super::hello::{default_hello, with_auth_token};
use super::LiveSyncSession;

impl LiveSyncSession {
    /// Sends a RESYNC_REQUEST packet to request recovery / re-sync.
    pub fn request_resync(&self) -> Result<(), String> {
        let (epoch, seq) = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            // Drop any partially received baseline so the incoming snapshot can
            // be assembled from scratch; otherwise stale chunks are silently
            // rejected and the UI can stall mid-progress.
            s.awaiting_baseline();
            // Manual retry: give the selection a fresh retry budget.
            s.validation_failures = 0;
            (s.epoch, s.sequence)
        };
        let packet = Packet::ResyncRequest {
            epoch,
            missing_from_seq: seq,
            missing_to_seq: seq + 1,
            reason: ResyncReason::StateHashMismatch,
        };
        let buf = encode_packet(&packet);
        if let Some(cmd) = &self.cmd_sender {
            cmd.send(ClientCommand::Send(buf))
                .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("Client not connected".to_string())
        }
    }

    /// Starts live sync using cached handshake parameters for 0ms zero-rebuild reconnect.
    pub fn start_with_cache(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        server_uuid: [u8; 16],
        epoch: u64,
        sequence: u64,
        min: glam::IVec3,
        size: glam::IVec3,
        state_hash: u64,
        dimension: String,
    ) -> Result<(), String> {
        let hello = Packet::Hello {
            min_version: PROTOCOL_VERSION as u16,
            max_version: PROTOCOL_VERSION as u16,
            capabilities: 0x03,
            flags: 0,
            cached_world_uuid: server_uuid,
            cached_epoch: epoch,
            cached_seq: sequence,
            cached_min: min,
            cached_size: size,
            cached_state_hash: state_hash,
            auth_token: self.auth_token.clone(),
            cached_dimension: dimension,
        };
        self.start_with_hello(url, auto_reconnect, max_reconnect_attempts, Some(hello))
    }

    /// Manually processes a packet directly into the state machine (useful for tests and mocks).
    pub fn process_packet_direct(&self, packet: Packet) {
        let dummy_cmd_sender = self
            .cmd_sender
            .clone()
            .unwrap_or_else(|| crossbeam_channel::unbounded().0);
        let mut state_guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        handle_packet(
            packet,
            0,
            &mut state_guard,
            &self.world,
            &dummy_cmd_sender,
            &self.event_sender,
            &self.generation,
        );
    }

    /// Starts the live sync session connecting to the given WebSocket `url`,
    /// optionally passing cached session parameters for HELLO handshake negotiation.
    pub fn start_with_hello(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        initial_hello: Option<Packet>,
    ) -> Result<(), String> {
        self.stop();

        // RFC v3.1 §4.1: every HELLO must carry the shared auth token. Hosts with no
        // persisted session state pass `None`, which used to produce a zero-token
        // HELLO; token-protected servers then rejected the client with
        // "Authentication failed" even though both sides held the same secret.
        let initial_hello = match initial_hello {
            Some(hello) => with_auth_token(hello, &self.auth_token),
            None => default_hello(&self.auth_token),
        };

        // Drop any mesh/progress events left over from the previous connection so a
        // reconnecting host cannot apply a stale world mesh to the new session.
        while self.event_receiver.try_recv().is_ok() {}

        // Reset the per-connection state machine. Reusing a session (hot reconnect
        // from Blender) must never inherit a half-assembled baseline, a stale
        // epoch/sequence or palette entries from the previous selection.
        {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.reset_for_new_connection();
        }

        *self.status.write().unwrap_or_else(|e| e.into_inner()) = "CONNECTING...".to_string();
        self.is_connected.store(false, Ordering::SeqCst);

        let (msg_sender, msg_receiver) = crossbeam_channel::unbounded::<ClientMessage>();
        let msg_sender_skin = msg_sender.clone();
        {
            let mut w = self.world.write().unwrap_or_else(|e| e.into_inner());
            w.set_skin_resolved_callback(move |key| {
                let _ = msg_sender_skin.send(ClientMessage::SkinDownloaded(key));
            });
        }
        let client = SyncClient::connect(
            url.to_string(),
            msg_sender,
            auto_reconnect,
            max_reconnect_attempts,
        )?;

        let cmd_sender = client.get_cmd_sender();
        self.cmd_sender = Some(cmd_sender.clone());
        self.client = Some(client);
        self.worker_running.store(true, Ordering::SeqCst);

        let world_clone = self.world.clone();
        let event_sender = self.event_sender.clone();
        let running_clone = self.worker_running.clone();
        let is_connected_clone = self.is_connected.clone();
        let status_clone = self.status.clone();
        let generation_clone = self.generation.clone();
        let state_clone = self.state.clone();

        let handle = std::thread::spawn(move || {
            event_worker_loop(
                msg_receiver,
                cmd_sender,
                world_clone,
                event_sender,
                running_clone,
                is_connected_clone,
                status_clone,
                generation_clone,
                state_clone,
                initial_hello,
            );
        });

        self.worker_handle = Some(handle);
        Ok(())
    }

    /// Convenience start method using default HELLO parameters.
    pub fn start(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
    ) -> Result<(), String> {
        self.start_with_hello(url, auto_reconnect, max_reconnect_attempts, None)
    }

    /// Stops the live sync session cleanly and disconnects from server.
    pub fn stop(&mut self) {
        self.worker_running.store(false, Ordering::SeqCst);
        self.is_connected.store(false, Ordering::SeqCst);
        *self.status.write().unwrap_or_else(|e| e.into_inner()) = "DISCONNECTED".to_string();

        if let Some(mut client) = self.client.take() {
            client.stop();
        }

        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.cmd_sender = None;
    }
}
