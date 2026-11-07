//! # Python Bindings for Mesh Topology Analysis
//!
//! Exposes zero-copy hard/sharp edge analysis and dihedral angle computations
//! to Python and Blender via NumPy buffer protocol.
//!
//! ## Unsafe invariant
//! The `get_cow_slice_*` helpers may borrow memory directly from a Python buffer.
//! That is only sound because `is_c_contiguous()` guarantees a single densely packed
//! block, and the returned `CowSlice<'py, _>` is always consumed while the originating
//! `Bound<'py, PyAny>` (and therefore the exporter object) is kept alive for the
//! GIL-bound `'py` lifetime. Callers must not let the slice outlive `obj`.

use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub enum CowSlice<T> {
    PyF32(PyBuffer<f32>),
    PyU32(PyBuffer<u32>),
    PyI32(PyBuffer<i32>),
    PyU8(PyBuffer<u8>),
    PyI8(PyBuffer<i8>),
    Owned(Vec<T>),
}

impl<T> CowSlice<T> {
    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::PyF32(b) => unsafe {
                std::slice::from_raw_parts(b.buf_ptr() as *const T, b.item_count())
            },
            Self::PyU32(b) => unsafe {
                std::slice::from_raw_parts(b.buf_ptr() as *const T, b.item_count())
            },
            Self::PyI32(b) => unsafe {
                std::slice::from_raw_parts(b.buf_ptr() as *const T, b.item_count())
            },
            Self::PyU8(b) => unsafe {
                std::slice::from_raw_parts(b.buf_ptr() as *const T, b.item_count())
            },
            Self::PyI8(b) => unsafe {
                std::slice::from_raw_parts(b.buf_ptr() as *const T, b.item_count())
            },
            Self::Owned(v) => v.as_slice(),
        }
    }
}

impl<T> std::ops::Deref for CowSlice<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

pub fn get_cow_slice_f32(obj: &Bound<'_, PyAny>) -> PyResult<CowSlice<f32>> {
    if let Ok(buf) = PyBuffer::<f32>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyF32(buf));
        }
    }
    let vec: Vec<f32> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

pub fn get_cow_slice_u32(obj: &Bound<'_, PyAny>) -> PyResult<CowSlice<u32>> {
    if let Ok(buf) = PyBuffer::<u32>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyU32(buf));
        }
    }
    if let Ok(buf) = PyBuffer::<i32>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyI32(buf));
        }
    }
    let vec: Vec<u32> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

pub fn get_cow_slice_i32(obj: &Bound<'_, PyAny>) -> PyResult<CowSlice<i32>> {
    if let Ok(buf) = PyBuffer::<i32>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyI32(buf));
        }
    }
    if let Ok(buf) = PyBuffer::<u32>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyU32(buf));
        }
    }
    let vec: Vec<i32> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

pub fn get_cow_slice_u8(obj: &Bound<'_, PyAny>) -> PyResult<CowSlice<u8>> {
    if let Ok(buf) = PyBuffer::<u8>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyU8(buf));
        }
    }
    if let Ok(buf) = PyBuffer::<i8>::get(obj) {
        if buf.is_c_contiguous() {
            return Ok(CowSlice::PyI8(buf));
        }
    }
    let vec: Vec<u8> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

/// Identifies hard and sharp edges based on dihedral face angles and open boundaries.
///
/// Returns a `PyBytes` buffer of length `num_edges`, with `1` for hard/sharp and `0` for smooth.
/// Supports zero-copy numpy buffers directly from Blender `foreach_get`.
#[pyfunction]
#[pyo3(signature = (positions, edge_verts, face_loop_starts, face_loop_totals, loop_verts, sharp_angle_rad, explicit_sharp_edges=None))]
pub fn analyze_mesh_hard_edges<'py>(
    py: Python<'py>,
    positions: &Bound<'py, PyAny>,
    edge_verts: &Bound<'py, PyAny>,
    face_loop_starts: &Bound<'py, PyAny>,
    face_loop_totals: &Bound<'py, PyAny>,
    loop_verts: &Bound<'py, PyAny>,
    sharp_angle_rad: f32,
    explicit_sharp_edges: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let pos_slice = get_cow_slice_f32(positions)?;
    let edges_slice = get_cow_slice_u32(edge_verts)?;
    let starts_slice = get_cow_slice_u32(face_loop_starts)?;
    let totals_slice = get_cow_slice_u32(face_loop_totals)?;
    let loops_slice = get_cow_slice_u32(loop_verts)?;

    let explicit_slice = if let Some(exp) = explicit_sharp_edges {
        Some(get_cow_slice_u8(exp)?)
    } else {
        None
    };

    let mask = py.detach(|| {
        mtk_core::analyze_hard_edges(
            &pos_slice,
            &edges_slice,
            &starts_slice,
            &totals_slice,
            &loops_slice,
            sharp_angle_rad,
            explicit_slice.as_deref(),
        )
    });

    Ok(PyBytes::new(py, &mask))
}
