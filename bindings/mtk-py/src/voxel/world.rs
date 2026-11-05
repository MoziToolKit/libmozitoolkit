//! VoxelWorld Python bindings.

use super::config::PyMesherConfig;
use super::map_voxel_err;
use super::storage::PyVoxelStorage;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Python wrapper for `VoxelWorld` 3D scene engine.
#[pyclass(name = "VoxelWorld")]
#[derive(Clone)]
pub struct PyVoxelWorld {
    pub(crate) inner: mtk_voxel::VoxelWorld,
}

#[pymethods]
impl PyVoxelWorld {
    #[new]
    #[pyo3(signature = (config=None, culler=None, model_db=None))]
    pub fn new(
        config: Option<&PyMesherConfig>,
        culler: Option<&crate::cull::PyFaceCuller>,
        model_db: Option<&crate::model::PyBakedModelDatabase>,
    ) -> Self {
        let cfg = config.map(|c| c.inner.clone());
        let cul = culler.map(|c| c.inner.clone());
        let mdb = model_db.map(|db| db.inner.clone());
        Self {
            inner: mtk_voxel::VoxelWorld::new(cfg, cul, mdb),
        }
    }

    /// Creates a `VoxelWorld` taking initial data from an existing `VoxelStorage`.
    #[staticmethod]
    #[pyo3(signature = (storage, config=None, culler=None, model_db=None, num_threads=None))]
    pub fn from_storage(
        storage: &PyVoxelStorage,
        config: Option<&PyMesherConfig>,
        culler: Option<&crate::cull::PyFaceCuller>,
        model_db: Option<&crate::model::PyBakedModelDatabase>,
        num_threads: Option<usize>,
    ) -> Self {
        let cfg = config.map(|c| c.inner.clone());
        let cul = culler.map(|c| c.inner.clone());
        let mdb = model_db.map(|db| db.inner.clone());
        let threads = num_threads.or_else(|| config.and_then(|c| c.num_threads));
        Self {
            inner: mtk_voxel::VoxelWorld::from_storage_with_threads(
                storage.inner.clone(),
                cfg,
                cul,
                mdb,
                threads,
            ),
        }
    }

    /// Loads or generates the canonical Minecraft debug world into a new `VoxelWorld`.
    #[staticmethod]
    #[pyo3(signature = (config=None, culler=None, model_db=None, num_threads=None, states=None, stack=None))]
    pub fn create_debug_world(
        config: Option<&PyMesherConfig>,
        culler: Option<&crate::cull::PyFaceCuller>,
        model_db: Option<&crate::model::PyBakedModelDatabase>,
        num_threads: Option<usize>,
        states: Option<Vec<String>>,
        stack: Option<&crate::resource::PyResourcePackStack>,
    ) -> PyResult<Self> {
        let cfg = config.map(|c| c.inner.clone());
        let cul = culler.map(|c| c.inner.clone());
        let mdb = model_db.map(|db| db.inner.clone());
        let threads = num_threads.or_else(|| config.and_then(|c| c.num_threads));

        let world = if let Some(st) = states {
            mtk_voxel::VoxelWorld::create_debug_world_from_states(&st, cfg, cul, mdb, threads)
        } else if let Some(s) = stack {
            mtk_voxel::VoxelWorld::create_debug_world_from_pack_stack(
                &s.inner, cfg, cul, mdb, threads,
            )
        } else {
            mtk_voxel::VoxelWorld::create_debug_world_with_threads(cfg, cul, mdb, threads)
        }
        .map_err(map_voxel_err)?;

        Ok(Self { inner: world })
    }

    /// Gets the number of worker threads configured for meshing.
    #[getter]
    pub fn num_threads(&self) -> Option<usize> {
        self.inner.num_threads()
    }

    /// Re-meshes all non-empty sections in parallel and returns the unified `MeshData`.
    #[pyo3(signature = (callback=None))]
    pub fn rebuild_all(
        &mut self,
        py: Python<'_>,
        callback: Option<Py<PyAny>>,
    ) -> PyResult<crate::mesh::PyMeshData> {
        let mesh = if let Some(cb) = callback {
            let cb_ref = &cb;
            py.detach(|| {
                self.inner
                    .rebuild_all_with_progress(Some(&|prog| {
                        Python::attach(|py| {
                            let dict = pyo3::types::PyDict::new(py);
                            let _ = dict.set_item("stage", prog.stage);
                            let _ = dict.set_item("current", prog.current);
                            let _ = dict.set_item("total", prog.total);
                            let _ = dict.set_item("message", &prog.message);
                            let _ = dict.set_item("percent", prog.percent());
                            let _ = cb_ref.call1(py, (dict,));
                        });
                    }))
                    .cloned()
            })
        } else {
            py.detach(|| self.inner.rebuild_all().cloned())
        }
        .map_err(map_voxel_err)?;
        let profile = self.inner.last_profile.clone();
        let (dynamic_entity_atlases, dynamic_entity_slots) =
            self.inner.dynamic_entity_atlases_if_used();
        Ok(crate::mesh::PyMeshData {
            inner: mesh,
            profile,
            dynamic_entity_atlases,
            dynamic_entity_slots,
        })
    }

    /// Execution profile of the most recent rebuild operation.
    #[getter]
    pub fn last_profile(&self) -> Option<crate::profile::PyExecutionProfile> {
        self.inner.last_profile.clone().map(Into::into)
    }

    /// Incrementally rebuilds only the modified/dirty sections and returns a dict mapping (sx, sy, sz) to PyMeshData.
    pub fn rebuild_dirty<'py>(
        &mut self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let rebuilt = py
            .detach(|| self.inner.rebuild_dirty())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let dict = pyo3::types::PyDict::new(py);
        let (dyn_atlases, dyn_slots) = self.inner.dynamic_entity_atlases_if_used();
        for (coord, mesh) in rebuilt {
            let mut py_mesh = crate::mesh::PyMeshData::from_mesh(mesh);
            py_mesh.dynamic_entity_atlases = dyn_atlases.clone();
            py_mesh.dynamic_entity_slots = dyn_slots.clone();
            dict.set_item((coord.x, coord.y, coord.z), py_mesh)?;
        }
        Ok(dict)
    }

    /// Returns the current full merged world geometry directly as a PyMeshData buffer.
    pub fn get_world_mesh(&self) -> crate::mesh::PyMeshData {
        let mesh = self.inner.get_world_mesh().cloned().unwrap_or_default();
        let profile = self.inner.last_profile.clone();
        let (dynamic_entity_atlases, dynamic_entity_slots) =
            self.inner.dynamic_entity_atlases_if_used();
        crate::mesh::PyMeshData {
            inner: mesh,
            profile,
            dynamic_entity_atlases,
            dynamic_entity_slots,
        }
    }

    /// Returns the list of unique Atlas Chunk IDs used by the active world mesh.
    pub fn used_chunk_ids(&self) -> Vec<u32> {
        self.inner.used_chunk_ids().to_vec()
    }

    /// Updates mesher configuration.
    pub fn set_config(&mut self, config: &PyMesherConfig) {
        self.inner.set_config(config.inner.clone());
    }

    /// Returns a copy of the underlying VoxelStorage.
    pub fn get_storage(&self) -> PyVoxelStorage {
        PyVoxelStorage {
            inner: self.inner.storage.clone(),
        }
    }

    /// Extracts an unculled point cloud representation of the current world geometry.
    pub fn to_point_cloud(&self) -> crate::point_cloud::PyVoxelPointCloud {
        crate::point_cloud::PyVoxelPointCloud {
            inner: self.inner.to_point_cloud(),
        }
    }

    /// Syncs world storage with updated storage (e.g. from carved point cloud)
    /// and marks only sections with CRC checksum differences as dirty.
    pub fn sync_from_storage(&mut self, new_storage: &PyVoxelStorage) -> usize {
        self.inner.sync_from_storage(&new_storage.inner)
    }

    /// Clears the cached section meshes and unified world mesh, marking all storage sections as dirty.
    pub fn clear_cache(&mut self) {
        self.inner.clear_cache();
    }

    /// Hot-reloads mesher configuration and model database, clearing all cached section meshes.
    #[pyo3(signature = (config=None, model_db=None))]
    pub fn hot_reload(
        &mut self,
        config: Option<&PyMesherConfig>,
        model_db: Option<&crate::model::PyBakedModelDatabase>,
    ) {
        if let Some(cfg) = config {
            self.inner.set_config(cfg.inner.clone());
        }
        if let Some(mdb) = model_db {
            self.inner.set_model_db(Some(mdb.inner.clone()));
        }
        self.inner.clear_cache();
    }

    /// Returns a dict mapping Chunk ID (e.g. 0xFFF0, 0xFFF1) to dynamic entity atlas RGBA bytes (1024x1024x4).
    pub fn get_dynamic_entity_atlases<'py>(
        &self,
        py: Python<'py>,
    ) -> Bound<'py, pyo3::types::PyDict> {
        let dict = pyo3::types::PyDict::new(py);
        for (chunk_id, buf) in self.inner.dynamic_entity_atlases() {
            let _ = dict.set_item(chunk_id, PyBytes::new(py, buf));
        }
        dict
    }

    /// Returns the dynamic entity key to (chunk_id, slot_index) mapping.
    pub fn get_dynamic_entity_slots(&self) -> std::collections::HashMap<String, (u16, u16)> {
        self.inner.dynamic_entity_slots()
    }

    /// Sets the on-disk cache directory for downloaded player skins.
    pub fn set_skin_cache_dir(&mut self, cache_dir: Option<String>) {
        self.inner
            .set_skin_cache_dir(cache_dir.map(std::path::PathBuf::from));
    }
}
