use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use glam::IVec3;

use crate::server::decision::HandshakeContext;

use super::config::{ServerCommand, ServerConfig, ServerEvent};

#[derive(Debug, Clone)]
pub(crate) struct AuthSettings {
    pub(crate) auth_token: Option<String>,
    pub(crate) allow_loopback_without_token: bool,
    pub(crate) require_token_on_ipc: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SelectionContext {
    pub(crate) min_pos: IVec3,
    pub(crate) size: IVec3,
    pub(crate) dimension: String,
    pub(crate) state_hash: u64,
}

pub(crate) struct ClientHandle {
    pub(crate) tx: Sender<ServerCommand>,
    pub(crate) is_ipc: bool,
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

/// Acquires a mutex even if a previous holder panicked while keeping invariants intact.
#[inline]
pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct ServerInner {
    pub(crate) running: AtomicBool,
    pub(crate) ipc_running: AtomicBool,
    pub(crate) ws_running: AtomicBool,
    pub(crate) epoch: AtomicU64,
    pub(crate) sequence: AtomicU64,
    pub(crate) next_client_id: AtomicU64,
    /// Monotonic per-baseline id source; see [`ServerInner::next_snapshot_uuid`].
    pub(crate) snapshot_counter: AtomicU64,
    pub(crate) server_uuid: [u8; 16],
    pub(crate) max_connections: usize,
    pub(crate) handshake_timeout: Duration,
    pub(crate) ping_interval: Duration,
    pub(crate) auth: Mutex<AuthSettings>,
    pub(crate) selection: Mutex<Option<SelectionContext>>,
    pub(crate) palette: Mutex<PaletteState>,
    pub(crate) clients: Mutex<HashMap<u64, ClientHandle>>,
    pub(crate) event_tx: Sender<ServerEvent>,
    pub(crate) event_rx: Receiver<ServerEvent>,
    pub(crate) local_addr: Mutex<Option<SocketAddr>>,
    pub(crate) ipc_url: Mutex<Option<String>>,
}

impl ServerInner {
    pub(crate) fn new(config: &ServerConfig) -> Self {
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        Self {
            running: AtomicBool::new(true),
            ipc_running: AtomicBool::new(false),
            ws_running: AtomicBool::new(false),
            epoch: AtomicU64::new(1),
            sequence: AtomicU64::new(0),
            next_client_id: AtomicU64::new(1),
            snapshot_counter: AtomicU64::new(0),
            server_uuid: config.server_uuid,
            max_connections: config.max_connections,
            handshake_timeout: config.handshake_timeout,
            ping_interval: config.ping_interval,
            auth: Mutex::new(AuthSettings {
                auth_token: config.auth_token.clone(),
                allow_loopback_without_token: config.allow_loopback_without_token,
                require_token_on_ipc: config.require_token_on_ipc,
            }),
            selection: Mutex::new(None),
            palette: Mutex::new(PaletteState::default()),
            clients: Mutex::new(HashMap::new()),
            event_tx,
            event_rx,
            local_addr: Mutex::new(None),
            ipc_url: Mutex::new(None),
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub(crate) fn is_ipc_running(&self) -> bool {
        self.is_running() && self.ipc_running.load(Ordering::Relaxed)
    }

    pub(crate) fn is_ws_running(&self) -> bool {
        self.is_running() && self.ws_running.load(Ordering::Relaxed)
    }

    pub(crate) fn is_transport_running(&self, is_ipc: bool) -> bool {
        if is_ipc {
            self.is_ipc_running()
        } else {
            self.is_ws_running()
        }
    }

    /// Allocates the identity of one baseline stream.
    ///
    /// Clients drop chunks whose `snapshot_uuid` does not match the baseline they are currently
    /// assembling. The previous implementation derived it from `Instant::now().elapsed()`, which
    /// is nanoseconds since a freshly created `Instant` — effectively zero, so two baselines could
    /// share an id and stale chunks from an aborted stream were accepted into the new one.
    pub(crate) fn next_snapshot_uuid(&self) -> [u8; 16] {
        let counter = self.snapshot_counter.fetch_add(1, Ordering::Relaxed);
        snapshot_uuid_for(counter, self.server_uuid)
    }

    pub(crate) fn remove_client(&self, client_id: u64) {
        let mut clients = lock_unpoisoned(&self.clients);
        clients.remove(&client_id);
    }

    pub(crate) fn palette_snapshot(&self) -> (u32, Vec<(u16, String)>) {
        let pal = lock_unpoisoned(&self.palette);
        let entries = pal
            .list
            .iter()
            .enumerate()
            .map(|(idx, name)| (idx as u16, name.clone()))
            .collect();
        (pal.revision, entries)
    }

    pub(crate) fn get_handshake_context(
        &self,
        is_ipc: bool,
        is_loopback: bool,
    ) -> HandshakeContext {
        let selection_opt = lock_unpoisoned(&self.selection).clone();
        let (min_pos, size, dimension, state_hash) = match selection_opt {
            Some(s) => (s.min_pos, s.size, s.dimension, s.state_hash),
            None => (
                IVec3::ZERO,
                IVec3::ZERO,
                "minecraft:overworld".to_string(),
                0u64,
            ),
        };
        let auth = lock_unpoisoned(&self.auth).clone();

        HandshakeContext {
            server_uuid: self.server_uuid,
            epoch: self.epoch.load(Ordering::Relaxed),
            sequence: self.sequence.load(Ordering::Relaxed),
            min_pos,
            size,
            state_hash,
            dimension,
            expected_auth_token: auth.auth_token,
            is_ipc,
            is_loopback,
            allow_loopback_without_token: auth.allow_loopback_without_token,
            require_token_on_ipc: auth.require_token_on_ipc,
        }
    }
}

/// Deterministic-but-unique baseline id: the counter guarantees distinctness within a session,
/// the server uuid (regenerated per process when the host passes zeros) keeps ids from lining up
/// across restarts, and splitmix64 decorrelates neighbours.
pub(crate) fn snapshot_uuid_for(counter: u64, server_uuid: [u8; 16]) -> [u8; 16] {
    let seed = u64::from_le_bytes([
        server_uuid[0],
        server_uuid[1],
        server_uuid[2],
        server_uuid[3],
        server_uuid[4],
        server_uuid[5],
        server_uuid[6],
        server_uuid[7],
    ]);
    let mut z = counter ^ seed;
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;

    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(&counter.to_le_bytes());
    uuid[8..].copy_from_slice(&z.to_le_bytes());
    uuid
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn snapshot_ids_are_unique_per_baseline() {
        let cfg = ServerConfig {
            server_uuid: [0x11; 16],
            ..Default::default()
        };
        let inner = ServerInner::new(&cfg);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1024 {
            assert!(
                seen.insert(inner.next_snapshot_uuid()),
                "baseline ids must never repeat within a session"
            );
        }
    }

    #[test]
    fn snapshot_ids_do_not_collide_across_server_restarts() {
        let cfg1 = ServerConfig {
            server_uuid: [0x22; 16],
            ..Default::default()
        };
        let cfg2 = ServerConfig {
            server_uuid: [0x33; 16],
            ..Default::default()
        };
        let first = ServerInner::new(&cfg1);
        let second = ServerInner::new(&cfg2);
        assert_ne!(
            first.next_snapshot_uuid(),
            second.next_snapshot_uuid(),
            "two processes starting from counter 0 must not share the first baseline id"
        );
    }

    #[test]
    fn snapshot_ids_neighbours_are_decorrelated() {
        let a = snapshot_uuid_for(0, [9u8; 16]);
        let b = snapshot_uuid_for(1, [9u8; 16]);
        assert_ne!(
            a[8..],
            b[8..],
            "counter neighbours must not share the hash half"
        );
    }

    #[test]
    fn handshake_context_recovers_after_selection_mutex_poisoning() {
        let cfg = ServerConfig {
            server_uuid: [0x44; 16],
            ..Default::default()
        };
        let inner = Arc::new(ServerInner::new(&cfg));
        {
            let mut sel = lock_unpoisoned(&inner.selection);
            *sel = Some(SelectionContext {
                min_pos: IVec3::new(1, 2, 3),
                size: IVec3::new(4, 5, 6),
                dimension: "minecraft:overworld".to_string(),
                state_hash: 0xDEAD_BEEF,
            });
        }

        let poison_target = Arc::clone(&inner);
        let handle = std::thread::spawn(move || {
            let _guard = lock_unpoisoned(&poison_target.selection);
            panic!("simulated panic while holding selection lock");
        });
        assert!(handle.join().is_err());
        assert!(inner.selection.is_poisoned());

        let ctx = inner.get_handshake_context(false, true);
        assert_eq!(ctx.min_pos, IVec3::new(1, 2, 3));
        assert_eq!(ctx.state_hash, 0xDEAD_BEEF);
    }
}
