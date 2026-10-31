use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use glam::IVec3;
use mtk_sync::protocol::*;
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

    /// Sets or updates the active baked model database.
    pub fn set_model_db(&mut self, model_db: Option<&PyBakedModelDatabase>) {
        self.inner.set_model_db(model_db.map(|db| db.inner.clone()));
    }

    /// Clears the cached section meshes and unified world mesh on the underlying VoxelWorld.
    pub fn clear_cache(&self) {
        self.inner.clear_cache();
    }

    /// Sets or updates the active mesher configuration on the underlying VoxelWorld.
    pub fn set_config(&self, config: &PyMesherConfig) {
        self.inner.set_config(config.inner.clone());
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
        let dynamic_entity_atlas = self.inner.dynamic_entity_atlas_rgba();
        let dynamic_entity_atlases = Some(self.inner.dynamic_entity_atlases());
        PyMeshData {
            inner: mesh,
            profile,
            dynamic_entity_atlas,
            dynamic_entity_atlases,
        }
    }

    /// Returns the dynamic entity atlas texture RGBA bytes (1024x1024x4), or None if no dynamic entities.
    pub fn get_dynamic_entity_atlas<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner
            .dynamic_entity_atlas_rgba()
            .map(|bytes| PyBytes::new(py, &bytes))
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
    /// optionally passing cached session parameters for zero-rebuild fast path verification.
    #[pyo3(signature = (
        url="ws://localhost:8765",
        auto_reconnect=true,
        max_reconnect_attempts=5,
        cached_uuid=None,
        cached_epoch=0,
        cached_seq=0,
        cached_min=None,
        cached_size=None,
        cached_state_hash=0,
        auth_token=None,
        cached_dimension=None
    ))]
    pub fn start(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        cached_uuid: Option<&[u8]>,
        cached_epoch: u64,
        cached_seq: u64,
        cached_min: Option<(i32, i32, i32)>,
        cached_size: Option<(i32, i32, i32)>,
        cached_state_hash: u64,
        auth_token: Option<&str>,
        cached_dimension: Option<&str>,
    ) -> PyResult<()> {
        // The token must ride on the HELLO even when there is no cached state:
        // `start_with_hello` builds the default HELLO from this value.
        self.inner.set_auth_token(auth_token.unwrap_or(""));

        let initial_hello = if cached_uuid.is_some() || cached_state_hash != 0 {
            let mut uuid_arr = [0u8; 16];
            if let Some(b) = cached_uuid {
                if b.len() == 16 {
                    uuid_arr.copy_from_slice(b);
                }
            }
            let c_min = cached_min
                .map(|(x, y, z)| IVec3::new(x, y, z))
                .unwrap_or(IVec3::ZERO);
            let c_sz = cached_size
                .map(|(x, y, z)| IVec3::new(x, y, z))
                .unwrap_or(IVec3::ZERO);
            Some(Packet::Hello {
                min_version: PROTOCOL_VERSION as u16,
                max_version: PROTOCOL_VERSION as u16,
                capabilities: 0x03, // Zstd + DNA Direct Memory
                flags: 0,
                cached_world_uuid: uuid_arr,
                cached_epoch,
                cached_seq,
                cached_min: c_min,
                cached_size: c_sz,
                cached_state_hash,
                // Filled in by `LiveSyncSession::start_with_hello` from the trimmed
                // session token so both HELLO paths normalize identically.
                auth_token: String::new(),
                cached_dimension: cached_dimension
                    .unwrap_or("minecraft:overworld")
                    .to_string(),
            })
        } else {
            None
        };

        self.inner
            .start_with_hello(url, auto_reconnect, max_reconnect_attempts, initial_hello)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
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
                    let py_mesh = PyMeshData {
                        inner: mesh,
                        profile,
                        dynamic_entity_atlas: self.inner.dynamic_entity_atlas_rgba(),
                        dynamic_entity_atlases: Some(self.inner.dynamic_entity_atlases()),
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

    /// Export host-agnostic persisted session state as JSON.
    #[pyo3(signature = (user_modified=false))]
    pub fn export_persisted_state_json(&self, user_modified: bool) -> String {
        self.inner.export_persisted_state_json(user_modified)
    }

    /// Start a session, restoring a cached HELLO from persisted state JSON when possible.
    #[pyo3(signature = (
        url="ws://localhost:8765",
        auto_reconnect=true,
        max_reconnect_attempts=5,
        state_json=None,
        auth_token=None
    ))]
    pub fn start_with_persisted_state(
        &mut self,
        url: &str,
        auto_reconnect: bool,
        max_reconnect_attempts: usize,
        state_json: Option<&str>,
        auth_token: Option<&str>,
    ) -> PyResult<()> {
        // Persisted state may not be a reconnect candidate (or the host may
        // disable the fast path); the session still needs the token for the
        // default HELLO it falls back to.
        self.inner.set_auth_token(auth_token.unwrap_or(""));

        self.inner
            .start_with_persisted_state_json(
                url,
                auto_reconnect,
                max_reconnect_attempts,
                state_json,
                auth_token,
            )
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
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
}
