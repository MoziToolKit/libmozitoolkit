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

pub enum CowSlice<'a, T> {
    Borrowed(&'a [T]),
    Owned(Vec<T>),
}

impl<'a, T> std::ops::Deref for CowSlice<'a, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            CowSlice::Borrowed(s) => s,
            CowSlice::Owned(ref v) => v.as_slice(),
        }
    }
}

pub fn get_cow_slice_f32<'py>(obj: &Bound<'py, PyAny>) -> PyResult<CowSlice<'py, f32>> {
    if let Ok(buf) = PyBuffer::<f32>::get(obj) {
        if buf.is_c_contiguous() {
            // SAFETY: C-contiguous buffer = one densely packed block; `obj` outlives `'py`.
            let slice = unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const f32, buf.item_count())
            };
            return Ok(CowSlice::Borrowed(slice));
        }
    }
    let vec: Vec<f32> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

pub fn get_cow_slice_u32<'py>(obj: &Bound<'py, PyAny>) -> PyResult<CowSlice<'py, u32>> {
    if let Ok(buf) = PyBuffer::<u32>::get(obj) {
        if buf.is_c_contiguous() {
            // SAFETY: C-contiguous buffer = one densely packed block; `obj` outlives `'py`.
            let slice = unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const u32, buf.item_count())
            };
            return Ok(CowSlice::Borrowed(slice));
        }
    }
    if let Ok(buf) = PyBuffer::<i32>::get(obj) {
        if buf.is_c_contiguous() {
            // SAFETY: as above; `i32` and `u32` share size/alignment and every bit pattern
            // is a valid `u32`, so the reinterpretation is well-defined.
            let slice = unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const u32, buf.item_count())
            };
            return Ok(CowSlice::Borrowed(slice));
        }
    }
    let vec: Vec<u32> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

pub fn get_cow_slice_i32<'py>(obj: &Bound<'py, PyAny>) -> PyResult<CowSlice<'py, i32>> {
    if let Ok(buf) = PyBuffer::<i32>::get(obj) {
        if buf.is_c_contiguous() {
            let slice = unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const i32, buf.item_count())
            };
            return Ok(CowSlice::Borrowed(slice));
        }
    }
    if let Ok(buf) = PyBuffer::<u32>::get(obj) {
        if buf.is_c_contiguous() {
            let slice = unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const i32, buf.item_count())
            };
            return Ok(CowSlice::Borrowed(slice));
        }
    }
    let vec: Vec<i32> = obj.extract()?;
    Ok(CowSlice::Owned(vec))
}

pub fn get_cow_slice_u8<'py>(obj: &Bound<'py, PyAny>) -> PyResult<CowSlice<'py, u8>> {
    if let Ok(buf) = PyBuffer::<u8>::get(obj) {
        if buf.is_c_contiguous() {
            // SAFETY: C-contiguous buffer = one densely packed block; `obj` outlives `'py`.
            let slice =
                unsafe { std::slice::from_raw_parts(buf.buf_ptr() as *const u8, buf.item_count()) };
            return Ok(CowSlice::Borrowed(slice));
        }
    }
    if let Ok(buf) = PyBuffer::<i8>::get(obj) {
        if buf.is_c_contiguous() {
            // SAFETY: as above; every `i8` bit pattern is a valid `u8`.
            let slice =
                unsafe { std::slice::from_raw_parts(buf.buf_ptr() as *const u8, buf.item_count()) };
            return Ok(CowSlice::Borrowed(slice));
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
