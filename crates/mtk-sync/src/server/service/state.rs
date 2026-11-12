use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use crossbeam_channel::{Receiver, Sender};
use glam::IVec3;

use crate::server::decision::HandshakeContext;

use super::config::{ServerCommand, ServerConfig, ServerEvent};

#[derive(Debug, Clone)]
pub(crate) struct SelectionContext {
    pub(crate) min_pos: IVec3,
    pub(crate) size: IVec3,
    pub(crate) dimension: String,
    pub(crate) state_hash: u64,
}

pub(crate) struct ClientHandle {
    pub(crate) tx: Sender<ServerCommand>,
}

#[derive(Debug)]
pub(crate) struct PaletteState {
    pub(crate) revision: u32,
    pub(crate) lookup: HashMap<String, u32>,
    pub(crate) list: Vec<String>,
}

impl Default for PaletteState {
    fn default() -> Self {
        let mut lookup = HashMap::new();
        lookup.insert("minecraft:air".to_string(), 0);
        Self {
            revision: 1,
            lookup,
            list: vec!["minecraft:air".to_string()],
        }
    }
}

pub(crate) struct ServerInner {
    pub(crate) running: AtomicBool,
    pub(crate) epoch: AtomicU64,
    pub(crate) sequence: AtomicU64,
    pub(crate) server_uuid: [u8; 16],
    pub(crate) selection: Mutex<Option<SelectionContext>>,
    pub(crate) palette: Mutex<PaletteState>,
    pub(crate) clients: Mutex<HashMap<u64, ClientHandle>>,
    pub(crate) client_subscriptions: Mutex<HashMap<u64, Vec<String>>>,
    pub(crate) event_tx: Sender<ServerEvent>,
    pub(crate) event_rx: Receiver<ServerEvent>,
    pub(crate) local_addr: Mutex<Option<SocketAddr>>,
}

impl ServerInner {
    pub(crate) fn new(server_uuid: [u8; 16]) -> Self {
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        Self {
            running: AtomicBool::new(true),
            epoch: AtomicU64::new(1),
            sequence: AtomicU64::new(1),
            server_uuid,
            selection: Mutex::new(None),
            palette: Mutex::new(PaletteState::default()),
            clients: Mutex::new(HashMap::new()),
            client_subscriptions: Mutex::new(HashMap::new()),
            event_tx,
            event_rx,
            local_addr: Mutex::new(None),
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub(crate) fn remove_client(&self, client_id: u64) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.remove(&client_id);
        }
        if let Ok(mut subs) = self.client_subscriptions.lock() {
            subs.remove(&client_id);
        }
    }

    pub(crate) fn set_client_subscriptions(&self, client_id: u64, subscribed: Vec<String>) {
        if let Ok(mut subs) = self.client_subscriptions.lock() {
            subs.insert(client_id, subscribed);
        }
    }

    pub(crate) fn get_handshake_context(
        &self,
        config: &ServerConfig,
        addr: SocketAddr,
    ) -> HandshakeContext {
        let selection_opt = self.selection.lock().unwrap().clone();
        let (min_pos, size, dimension, state_hash) = match selection_opt {
            Some(s) => (s.min_pos, s.size, s.dimension, s.state_hash),
            None => (
                IVec3::ZERO,
                IVec3::ZERO,
                "minecraft:overworld".to_string(),
                0u64,
            ),
        };

        HandshakeContext {
            server_uuid: self.server_uuid,
            epoch: self.epoch.load(Ordering::Relaxed),
            sequence: self.sequence.load(Ordering::Relaxed),
            min_pos,
            size,
            state_hash,
            dimension,
            expected_auth_token: config.auth_token.clone(),
            is_loopback: addr.ip().is_loopback(),
            allow_loopback_without_token: config.allow_loopback_without_token,
        }
    }
}
