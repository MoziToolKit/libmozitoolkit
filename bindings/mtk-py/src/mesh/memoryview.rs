//! Zero-copy memoryview and buffer protocol exports for `MeshData`.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyMemoryView};

use mtk_core::mesh::UvLayout;

use super::PyMeshData;
use crate::buffer::{
    create_owned_f32_memoryview, create_owned_i32_memoryview, create_owned_u32_memoryview,
    create_zerocopy_memoryview,
};

/// Read-only memoryview of vertex positions as raw bytes (`float32 * 3` per vertex).
pub fn positions_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if inner.positions.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    let ptr = inner.positions.as_ptr() as *const u8;
    let len = inner.positions.len() * std::mem::size_of::<[f32; 3]>();
    create_zerocopy_memoryview(py, ptr, len, mesh.as_any())
}

/// Read-only memoryview of vertex normals as raw bytes (`float32 * 3` per vertex).
pub fn normals_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if inner.normals.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    let ptr = inner.normals.as_ptr() as *const u8;
    let len = inner.normals.len() * std::mem::size_of::<[f32; 3]>();
    create_zerocopy_memoryview(py, ptr, len, mesh.as_any())
}

/// Read-only memoryview of vertex UV coordinates as raw bytes (`float32 * 2` per vertex).
pub fn uvs_memoryview<'py>(mesh: &Bound<'py, PyMeshData>) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if inner.uvs.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    let ptr = inner.uvs.as_ptr() as *const u8;
    let len = inner.uvs.len() * std::mem::size_of::<[f32; 2]>();
    create_zerocopy_memoryview(py, ptr, len, mesh.as_any())
}

/// Read-only memoryview of secondary normalized [0, 1] vertex UVs (if generated).
pub fn secondary_uvs_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if let Some(ref sec_uvs) = inner.secondary_uvs {
        if sec_uvs.is_empty() {
            return Ok(None);
        }
        let ptr = sec_uvs.as_ptr() as *const u8;
        let len = sec_uvs.len() * std::mem::size_of::<[f32; 2]>();
        Ok(Some(create_zerocopy_memoryview(
            py,
            ptr,
            len,
            mesh.as_any(),
        )?))
    } else {
        Ok(None)
    }
}

/// Read-only memoryview of triangle indices as raw bytes (`uint32` per index).
pub fn indices_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if inner.indices.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    let ptr = inner.indices.as_ptr() as *const u8;
    let len = inner.indices.len() * std::mem::size_of::<u32>();
    create_zerocopy_memoryview(py, ptr, len, mesh.as_any())
}

/// Read-only memoryview of quad indices as raw bytes (`uint32` per index) if quads are recorded.
pub fn quad_indices_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if let Some(ref quads) = inner.quad_indices {
        if quads.is_empty() {
            return Ok(None);
        }
        let ptr = quads.as_ptr() as *const u8;
        let len = quads.len() * std::mem::size_of::<u32>();
        Ok(Some(create_zerocopy_memoryview(
            py,
            ptr,
            len,
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
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }

    if inner.uv_layout == UvLayout::PerLoop {
        let uvs_flat = inner.uvs_flat();
        let ptr = uvs_flat.as_ptr() as *const u8;
        let len = std::mem::size_of_val(uvs_flat);
        return create_zerocopy_memoryview(py, ptr, len, mesh.as_any());
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
    if starts.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    create_owned_i32_memoryview(py, starts)
}

/// Read-only memoryview of polygon loop totals (`int32` per polygon: 4 for quads, 3 for triangles).
pub fn loop_totals_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    let totals: Vec<i32> = inner.loop_totals().into_iter().map(|v| v as i32).collect();
    if totals.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    create_owned_i32_memoryview(py, totals)
}

/// Read-only memoryview of face material slots (`uint16` per face).
pub fn face_materials_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if inner.face_materials.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    let ptr = inner.face_materials.as_ptr() as *const u8;
    let len = inner.face_materials.len() * std::mem::size_of::<u16>();
    create_zerocopy_memoryview(py, ptr, len, mesh.as_any())
}

/// Read-only memoryview of face tint indices (`int16` per face).
pub fn face_tint_indices_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if inner.face_tint_indices.is_empty() {
        let bytes = PyBytes::new(py, &[]);
        return PyMemoryView::from(&bytes);
    }
    let ptr = inner.face_tint_indices.as_ptr() as *const u8;
    let len =
        inner.face_tint_indices.len() * std::mem::size_of::<mtk_core::attributes::TintIndex>();
    create_zerocopy_memoryview(py, ptr, len, mesh.as_any())
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
        let ptr = cols.as_ptr() as *const u8;
        let len = cols.len() * std::mem::size_of::<[f32; 4]>();
        Ok(Some(create_zerocopy_memoryview(
            py,
            ptr,
            len,
            mesh.as_any(),
        )?))
    } else {
        Ok(None)
    }
}
