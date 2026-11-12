use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use mtk_sync::{normalize_stream_progress, LiveSyncSession, SyncEvent};

use crate::cull::PyFaceCuller;
use crate::mesh::PyMeshData;
use crate::model::PyBakedModelDatabase;
use crate::voxel::{PyMesherConfig, PyVoxelStorage};

/// Python wrapper for `LiveSyncSession` real-time WebSocket synchronization engine (RFC v3.1).
#[pyclass(name = "LiveSyncSession")]
pub struct PyLiveSyncSession {
    inner: LiveSyncSession,
}

#[pymethods]
impl PyLiveSyncSession {
    #[new]
    #[pyo3(signature = (config=None, culler=None, model_db=None))]
    pub fn new(
        config: Option<&PyMesherConfig>,
        culler: Option<&PyFaceCuller>,
        model_db: Option<&PyBakedModelDatabase>,
    ) -> Self {
        let cfg = config.map(|c| c.inner.clone());
        let cul = culler.map(|c| c.inner.clone());
        let mdb = model_db.map(|db| db.inner.clone());
        let session = LiveSyncSession::new(cfg, cul, mdb);
        Self { inner: session }
    }

    /// Clears the cached section meshes and unified world mesh on the underlying VoxelWorld.
    pub fn clear_cache(&self) {
        self.inner.clear_cache();
    }

    /// Hot-reloads mesher configuration and model database on the underlying VoxelWorld.
    #[pyo3(signature = (config=None, model_db=None))]
    pub fn hot_reload(
        &self,
        config: Option<&PyMesherConfig>,
        model_db: Option<&PyBakedModelDatabase>,
    ) {
        self.inner.hot_reload(
            config.map(|c| c.inner.clone()),
            model_db.map(|db| db.inner.clone()),
        );
    }

    /// Meshes the entire active VoxelStorage volume and returns a unified `MeshData`.
    pub fn get_world_mesh(&self) -> PyMeshData {
        let mesh = self.inner.get_world_mesh();
        let profile = self.inner.last_profile();
        let (dynamic_entity_atlases, dynamic_entity_slots) =
            self.inner.dynamic_entity_atlases_and_slots_if_used();
        PyMeshData {
            inner: mesh,
            profile,
            dynamic_entity_atlases,
            dynamic_entity_slots,
        }
    }

    /// Returns a dict mapping Chunk ID (e.g. 0xFFF0, 0xFFF1) to dynamic entity atlas RGBA bytes (1024x1024x4).
    pub fn get_dynamic_entity_atlases<'py>(
        &self,
        py: Python<'py>,
    ) -> Bound<'py, pyo3::types::PyDict> {
        let dict = pyo3::types::PyDict::new(py);
        for (chunk_id, buf) in self.inner.dynamic_entity_atlases() {
            let _ = dict.set_item(chunk_id, PyBytes::new(py, &buf));
        }
        dict
    }

    /// Sets the on-disk cache directory for downloaded player skins.
    pub fn set_skin_cache_dir(&mut self, cache_dir: Option<String>) {
        self.inner
            .set_skin_cache_dir(cache_dir.map(std::path::PathBuf::from));
    }

    /// Starts the background sync client thread connecting to Minecraft at `url`,
    /// authenticating the HELLO with the shared session token (RFC v3.1 §4.1) and
    /// subscribing to continuous visual entities (RFC v3.2).
    #[pyo3(signature = (
        url="ws://localhost:8765",
        auto_reconnect=true,
        max_reconnect_attempts=5,
        auth_token=None,
        subscribed_entities=None
    ))]
    pub fn start(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        auth_token: Option<&str>,
        subscribed_entities: Option<Vec<String>>,
    ) -> PyResult<()> {
        // The token must ride on the HELLO even when there is no cached state:
        // `LiveSyncSession::start` builds the default HELLO from this value.
        self.inner.set_auth_token(auth_token.unwrap_or(""));
        self.inner.set_subscribed_entities(subscribed_entities);
        self.inner
            .start(url, auto_reconnect, max_reconnect_attempts)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    /// Returns the active or default subscribed continuous visual entity types.
    #[getter]
    pub fn subscribed_entities(&self) -> Vec<String> {
        self.inner.subscribed_entities()
    }

    /// Stops the live sync session cleanly and disconnects from server.
    pub fn stop(&mut self) {
        self.inner.stop();
    }

    /// Sends a RESYNC_REQUEST to the server.
    pub fn request_resync(&self) -> PyResult<()> {
        self.inner
            .request_resync()
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    /// Whether the background synchronization session worker is currently active.
    #[getter]
    pub fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    /// Whether the session is actively connected to the server.
    #[getter]
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    /// Current connection status string ("CONNECTED", "CONNECTING...", "DISCONNECTED", etc.).
    #[getter]
    pub fn status(&self) -> String {
        self.inner.status()
    }

    /// Returns current client generation ID.
    #[getter]
    pub fn client_generation(&self) -> u64 {
        self.inner.client_generation()
    }

    /// Returns current session sequence ID.
    #[getter]
    pub fn current_sequence(&self) -> u64 {
        self.inner.current_sequence()
    }

    /// Returns current session epoch ID.
    #[getter]
    pub fn current_epoch(&self) -> u64 {
        self.inner.current_epoch()
    }

    /// Returns the last server-confirmed canonical world state hash.
    #[getter]
    pub fn state_hash(&self) -> u64 {
        self.inner.current_state_hash()
    }

    /// Returns the server instance UUID as a 32-character lowercase hex string.
    #[getter]
    pub fn server_uuid(&self) -> String {
        self.inner.server_uuid_hex()
    }

    /// Polls all pending synchronization events from the background Rust engine (non-blocking).
    ///
    /// `coalesce_meshes=True` keeps only the newest `WORLD_MESH_READY` generation.
    #[pyo3(signature = (coalesce_meshes=false))]
    pub fn poll_events<'py>(
        &self,
        py: Python<'py>,
        coalesce_meshes: bool,
    ) -> PyResult<Bound<'py, PyList>> {
        let events = if coalesce_meshes {
            self.inner.poll_events_coalesced()
        } else {
            self.inner.poll_events()
        };
        let list = PyList::empty(py);

        for evt in events {
            let dict = PyDict::new(py);
            match evt {
                SyncEvent::StatusChange(status) => {
                    dict.set_item("type", "STATUS_CHANGE")?;
                    dict.set_item("status", status)?;
                }
                SyncEvent::SelectionUpdated { min_pos, size } => {
                    dict.set_item("type", "SELECTION_UPDATED")?;
                    dict.set_item("min_x", min_pos.x)?;
                    dict.set_item("min_y", min_pos.y)?;
                    dict.set_item("min_z", min_pos.z)?;
                    dict.set_item("size_x", size.x)?;
                    dict.set_item("size_y", size.y)?;
                    dict.set_item("size_z", size.z)?;
                }
                SyncEvent::Handshake {
                    epoch,
                    sequence,
                    decision,
                    decision_code,
                    dimension,
                } => {
                    dict.set_item("type", "HANDSHAKE")?;
                    dict.set_item("epoch", epoch)?;
                    dict.set_item("sequence", sequence)?;
                    dict.set_item("decision", decision)?;
                    dict.set_item("decision_code", decision_code)?;
                    dict.set_item("dimension", dimension)?;
                }
                SyncEvent::WorldMeshReady {
                    mesh,
                    generation,
                    profile,
                } => {
                    dict.set_item("type", "WORLD_MESH_READY")?;
                    let (dyn_atlases, dyn_slots) = if mesh.has_dynamic_entity_chunks() {
                        (
                            Some(self.inner.dynamic_entity_atlases()),
                            Some(self.inner.dynamic_entity_slots()),
                        )
                    } else {
                        (None, None)
                    };
                    let py_mesh = PyMeshData {
                        inner: mesh,
                        profile,
                        dynamic_entity_atlases: dyn_atlases,
                        dynamic_entity_slots: dyn_slots,
                    };
                    dict.set_item("mesh", py_mesh)?;
                    dict.set_item("generation", generation)?;
                }
                SyncEvent::StreamProgress {
                    stage,
                    current,
                    total,
                    message,
                } => {
                    dict.set_item("type", "STREAM_PROGRESS")?;
                    dict.set_item("stage", &stage)?;
                    dict.set_item("current", current)?;
                    dict.set_item("total", total)?;
                    dict.set_item("message", message)?;
                    let pct = normalize_stream_progress(&stage, current, total);
                    dict.set_item("percent", pct)?;
                    dict.set_item("overall_percent", pct)?;
                }
                SyncEvent::StreamFinished { built_sections } => {
                    dict.set_item("type", "STREAM_FINISHED")?;
                    dict.set_item("built_sections", built_sections)?;
                }
                SyncEvent::DeltaApplied {
                    change_count,
                    affected_sections,
                } => {
                    dict.set_item("type", "DELTA_APPLIED")?;
                    dict.set_item("change_count", change_count)?;
                    let aff_list = PyList::empty(py);
                    for c in affected_sections {
                        let _ = aff_list.append((c.x, c.y, c.z));
                    }
                    dict.set_item("affected_sections", aff_list)?;
                }
                SyncEvent::Verified {
                    is_verified,
                    message,
                } => {
                    dict.set_item("type", "VERIFIED")?;
                    dict.set_item("is_verified", is_verified)?;
                    dict.set_item("message", message)?;
                }
                SyncEvent::VisualEntitiesUpdated {
                    upserted,
                    deleted_uuids,
                } => {
                    dict.set_item("type", "VISUAL_ENTITIES_UPDATED")?;
                    let upserted_list = PyList::empty(py);
                    for e in upserted {
                        let json_str = serde_json::to_string(&e).map_err(|err| {
                            pyo3::exceptions::PyValueError::new_err(format!(
                                "Failed to serialize visual entity: {err}"
                            ))
                        })?;
                        upserted_list.append(json_str)?;
                    }
                    dict.set_item("upserted", upserted_list)?;
                    let deleted_list = PyList::empty(py);
                    for u in deleted_uuids {
                        deleted_list.append(u)?;
                    }
                    dict.set_item("deleted_uuids", deleted_list)?;
                }
                SyncEvent::Warning(msg) => {
                    dict.set_item("type", "WARNING")?;
                    dict.set_item("message", msg)?;
                }
                SyncEvent::Error(msg) => {
                    dict.set_item("type", "ERROR")?;
                    dict.set_item("message", msg)?;
                }
            }
            list.append(dict)?;
        }

        Ok(list)
    }

    /// Returns a copy of the underlying `VoxelStorage`.
    pub fn get_storage(&self) -> PyResult<PyVoxelStorage> {
        let w = self.inner.world.read().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("World lock poisoned: {}", e))
        })?;
        Ok(PyVoxelStorage {
            inner: w.storage.clone(),
        })
    }

    /// Restores voxel storage from an existing point cloud without re-meshing.
    pub fn load_from_point_cloud(&self, point_cloud: &crate::point_cloud::PyVoxelPointCloud) {
        self.inner.load_from_point_cloud(&point_cloud.inner);
    }

    /// Starts live sync using cached handshake parameters for 0ms zero-rebuild reconnect.
    #[pyo3(signature = (
        url="ws://127.0.0.1:8765",
        auto_reconnect=true,
        max_reconnect_attempts=5,
        server_uuid="",
        epoch=0,
        sequence=0,
        min_pos=(0, 0, 0),
        size=(0, 0, 0),
        state_hash=0,
        dimension="minecraft:overworld",
        auth_token=None
    ))]
    pub fn start_with_cache(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        server_uuid: &str,
        epoch: u64,
        sequence: u64,
        min_pos: (i32, i32, i32),
        size: (i32, i32, i32),
        state_hash: u64,
        dimension: &str,
        auth_token: Option<&str>,
    ) -> PyResult<()> {
        let uuid_bytes = parse_uuid_hex(server_uuid);
        self.inner.set_auth_token(auth_token.unwrap_or(""));
        self.inner
            .start_with_cache(
                url,
                auto_reconnect,
                max_reconnect_attempts,
                uuid_bytes,
                epoch,
                sequence,
                glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
                glam::IVec3::new(size.0, size.1, size.2),
                state_hash,
                dimension.to_string(),
            )
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }
}

fn parse_uuid_hex(raw: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let trimmed = raw.trim();
    let without_prefix = if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        &trimmed[2..]
    } else {
        trimmed
    };
    let clean: Vec<u8> = without_prefix.bytes().filter(|b| *b != b'-').collect();
    if clean.len() != 32 {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(clean[i * 2]);
        let lo = hex_nibble(clean[i * 2 + 1]);
        match (hi, lo) {
            (Some(h), Some(l)) => *slot = (h << 4) | l,
            _ => return [0u8; 16],
        }
    }
    out
}

#[inline]
fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Python wrapper for native RFC v3.1 `LiveSyncServer`.
#[pyclass(name = "LiveSyncServer")]
pub struct PyLiveSyncServer {
    inner: Option<mtk_sync::LiveSyncServer>,
}

#[pymethods]
impl PyLiveSyncServer {
    #[new]
    #[pyo3(signature = (
        bind_address="127.0.0.1:8765".to_string(),
        auth_token=None,
        allow_loopback_without_token=true,
        server_uuid=None
    ))]
    pub fn new(
        bind_address: String,
        auth_token: Option<String>,
        allow_loopback_without_token: bool,
        server_uuid: Option<String>,
    ) -> PyResult<Self> {
        let mut uuid_bytes = [0u8; 16];
        if let Some(ref s) = server_uuid {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                uuid_bytes = parse_uuid_hex(trimmed);
            }
        }

        let config = mtk_sync::ServerConfig {
            bind_address,
            auth_token,
            allow_loopback_without_token,
            server_uuid: uuid_bytes,
            max_connections: 8,
            handshake_timeout: std::time::Duration::from_millis(5000),
            ping_interval: std::time::Duration::from_millis(5000),
        };
        let server = mtk_sync::LiveSyncServer::start(config).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to start LiveSyncServer: {}",
                e
            ))
        })?;
        Ok(Self {
            inner: Some(server),
        })
    }

    /// Stops the server and disconnects all clients.
    pub fn stop(&mut self) {
        if let Some(mut server) = self.inner.take() {
            server.stop();
        }
    }

    /// Returns whether the server is actively running.
    pub fn is_running(&self) -> bool {
        self.inner.as_ref().map(|s| s.is_running()).unwrap_or(false)
    }

    /// Returns the unique server instance UUID as a hex string.
    #[getter]
    pub fn server_uuid(&self) -> PyResult<String> {
        let server = self
            .inner
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Server is stopped"))?;
        let bytes = server.server_uuid();
        let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        Ok(hex)
    }

    /// Returns the number of currently connected clients.
    pub fn client_count(&self) -> usize {
        self.inner.as_ref().map(|s| s.client_count()).unwrap_or(0)
    }

    /// Returns the local bound socket address as string.
    pub fn local_addr(&self) -> Option<String> {
        self.inner
            .as_ref()
            .and_then(|s| s.local_addr().map(|a| a.to_string()))
    }

    /// Updates the authoritative selection.
    #[pyo3(signature = (min_pos, size, dimension="minecraft:overworld".to_string()))]
    pub fn set_selection(
        &self,
        min_pos: (i32, i32, i32),
        size: (i32, i32, i32),
        dimension: String,
    ) -> PyResult<()> {
        let server = self
            .inner
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Server is stopped"))?;
        server.set_selection(
            glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
            glam::IVec3::new(size.0, size.1, size.2),
            dimension,
        );
        Ok(())
    }

    /// Clears the authoritative selection and cancels any active streams.
    pub fn clear_selection(&self) -> PyResult<()> {
        let server = self
            .inner
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Server is stopped"))?;
        server.clear_selection();
        Ok(())
    }

    /// Submits a raw baseline scan to the server. The server computes state_hash, updates selection,
    /// initializes the dynamic palette, encodes chunked baseline frames, and streams to client_id (or broadcasts if 0).
    /// Returns the computed state_hash.
    #[pyo3(signature = (
        client_id,
        min_pos,
        size,
        dimension,
        raw_states,
        block_palette,
        raw_biomes,
        biome_palette,
        block_entities=None
    ))]
    pub fn submit_baseline(
        &self,
        client_id: u64,
        min_pos: (i32, i32, i32),
        size: (i32, i32, i32),
        dimension: String,
        raw_states: Vec<u32>,
        block_palette: Vec<String>,
        raw_biomes: Vec<u32>,
        biome_palette: Vec<String>,
        block_entities: Option<Vec<((i32, i32, i32), String)>>,
    ) -> PyResult<u64> {
        let server = self
            .inner
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Server is stopped"))?;

        let entities: Vec<(glam::IVec3, String)> = block_entities
            .unwrap_or_default()
            .into_iter()
            .map(|((x, y, z), json)| (glam::IVec3::new(x, y, z), json))
            .collect();

        server
            .submit_baseline(
                client_id,
                glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
                glam::IVec3::new(size.0, size.1, size.2),
                dimension,
                &raw_states,
                &block_palette,
                &raw_biomes,
                &biome_palette,
                &entities,
            )
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{:?}", e)))
    }

    /// Submits incremental block changes and entity mutations, automatically managing the dynamic palette.
    #[pyo3(signature = (
        rel_coords,
        block_states,
        block_entities=None
    ))]
    pub fn submit_deltas(
        &self,
        rel_coords: Vec<(i32, i32, i32)>,
        block_states: Vec<String>,
        block_entities: Option<Vec<((i32, i32, i32), Option<String>)>>,
    ) -> PyResult<()> {
        let server = self
            .inner
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Server is stopped"))?;

        let coords: Vec<glam::IVec3> = rel_coords
            .into_iter()
            .map(|(x, y, z)| glam::IVec3::new(x, y, z))
            .collect();

        let entities: Vec<(glam::IVec3, Option<String>)> = block_entities
            .unwrap_or_default()
            .into_iter()
            .map(|((x, y, z), json)| (glam::IVec3::new(x, y, z), json))
            .collect();

        server
            .submit_deltas(&coords, &block_states, &entities)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{:?}", e)))
    }

    /// Polls pending server events as a list of dicts.
    pub fn poll_events<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        if let Some(server) = self.inner.as_ref() {
            for event in server.poll_events() {
                let dict = PyDict::new(py);
                match event {
                    mtk_sync::ServerEvent::ClientConnected { client_id, addr } => {
                        dict.set_item("type", "client_connected")?;
                        dict.set_item("client_id", client_id)?;
                        dict.set_item("addr", addr.to_string())?;
                    }
                    mtk_sync::ServerEvent::ClientDisconnected { client_id, reason } => {
                        dict.set_item("type", "client_disconnected")?;
                        dict.set_item("client_id", client_id)?;
                        dict.set_item("reason", reason)?;
                    }
                    mtk_sync::ServerEvent::BaselineRequested {
                        client_id,
                        epoch,
                        snapshot_seq,
                    } => {
                        dict.set_item("type", "baseline_requested")?;
                        dict.set_item("client_id", client_id)?;
                        dict.set_item("epoch", epoch)?;
                        dict.set_item("snapshot_seq", snapshot_seq)?;
                    }
                    mtk_sync::ServerEvent::ClientResync { client_id, reason } => {
                        dict.set_item("type", "client_resync")?;
                        dict.set_item("client_id", client_id)?;
                        dict.set_item("reason", format!("{:?}", reason))?;
                    }
                }
                list.append(dict)?;
            }
        }
        Ok(list)
    }
}
