//! # `mtk-py` Voxel Point Cloud Binding
//!
//! Exposes `VoxelPointCloud` high-throughput unculled 3D voxel point cloud storage,
//! zero-copy memoryviews, bidirectional conversions with `MeshData`, and `VoxelStorage`.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyMemoryView};

use mtk_voxel::storage::VoxelPointCloud;

use crate::mesh::PyMeshData;
use crate::voxel::PyVoxelStorage;

/// Python wrapper for `VoxelPointCloud` flat 3D voxel point cloud container.
#[pyclass(name = "VoxelPointCloud")]
#[derive(Debug, Clone, Default)]
pub struct PyVoxelPointCloud {
    pub(crate) inner: VoxelPointCloud,
}

enum BorrowedF32Buffer {
    Py(pyo3::buffer::PyBuffer<f32>),
    Owned(Vec<f32>),
}

impl BorrowedF32Buffer {
    fn extract(py_obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = pyo3::buffer::PyBuffer::<f32>::get(py_obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        let vec = py_obj.extract::<Vec<f32>>()?;
        Ok(Self::Owned(vec))
    }

    fn as_slice(&self) -> &[f32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const f32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

enum BorrowedI32Buffer {
    Py(pyo3::buffer::PyBuffer<i32>),
    Owned(Vec<i32>),
}

impl BorrowedI32Buffer {
    fn extract(py_obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = pyo3::buffer::PyBuffer::<i32>::get(py_obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        let vec = py_obj.extract::<Vec<i32>>()?;
        Ok(Self::Owned(vec))
    }

    fn as_slice(&self) -> &[i32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const i32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

#[pymethods]
impl PyVoxelPointCloud {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: VoxelPointCloud::new(),
        }
    }

    /// Returns the total number of voxel points stored in this cloud.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Checks if the point cloud contains no voxel points.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Appends a single voxel point.
    #[pyo3(signature = (pos, block_pos, state, biome=None, light=None))]
    pub fn push(
        &mut self,
        pos: [f32; 3],
        block_pos: [i32; 3],
        state: String,
        biome: Option<String>,
        light: Option<u8>,
    ) {
        self.inner.push(
            pos,
            block_pos,
            state,
            biome.unwrap_or_else(|| "minecraft:plains".to_string()),
            light.unwrap_or(0),
        );
    }

    /// Converts this `VoxelPointCloud` into a pure-point `MeshData` with standard
    /// attributes attached on the `Point` domain (without edges or polygons).
    pub fn to_mesh_data(&self) -> PyMeshData {
        PyMeshData::from_mesh(self.inner.to_mesh_data())
    }

    /// Reconstructs a `VoxelPointCloud` from a `MeshData` containing Point-domain attributes.
    #[staticmethod]
    pub fn from_mesh_data(mesh: &PyMeshData) -> PyResult<Self> {
        let cloud = VoxelPointCloud::from_mesh_data(&mesh.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner: cloud })
    }

    /// Reconstructs a `VoxelStorage` from this point cloud.
    pub fn to_storage(&self) -> PyVoxelStorage {
        PyVoxelStorage {
            inner: self.inner.reconstruct_storage(),
        }
    }

    /// Returns optional bounding box `[min_x, min_y, min_z, size_x, size_y, size_z]`.
    #[getter]
    pub fn bounds(&self) -> Option<[i32; 6]> {
        self.inner.bounds
    }

    #[setter]
    pub fn set_bounds(&mut self, bounds: Option<[i32; 6]>) {
        self.inner.bounds = bounds;
    }

    /// High-performance batch constructor from palette indices using buffer protocol / memoryview.
    #[staticmethod]
    #[pyo3(signature = (positions, block_x, block_y, block_z, state_indices, state_palette, biome_indices=None, biome_palette=None, light_levels=None, bounds=None, entity_indices=None, entity_palette=None))]
    pub fn from_indexed_arrays(
        positions: &Bound<'_, PyAny>,
        block_x: &Bound<'_, PyAny>,
        block_y: &Bound<'_, PyAny>,
        block_z: &Bound<'_, PyAny>,
        state_indices: &Bound<'_, PyAny>,
        state_palette: Vec<String>,
        biome_indices: Option<&Bound<'_, PyAny>>,
        biome_palette: Option<Vec<String>>,
        light_levels: Option<Vec<u8>>,
        bounds: Option<[i32; 6]>,
        entity_indices: Option<&Bound<'_, PyAny>>,
        entity_palette: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let pos_buf = BorrowedF32Buffer::extract(positions)?;
        let bx_buf = BorrowedI32Buffer::extract(block_x)?;
        let by_buf = BorrowedI32Buffer::extract(block_y)?;
        let bz_buf = BorrowedI32Buffer::extract(block_z)?;
        let st_buf = BorrowedI32Buffer::extract(state_indices)?;

        let bm_buf = match biome_indices {
            Some(obj) => Some(BorrowedI32Buffer::extract(obj)?),
            None => None,
        };

        let ent_buf = match entity_indices {
            Some(obj) => Some(BorrowedI32Buffer::extract(obj)?),
            None => None,
        };

        let pos_slice = pos_buf.as_slice();
        let bx_slice = bx_buf.as_slice();
        let by_slice = by_buf.as_slice();
        let bz_slice = bz_buf.as_slice();
        let st_slice = st_buf.as_slice();
        let bm_slice = bm_buf.as_ref().map(|b| b.as_slice());
        let ent_slice = ent_buf.as_ref().map(|b| b.as_slice());

        let cloud = VoxelPointCloud::from_indexed_arrays(
            pos_slice.to_vec(),
            bx_slice.to_vec(),
            by_slice.to_vec(),
            bz_slice.to_vec(),
            st_slice,
            &state_palette,
            bm_slice,
            biome_palette.as_deref(),
            light_levels,
            bounds,
            ent_slice,
            entity_palette.as_deref(),
        )
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        Ok(Self { inner: cloud })
    }

    /// Read-only memoryview of 3D vertex positions as raw bytes (`float32 * 3` per point).
    pub fn positions_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.positions.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.positions.as_ptr() as *const u8;
        let len = inner.positions.len() * std::mem::size_of::<f32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Read-only memoryview of native integer X block coordinates (`int32` per point).
    pub fn block_x_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.block_x.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.block_x.as_ptr() as *const u8;
        let len = inner.block_x.len() * std::mem::size_of::<i32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Read-only memoryview of native integer Y block coordinates (`int32` per point).
    pub fn block_y_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.block_y.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.block_y.as_ptr() as *const u8;
        let len = inner.block_y.len() * std::mem::size_of::<i32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Read-only memoryview of native integer Z block coordinates (`int32` per point).
    pub fn block_z_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.block_z.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.block_z.as_ptr() as *const u8;
        let len = inner.block_z.len() * std::mem::size_of::<i32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Returns the unique canonical blockstate palette.
    pub fn get_state_palette(&self) -> Vec<String> {
        self.inner.state_palette.clone()
    }

    /// Returns the unique canonical biome palette.
    pub fn get_biome_palette(&self) -> Vec<String> {
        self.inner.biome_palette.clone()
    }

    /// Checks if this point cloud carries any block entities.
    pub fn has_entities(&self) -> bool {
        self.inner.has_entities()
    }

    /// Returns the unique canonical block entity (Visual NBT JSON) palette.
    pub fn get_entity_palette(&self) -> Vec<String> {
        self.inner.entity_palette.clone()
    }

    /// Read-only memoryview of per-point block entity palette indices (`int32`, -1 for none).
    pub fn entity_indices_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.entity_indices.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.entity_indices.as_ptr() as *const u8;
        let len = inner.entity_indices.len() * std::mem::size_of::<i32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Read-only memoryview of per-point light levels (`uint8`).
    pub fn light_levels_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.light_levels.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.light_levels.as_ptr();
        let len = inner.light_levels.len();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Read-only memoryview of per-point blockstate palette indices (`int32`).
    pub fn state_indices_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.state_indices.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.state_indices.as_ptr() as *const u8;
        let len = inner.state_indices.len() * std::mem::size_of::<i32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Read-only memoryview of per-point biome palette indices (`int32`).
    pub fn biome_indices_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Bound<'py, PyMemoryView>> {
        let py = slf.py();
        let inner = &slf.borrow().inner;
        if inner.biome_indices.is_empty() {
            let bytes = PyBytes::new(py, &[]);
            return PyMemoryView::from(&bytes);
        }
        let ptr = inner.biome_indices.as_ptr() as *const u8;
        let len = inner.biome_indices.len() * std::mem::size_of::<i32>();
        crate::buffer::create_zerocopy_memoryview(py, ptr, len, slf.as_any())
    }

    /// Exports all point cloud arrays and palettes as a host-agnostic dictionary
    /// of memoryviews and lists, ideal for headless processing.
    pub fn to_buffers<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let py = slf.py();
        let this = slf.borrow();
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("count", this.len())?;
        dict.set_item("positions", Self::positions_memoryview(slf)?)?;
        dict.set_item("block_x", Self::block_x_memoryview(slf)?)?;
        dict.set_item("block_y", Self::block_y_memoryview(slf)?)?;
        dict.set_item("block_z", Self::block_z_memoryview(slf)?)?;
        dict.set_item("state_indices", Self::state_indices_memoryview(slf)?)?;
        dict.set_item("state_palette", this.get_state_palette())?;
        dict.set_item("biome_indices", Self::biome_indices_memoryview(slf)?)?;
        dict.set_item("biome_palette", this.get_biome_palette())?;
        if this.has_entities() {
            if let Ok(ent_mv) = Self::entity_indices_memoryview(slf) {
                dict.set_item("entity_indices", ent_mv)?;
                dict.set_item("entity_palette", this.get_entity_palette())?;
            }
        }
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!("<VoxelPointCloud len={}>", self.inner.len())
    }
}
