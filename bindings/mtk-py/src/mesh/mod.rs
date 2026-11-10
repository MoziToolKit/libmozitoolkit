//! # `mtk-py` Mesh Buffer Binding
//!
//! Exposes contiguous geometry buffers (`MeshData`) to Python with zero-copy
//! buffer protocol and memoryview support for ultrafast Blender vertex injection.

pub mod attributes;
pub mod memoryview;
pub mod topology;

pub use topology::analyze_mesh_hard_edges;

use pyo3::prelude::*;
use pyo3::types::PyMemoryView;

use mtk_core::FlatPolygonMesh;
use mtk_core::MeshData;

/// Python-facing wrapper around contiguous `MeshData`.
#[pyclass(name = "MeshData")]
#[derive(Debug, Clone, Default)]
pub struct PyMeshData {
    pub inner: MeshData,
    pub profile: Option<mtk_core::ExecutionProfile>,
    /// Dynamic entity atlas chunks keyed by chunk id (`{0xFFF0: RGBA bytes, ...}`).
    pub dynamic_entity_atlases: Option<std::collections::HashMap<u16, Vec<u8>>>,
    /// Dynamic entity slots keyed by skin/entity key (`{"player_Alex": (0xFFF0, 0), ...}`).
    pub dynamic_entity_slots: Option<std::collections::HashMap<String, (u16, u16)>>,
}

impl From<MeshData> for PyMeshData {
    fn from(inner: MeshData) -> Self {
        Self {
            inner,
            profile: None,
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
        }
    }
}

impl PyMeshData {
    /// Wraps raw MeshData with no profile.
    pub fn from_mesh(inner: MeshData) -> Self {
        Self {
            inner,
            profile: None,
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
        }
    }

    /// Direct reference to underlying MeshData.
    pub fn inner(&self) -> &MeshData {
        &self.inner
    }

    /// Direct mutable reference to underlying MeshData.
    pub fn inner_mut(&mut self) -> &mut MeshData {
        &mut self.inner
    }

    /// Constructs a `PyMeshData` with an execution profile.
    pub fn with_profile(mut self, profile: mtk_core::ExecutionProfile) -> Self {
        self.profile = Some(profile);
        self
    }
}

#[pymethods]
impl PyMeshData {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: MeshData::new(),
            profile: None,
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
        }
    }

    /// Execution profile of the operation that generated this mesh, if recorded.
    #[getter]
    pub fn profile(&self) -> Option<crate::profile::PyExecutionProfile> {
        self.profile.clone().map(Into::into)
    }

    /// Returns a dict mapping Chunk ID (e.g. 0xFFF0, 0xFFF1) to dynamic entity atlas RGBA bytes.
    /// Returns an empty dict `{}` if no dynamic entity atlases are present.
    #[getter]
    pub fn dynamic_entity_atlases<'py>(&self, py: Python<'py>) -> Bound<'py, pyo3::types::PyDict> {
        let dict = pyo3::types::PyDict::new(py);
        if let Some(ref map) = self.dynamic_entity_atlases {
            for (&chunk_id, bytes) in map {
                let _ = dict.set_item(chunk_id, pyo3::types::PyBytes::new(py, bytes));
            }
        }
        dict
    }

    /// Returns dynamic entity slots mapping if present.
    #[getter]
    pub fn dynamic_entity_slots(&self) -> Option<std::collections::HashMap<String, (u16, u16)>> {
        self.dynamic_entity_slots.clone()
    }

    #[setter]
    pub fn set_dynamic_entity_slots(
        &mut self,
        slots: Option<std::collections::HashMap<String, (u16, u16)>>,
    ) {
        self.dynamic_entity_slots = slots;
    }

    /// Number of vertices in the mesh buffer.
    #[getter]
    pub fn vertex_count(&self) -> usize {
        self.inner.vertex_count()
    }

    /// Number of triangles in the mesh buffer.
    #[getter]
    pub fn triangle_count(&self) -> usize {
        self.inner.triangle_count()
    }

    /// Number of quad faces recorded.
    #[getter]
    pub fn face_count(&self) -> usize {
        self.inner.face_count()
    }

    /// Checks if the mesh is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears all geometry in the mesh.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Constructs a shared-vertex polygon `MeshData` with per-loop UVs.
    ///
    /// Accepts NumPy arrays, typed memoryviews, or Python lists. Can be constructed
    /// either from `(face_loop_starts, face_loop_totals)` or directly from
    /// CSR-style `face_offsets` (length `face_count + 1`).
    #[staticmethod]
    #[pyo3(signature = (positions, loop_vertices, loop_uvs, face_loop_starts=None, face_loop_totals=None, normals=None, face_materials=None, face_offsets=None))]
    pub fn from_flat_polygons<'py>(
        positions: &Bound<'py, PyAny>,
        loop_vertices: &Bound<'py, PyAny>,
        loop_uvs: &Bound<'py, PyAny>,
        face_loop_starts: Option<&Bound<'py, PyAny>>,
        face_loop_totals: Option<&Bound<'py, PyAny>>,
        normals: Option<&Bound<'py, PyAny>>,
        face_materials: Option<&Bound<'py, PyAny>>,
        face_offsets: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let pos_slice = topology::get_cow_slice_f32(positions)?;
        if pos_slice.len() % 3 != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "positions buffer length must be a multiple of 3",
            ));
        }

        let loop_verts_slice = topology::get_cow_slice_u32(loop_vertices)?;

        let loop_uvs_slice = topology::get_cow_slice_f32(loop_uvs)?;
        if loop_uvs_slice.len() % 2 != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "loop_uvs buffer length must be a multiple of 2",
            ));
        }

        let normals_cow = if let Some(n) = normals {
            let s = topology::get_cow_slice_f32(n)?;
            if s.len() % 3 != 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "normals buffer length must be a multiple of 3",
                ));
            }
            Some(s)
        } else {
            None
        };

        let mats_cow = if let Some(m) = face_materials {
            Some(topology::get_cow_slice_u32(m)?)
        } else {
            None
        };
        let mats_vec = mats_cow.map(|m| m.to_vec()).unwrap_or_default();

        let flat = if let Some(offsets_obj) = face_offsets {
            let offsets_slice = topology::get_cow_slice_i32(offsets_obj)?;
            FlatPolygonMesh::from_face_offsets(
                &pos_slice,
                loop_verts_slice.to_vec(),
                &loop_uvs_slice,
                &offsets_slice,
                mats_vec,
            )
            .map_err(pyo3::exceptions::PyValueError::new_err)?
        } else {
            let starts_obj = face_loop_starts.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "either face_offsets or (face_loop_starts, face_loop_totals) must be provided",
                )
            })?;
            let totals_obj = face_loop_totals.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "either face_offsets or (face_loop_starts, face_loop_totals) must be provided",
                )
            })?;
            let starts_slice = topology::get_cow_slice_u32(starts_obj)?;
            let totals_slice = topology::get_cow_slice_u32(totals_obj)?;
            FlatPolygonMesh::from_flat_buffers(
                &pos_slice,
                loop_verts_slice.to_vec(),
                &loop_uvs_slice,
                starts_slice.to_vec(),
                totals_slice.to_vec(),
                mats_vec,
            )
        };

        let normal_vec = normals_cow.map(|values| {
            values
                .as_chunks::<3>()
                .0
                .iter()
                .map(|v| [v[0], v[1], v[2]])
                .collect::<Vec<[f32; 3]>>()
        });
        let mesh = MeshData::from_flat_polygons(&flat, normal_vec.as_deref())
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(Self::from_mesh(mesh))
    }

    /// Constructs a shared-vertex polygon `MeshData` directly from CSR-style `face_offsets` (length `face_count + 1`).
    #[staticmethod]
    #[pyo3(signature = (positions, loop_vertices, loop_uvs, face_offsets, normals=None, face_materials=None))]
    pub fn from_polygon_offsets<'py>(
        positions: &Bound<'py, PyAny>,
        loop_vertices: &Bound<'py, PyAny>,
        loop_uvs: &Bound<'py, PyAny>,
        face_offsets: &Bound<'py, PyAny>,
        normals: Option<&Bound<'py, PyAny>>,
        face_materials: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        Self::from_flat_polygons(
            positions,
            loop_vertices,
            loop_uvs,
            None,
            None,
            normals,
            face_materials,
            Some(face_offsets),
        )
    }

    // -------------------------------------------------------------------------
    // Zero-Copy MemoryView Delegates (implemented in memoryview.rs)
    // -------------------------------------------------------------------------

    /// Read-only memoryview of vertex positions as raw bytes (`float32 * 3` per vertex).
    pub fn positions_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        memoryview::positions_memoryview(slf)
    }

    /// Read-only memoryview of vertex normals as raw bytes (`float32 * 3` per vertex).
    pub fn normals_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        memoryview::normals_memoryview(slf)
    }

    /// Read-only memoryview of vertex UV coordinates as raw bytes (`float32 * 2` per vertex).
    pub fn uvs_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        memoryview::uvs_memoryview(slf)
    }

    /// Read-only memoryview of triangle indices as raw bytes (`uint32` per index).
    pub fn indices_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        memoryview::indices_memoryview(slf)
    }

    /// Read-only memoryview of quad indices as raw bytes (`uint32` per index) if quads are recorded.
    pub fn quad_indices_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
        memoryview::quad_indices_memoryview(slf)
    }

    /// Read-only memoryview of expanded per-loop UV coordinates (`float32 * 2` per loop).
    pub fn loop_uvs_memoryview<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMemoryView>> {
        memoryview::loop_uvs_memoryview(slf)
    }

    /// Read-only memoryview of face material slots (`uint16` per face).
    pub fn face_materials_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Bound<'py, PyMemoryView>> {
        memoryview::face_materials_memoryview(slf)
    }

    /// Read-only memoryview of vertex RGBA colors (`float32 * 4` per vertex) if present.
    pub fn colors_memoryview<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
        memoryview::colors_memoryview(slf)
    }

    /// Number of quad faces recorded.
    #[getter]
    pub fn quad_count(&self) -> usize {
        self.inner.quad_count()
    }

    /// Returns a list of unique material slot IDs present across all faces in this mesh.
    pub fn used_materials(&self) -> Vec<u16> {
        self.inner.used_materials()
    }

    // -------------------------------------------------------------------------
    // Custom Attributes & List Delegates (implemented in attributes.rs)
    // -------------------------------------------------------------------------

    /// Read-only zero-copy memoryview of a numeric custom attribute by name.
    pub fn attribute_memoryview<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
    ) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
        attributes::attribute_memoryview(slf, name)
    }

    /// Add a numeric custom attribute from any Python buffer.
    pub fn add_attribute_from_buffer(
        &mut self,
        name: &str,
        domain: &str,
        dtype: &str,
        buffer: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        attributes::add_attribute_from_buffer(self, name, domain, dtype, buffer)
    }

    /// Add a string custom attribute.
    pub fn add_string_attribute(
        &mut self,
        name: &str,
        domain: &str,
        values: Vec<String>,
    ) -> PyResult<()> {
        attributes::add_string_attribute(self, name, domain, values)
    }

    /// Add a string custom attribute from a compact palette + u32 index buffer.
    /// Retrieve a string custom attribute's values.
    pub fn get_string_attribute(&self, name: &str) -> Option<Vec<String>> {
        attributes::get_string_attribute(self, name)
    }

    /// List all custom attribute names.
    pub fn attribute_names(&self) -> Vec<String> {
        attributes::attribute_names(self)
    }

    /// Get attribute metadata: (domain_name, type_name, element_count).
    pub fn attribute_info(&self, name: &str) -> Option<(String, String, usize)> {
        attributes::attribute_info(self, name)
    }

    /// Exports all contiguous geometric buffers and custom attributes as a host-agnostic
    /// dictionary of memoryviews and metadata, ideal for headless Python batch
    /// processing or standard non-DNA batch ingestion.
    pub fn to_buffers<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let py = slf.py();
        let this = slf.borrow();
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("vertex_count", this.vertex_count())?;
        dict.set_item("triangle_count", this.triangle_count())?;
        dict.set_item("face_count", this.face_count())?;
        dict.set_item("quad_count", this.quad_count())?;
        dict.set_item("positions", Self::positions_memoryview(slf)?)?;
        dict.set_item("indices", Self::indices_memoryview(slf)?)?;
        if !this.inner.normals.is_empty() {
            dict.set_item("normals", Self::normals_memoryview(slf)?)?;
        }
        if !this.inner.uvs.is_empty() {
            dict.set_item("uvs", Self::uvs_memoryview(slf)?)?;
        }
        if let Some(quad_mv) = Self::quad_indices_memoryview(slf)? {
            dict.set_item("quad_indices", quad_mv)?;
        }
        dict.set_item("loop_uvs", Self::loop_uvs_memoryview(slf)?)?;
        dict.set_item("loop_starts", memoryview::loop_starts_memoryview(slf)?)?;
        dict.set_item("loop_totals", memoryview::loop_totals_memoryview(slf)?)?;
        dict.set_item("face_materials", Self::face_materials_memoryview(slf)?)?;
        if let Some(col_mv) = Self::colors_memoryview(slf)? {
            dict.set_item("colors", col_mv)?;
        }

        let attrs_dict = pyo3::types::PyDict::new(py);
        for name in this.attribute_names() {
            if let Some(attr_mv) = Self::attribute_memoryview(slf, &name)? {
                let item = pyo3::types::PyDict::new(py);
                if let Some((domain, dtype, count)) = this.attribute_info(&name) {
                    item.set_item("domain", domain)?;
                    item.set_item("dtype", dtype)?;
                    item.set_item("element_count", count)?;
                }
                item.set_item("data", attr_mv)?;
                attrs_dict.set_item(&name, item)?;
            } else if let Some(str_vals) = this.get_string_attribute(&name) {
                let item = pyo3::types::PyDict::new(py);
                if let Some((domain, dtype, count)) = this.attribute_info(&name) {
                    item.set_item("domain", domain)?;
                    item.set_item("dtype", dtype)?;
                    item.set_item("element_count", count)?;
                }
                item.set_item("data", str_vals.clone())?;
                item.set_item("palette", str_vals)?;
                attrs_dict.set_item(&name, item)?;
            }
        }
        dict.set_item("attributes", attrs_dict)?;

        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "<MeshData vertices={} triangles={} faces={}>",
            self.vertex_count(),
            self.triangle_count(),
            self.face_count()
        )
    }
}
