use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::parse_uuid_hex;

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
        let server = mtk_sync::LiveSyncServer::start(config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{:?}", e)))?;
        Ok(Self {
            inner: Some(server),
        })
    }

    /// Stops the server.
    pub fn stop(&mut self) {
        if let Some(mut server) = self.inner.take() {
            server.stop();
        }
    }

    /// Whether the server is actively running.
    pub fn is_running(&self) -> bool {
        self.inner.as_ref().map(|s| s.is_running()).unwrap_or(false)
    }

    /// Bound local address string (e.g. "127.0.0.1:8765").
    pub fn local_addr(&self) -> Option<String> {
        self.inner
            .as_ref()
            .and_then(|s| s.local_addr().map(|a| a.to_string()))
    }

    /// Number of connected clients.
    #[getter]
    pub fn client_count(&self) -> usize {
        self.inner.as_ref().map(|s| s.client_count()).unwrap_or(0)
    }

    /// Server instance UUID as hex string.
    #[getter]
    pub fn server_uuid(&self) -> String {
        self.inner
            .as_ref()
            .map(|s| {
                let u = s.server_uuid();
                let mut hex = String::with_capacity(32);
                for b in u {
                    use std::fmt::Write;
                    let _ = write!(hex, "{:02x}", b);
                }
                hex
            })
            .unwrap_or_default()
    }

    /// Returns the combined set of subscribed visual entity types from all connected clients.
    pub fn subscribed_visual_entities(&self) -> PyResult<Vec<String>> {
        let server = self
            .inner
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Server is stopped"))?;
        Ok(server.subscribed_visual_entities())
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
                &[],
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
