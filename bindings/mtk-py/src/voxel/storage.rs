//! VoxelStorage container Python bindings.

//! # `mtk-py` Voxel Storage and Configuration Binding
//!
//! Exposes `VoxelStorage` 3D world data containers and `MesherConfig` to Python.

use pyo3::prelude::*;

use mtk_voxel::storage::VoxelStorage;

use super::config::PyMesherConfig;
use super::map_voxel_err;

/// Python wrapper for `VoxelStorage` sparse 3D chunk voxel container.
#[pyclass(name = "VoxelStorage")]
#[derive(Debug, Clone)]
pub struct PyVoxelStorage {
    pub(crate) inner: VoxelStorage,
}

impl Default for PyVoxelStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[pymethods]
impl PyVoxelStorage {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: VoxelStorage::new(),
        }
    }

    /// Pure-code generation of a canonical Minecraft debug world.
    /// Can take an optional list of blockstates, a ResourcePackStack, or an unpack directory path.
    #[staticmethod]
    #[pyo3(signature = (states=None, stack=None, path=None))]
    pub fn create_debug_world(
        states: Option<Vec<String>>,
        stack: Option<&crate::resource::PyResourcePackStack>,
        path: Option<&str>,
    ) -> PyResult<Self> {
        let storage = if let Some(st) = states {
            VoxelStorage::create_debug_world_from_states(&st)
        } else if let Some(s) = stack {
            VoxelStorage::create_debug_world_from_pack_stack(&s.inner)
        } else if let Some(p) = path {
            VoxelStorage::create_debug_world_from_dir(p)
        } else {
            VoxelStorage::create_debug_world()
        }
        .map_err(map_voxel_err)?;

        Ok(Self { inner: storage })
    }

    /// Pure-code generation from an explicit slice of blockstate strings.
    #[staticmethod]
    pub fn create_debug_world_from_states(states: Vec<String>) -> PyResult<Self> {
        let storage =
            VoxelStorage::create_debug_world_from_states(&states).map_err(map_voxel_err)?;
        Ok(Self { inner: storage })
    }

    /// Pure-code generation by enumerating blockstates across a ResourcePackStack.
    #[staticmethod]
    pub fn create_debug_world_from_pack_stack(
        stack: &crate::resource::PyResourcePackStack,
    ) -> PyResult<Self> {
        let storage = VoxelStorage::create_debug_world_from_pack_stack(&stack.inner)
            .map_err(map_voxel_err)?;
        Ok(Self { inner: storage })
    }

    /// Clears the internally cached debug storage (useful during live pack reload or tests).
    #[staticmethod]
    pub fn clear_debug_world_cache() {
        VoxelStorage::clear_debug_world_cache();
    }

    /// Sets the active 3D selection bounding box with incremental section pruning.
    pub fn set_bounds(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
    ) -> bool {
        self.inner
            .set_bounds(min_x, min_y, min_z, size_x, size_y, size_z)
    }

    /// Gets the blockstate identifier string at world coordinate `(x, y, z)`.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> &str {
        self.inner.get_block(x, y, z)
    }

    /// Sets the blockstate at world coordinate `(x, y, z)` and marks dirty sections.
    #[pyo3(signature = (x, y, z, state, biome=None))]
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state: &str, biome: Option<&str>) -> bool {
        self.inner.set_block(x, y, z, state, biome)
    }

    #[getter]
    pub fn min_x(&self) -> i32 {
        self.inner.min_x
    }

    #[getter]
    pub fn min_y(&self) -> i32 {
        self.inner.min_y
    }

    #[getter]
    pub fn min_z(&self) -> i32 {
        self.inner.min_z
    }

    #[getter]
    pub fn size_x(&self) -> i32 {
        self.inner.size_x
    }

    #[getter]
    pub fn size_y(&self) -> i32 {
        self.inner.size_y
    }

    #[getter]
    pub fn size_z(&self) -> i32 {
        self.inner.size_z
    }

    /// Checks if world coordinate `(x, y, z)` falls within the active selection bounds.
    pub fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        self.inner.contains(x, y, z)
    }

    /// Returns the biome registry identifier for block coordinate `(x, y, z)`.
    pub fn get_biome(&self, x: i32, y: i32, z: i32) -> &str {
        self.inner.get_biome(x, y, z)
    }

    /// Extracts an unculled point cloud containing every populated voxel point.
    #[pyo3(signature = (config=None))]
    pub fn to_point_cloud(
        &self,
        config: Option<&PyMesherConfig>,
    ) -> crate::point_cloud::PyVoxelPointCloud {
        let mesher_cfg = config.map(|c| c.inner.clone()).unwrap_or_default();
        crate::point_cloud::PyVoxelPointCloud {
            inner: self.inner.to_point_cloud(&mesher_cfg),
        }
    }

    /// Reconstructs a `VoxelStorage` from an unculled point cloud.
    #[staticmethod]
    pub fn from_point_cloud(point_cloud: &crate::point_cloud::PyVoxelPointCloud) -> Self {
        Self {
            inner: VoxelStorage::from_point_cloud(&point_cloud.inner),
        }
    }

    /// Computes the authoritative sparse state hash (xxHash64) for this storage.
    #[pyo3(signature = (min_pos=(0, 0, 0), size=(0, 0, 0), dimension="minecraft:overworld"))]
    pub fn compute_sparse_state_hash(
        &self,
        min_pos: (i32, i32, i32),
        size: (i32, i32, i32),
        dimension: &str,
    ) -> u64 {
        let (min_pos_v, size_v) = if size == (0, 0, 0) {
            (
                glam::IVec3::new(self.inner.min_x, self.inner.min_y, self.inner.min_z),
                glam::IVec3::new(self.inner.size_x, self.inner.size_y, self.inner.size_z),
            )
        } else {
            (
                glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
                glam::IVec3::new(size.0, size.1, size.2),
            )
        };

        mtk_sync::protocol::hasher::compute_sparse_state_hash_for_storage(
            &self.inner,
            min_pos_v,
            size_v,
            dimension,
        )
    }

    fn __repr__(&self) -> String {
        format!(
            "<VoxelStorage bounds=({}, {}, {}, size={}x{}x{}) sections={} dirty={}>",
            self.inner.min_x,
            self.inner.min_y,
            self.inner.min_z,
            self.inner.size_x,
            self.inner.size_y,
            self.inner.size_z,
            self.inner.sections.len(),
            self.inner.dirty_sections.len()
        )
    }
}
