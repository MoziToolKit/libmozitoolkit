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

/// Encodes an existing VoxelStorage into a progressive RFC v3.1 baseline packet stream.
/// Returns a tuple `(packets_bytes_list, state_hash, total_chunks)`.
#[pyfunction]
#[pyo3(signature = (
    storage,
    min_pos,
    size,
    dimension="minecraft:overworld",
    epoch=1,
    snapshot_uuid=None,
    snapshot_seq=1,
    palette_revision=1,
    max_chunk_size=262144,
    compression_mode=1,
    compression_level=3
))]
pub fn encode_baseline_from_storage<'py>(
    py: Python<'py>,
    storage: &PyVoxelStorage,
    min_pos: (i32, i32, i32),
    size: (i32, i32, i32),
    dimension: &str,
    epoch: u64,
    snapshot_uuid: Option<&[u8]>,
    snapshot_seq: u64,
    palette_revision: u32,
    max_chunk_size: usize,
    compression_mode: u8,
    compression_level: i32,
) -> PyResult<(Bound<'py, PyList>, u64, u32)> {
    let mut uuid_arr = [0u8; 16];
    if let Some(raw_uuid) = snapshot_uuid {
        if raw_uuid.len() >= 16 {
            uuid_arr.copy_from_slice(&raw_uuid[..16]);
        }
    }

    let config = mtk_sync::BaselineEncoderConfig {
        max_chunk_size,
        compression_mode,
        compression_level,
        epoch,
        snapshot_uuid: uuid_arr,
        snapshot_seq,
        palette_revision,
    };

    let stream = mtk_sync::encode_baseline_from_storage(
        &storage.inner,
        glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
        glam::IVec3::new(size.0, size.1, size.2),
        dimension,
        &[],
        &config,
    )
    .map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Baseline encode failed: {}", e))
    })?;

    let py_list = PyList::empty(py);
    for pkt in &stream.packets {
        let wire_bytes = mtk_sync::encode_packet(pkt);
        py_list.append(PyBytes::new(py, &wire_bytes))?;
    }

    Ok((py_list, stream.state_hash, stream.total_chunks))
}

/// Evaluates a client HELLO frame against the server context and produces an RFC WELCOME frame.
/// Returns a tuple `(welcome_packet_bytes, decision_code)`.
#[pyfunction]
#[pyo3(signature = (
    hello_bytes,
    server_uuid,
    epoch,
    sequence,
    min_pos,
    size,
    state_hash,
    dimension="minecraft:overworld",
    expected_auth_token=None
))]
pub fn decide_handshake<'py>(
    py: Python<'py>,
    hello_bytes: &[u8],
    server_uuid: &[u8],
    epoch: u64,
    sequence: u64,
    min_pos: (i32, i32, i32),
    size: (i32, i32, i32),
    state_hash: u64,
    dimension: &str,
    expected_auth_token: Option<&str>,
) -> PyResult<(Bound<'py, PyBytes>, u8)> {
    let hello_packet = mtk_sync::decode_packet(hello_bytes).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Invalid HELLO frame: {}", e))
    })?;

    let mut uuid_arr = [0u8; 16];
    if server_uuid.len() >= 16 {
        uuid_arr.copy_from_slice(&server_uuid[..16]);
    }

    let ctx = mtk_sync::HandshakeContext {
        server_uuid: uuid_arr,
        epoch,
        sequence,
        min_pos: glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
        size: glam::IVec3::new(size.0, size.1, size.2),
        state_hash,
        dimension: dimension.to_string(),
        expected_auth_token: expected_auth_token.map(|s| s.to_string()),
    };

    let result = mtk_sync::decide_handshake(&hello_packet, &ctx).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Handshake decision failed: {}", e))
    })?;

    let welcome_bytes = mtk_sync::encode_packet(&result.welcome_packet);
    let decision_code = result.decision as u8;

    Ok((PyBytes::new(py, &welcome_bytes), decision_code))
}

/// Encodes a list of atomic delta changes into RFC DELTA_OP packet byte frames.
#[pyfunction]
#[pyo3(signature = (epoch, seq_id, palette_revision, changes))]
pub fn encode_delta_op_batch<'py>(
    py: Python<'py>,
    epoch: u64,
    seq_id: u64,
    palette_revision: u32,
    changes: Vec<((i32, i32, i32), u16)>,
) -> PyResult<Bound<'py, PyList>> {
    let delta_entries: Vec<mtk_sync::DeltaChangeEntry> = changes
        .into_iter()
        .map(|((x, y, z), state_id)| mtk_sync::DeltaChangeEntry {
            rel_pos: glam::IVec3::new(x, y, z),
            state_id,
        })
        .collect();

    let packets = mtk_sync::encode_delta_op(epoch, seq_id, palette_revision, delta_entries);

    let py_list = PyList::empty(py);
    for pkt in &packets {
        let wire_bytes = mtk_sync::encode_packet(pkt);
        py_list.append(PyBytes::new(py, &wire_bytes))?;
    }

    Ok(py_list)
}
