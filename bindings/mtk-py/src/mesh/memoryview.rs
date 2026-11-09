//! Zero-copy typed memoryview and buffer protocol exports for `MeshData`.

use pyo3::prelude::*;
use pyo3::types::PyMemoryView;

use mtk_core::UvLayout;

use super::PyMeshData;
use crate::buffer::{
    create_owned_f32_memoryview, create_owned_i32_memoryview, create_owned_u32_memoryview,
    create_zerocopy_f32_memoryview, create_zerocopy_u16_memoryview, create_zerocopy_u32_memoryview,
};

/// Read-only memoryview of vertex positions (`float32 * 3` per vertex).
pub fn positions_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let ptr = inner.positions.as_ptr() as *const f32;
    let count = inner.positions.len() * 3;
    create_zerocopy_f32_memoryview(py, ptr, count, mesh.as_any())
}

/// Read-only memoryview of vertex normals (`float32 * 3` per vertex).
pub fn normals_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let ptr = inner.normals.as_ptr() as *const f32;
    let count = inner.normals.len() * 3;
    create_zerocopy_f32_memoryview(py, ptr, count, mesh.as_any())
}

/// Read-only memoryview of vertex UV coordinates (`float32 * 2` per vertex).
pub fn uvs_memoryview<'py>(mesh: &Bound<'py, PyMeshData>) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let ptr = inner.uvs.as_ptr() as *const f32;
    let count = inner.uvs.len() * 2;
    create_zerocopy_f32_memoryview(py, ptr, count, mesh.as_any())
}

/// Read-only memoryview of triangle indices (`uint32` per index).
pub fn indices_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let ptr = inner.indices.as_ptr();
    let count = inner.indices.len();
    create_zerocopy_u32_memoryview(py, ptr, count, mesh.as_any())
}

/// Read-only memoryview of quad indices (`uint32` per index) if quads are recorded.
pub fn quad_indices_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if let Some(ref quads) = inner.quad_indices {
        if quads.is_empty() {
            return Ok(None);
        }
        let ptr = quads.as_ptr();
        let count = quads.len();
        Ok(Some(create_zerocopy_u32_memoryview(
            py,
            ptr,
            count,
            mesh.as_any(),
        )?))
    } else if inner.is_quad_topology() {
        let quads = inner.reconstruct_quad_indices();
        if quads.is_empty() {
            Ok(None)
        } else {
            Ok(Some(create_owned_u32_memoryview(py, quads)?))
        }
    } else {
        Ok(None)
    }
}

/// Read-only memoryview of expanded per-loop UV coordinates (`float32 * 2` per loop)
/// aligned directly with Blender's polygon loops.
pub fn loop_uvs_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;

    if inner.loop_count() == 0 {
        return create_zerocopy_f32_memoryview(py, std::ptr::null(), 0, mesh.as_any());
    }

    if inner.uv_layout == UvLayout::PerLoop {
        let uvs_flat = inner.uvs_flat();
        let ptr = uvs_flat.as_ptr();
        let count = uvs_flat.len();
        return create_zerocopy_f32_memoryview(py, ptr, count, mesh.as_any());
    }

    let loop_uvs = py.detach(|| inner.loop_uvs_expanded());
    let mut flat = Vec::with_capacity(loop_uvs.len() * 2);
    for uv in loop_uvs {
        flat.push(uv[0]);
        flat.push(uv[1]);
    }
    create_owned_f32_memoryview(py, flat)
}

/// Read-only memoryview of polygon loop start offsets (`int32` per polygon) for Blender direct topology injection.
pub fn loop_starts_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let starts: Vec<i32> = inner.loop_starts().into_iter().map(|v| v as i32).collect();
    create_owned_i32_memoryview(py, starts)
}

/// Read-only memoryview of polygon loop totals (`int32` per polygon: 4 for quads, 3 for triangles).
pub fn loop_totals_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let totals: Vec<i32> = inner.loop_totals().into_iter().map(|v| v as i32).collect();
    create_owned_i32_memoryview(py, totals)
}

/// Read-only memoryview of face material slots (`uint16` per face).
pub fn face_materials_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let ptr = inner.face_materials.as_ptr();
    let count = inner.face_materials.len();
    create_zerocopy_u16_memoryview(py, ptr, count, mesh.as_any())
}

/// Read-only memoryview of vertex RGBA colors (`float32 * 4` per vertex) if present.
pub fn colors_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if let Some(ref cols) = inner.colors {
        if cols.is_empty() {
            return Ok(None);
        }
        let ptr = cols.as_ptr() as *const f32;
        let count = cols.len() * 4;
        Ok(Some(create_zerocopy_f32_memoryview(
            py,
            ptr,
            count,
            mesh.as_any(),
        )?))
    } else {
        Ok(None)
    }
}
