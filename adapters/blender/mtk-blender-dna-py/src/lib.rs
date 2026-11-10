//! Standalone Blender raw-memory adapter wheel.
//!
//! The module knows only typed source buffers and destination addresses. It has
//! no dependency on `libmtk_py`, `mtk-core`, or `mtk-voxel`.

use std::mem::{align_of, size_of};

use pyo3::buffer::PyBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyMemoryView;

/// Integer API version for the standalone DNA adapter contract.
pub const API_VERSION: u32 = 2;

fn map_memory_err(err: mtk_blender_dna::DnaError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

macro_rules! impl_copy_fn {
    ($name:ident, $ty:ty, $copy_fn:path) => {
        #[pyfunction]
        #[pyo3(signature = (src, dst_ptr, count, max_bytes=None))]
        pub fn $name(
            src: &Bound<'_, PyAny>,
            dst_ptr: usize,
            count: usize,
            max_bytes: Option<usize>,
        ) -> PyResult<usize> {
            if count == 0 {
                return Ok(0);
            }

            // Direct Fast Path: raw bytes buffer view (PyBufferOwner, memoryview, bytes)
            if let Ok(buf) = PyBuffer::<u8>::get(src) {
                let expected = count * size_of::<$ty>();
                if buf.is_c_contiguous() && buf.len_bytes() == expected {
                    let raw_ptr = buf.buf_ptr() as usize;
                    if raw_ptr % align_of::<$ty>() == 0 {
                        let slice =
                            unsafe { std::slice::from_raw_parts(raw_ptr as *const $ty, count) };
                        return unsafe { $copy_fn(slice, dst_ptr, max_bytes) }
                            .map_err(map_memory_err);
                    }
                }
            }

            // Fast Path for typed buffers (NumPy arrays or typed memoryviews)
            if let Ok(buf) = PyBuffer::<$ty>::get(src) {
                if buf.is_c_contiguous() && buf.item_count() == count {
                    let slice =
                        unsafe { std::slice::from_raw_parts(buf.buf_ptr() as *const $ty, count) };
                    return unsafe { $copy_fn(slice, dst_ptr, max_bytes) }.map_err(map_memory_err);
                }
            }

            Err(PyValueError::new_err(format!(
                "source buffer is not a compatible C-contiguous aligned {} array of {} element(s)",
                stringify!($ty),
                count
            )))
        }
    };
}

impl_copy_fn!(copy_f32, f32, mtk_blender_dna::copy_f32_to_ptr);
impl_copy_fn!(copy_i32, i32, mtk_blender_dna::copy_i32_to_ptr);
impl_copy_fn!(copy_u32, u32, mtk_blender_dna::copy_u32_to_ptr);
impl_copy_fn!(copy_u8, u8, mtk_blender_dna::copy_u8_to_ptr);

fn make_zerocopy_memoryview<'py>(
    py: Python<'py>,
    src_ptr: usize,
    count: usize,
    type_size: usize,
    type_align: usize,
    type_format: Option<&'static str>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    if count == 0 {
        let empty_bytes = pyo3::types::PyBytes::new(py, &[]);
        let empty_mv = PyMemoryView::from(&empty_bytes)?;
        if let Some(fmt) = type_format {
            if fmt != "B" {
                return Ok(empty_mv
                    .call_method1("cast", (fmt,))?
                    .cast_into::<PyMemoryView>()?);
            }
        }
        return Ok(empty_mv);
    }

    let total_bytes = count * type_size;
    mtk_blender_dna::validate_source(src_ptr, total_bytes, type_align, None)
        .map_err(map_memory_err)?;

    unsafe {
        let raw_mv_ptr = pyo3::ffi::PyMemoryView_FromMemory(
            src_ptr as *mut std::ffi::c_char,
            total_bytes as pyo3::ffi::Py_ssize_t,
            pyo3::ffi::PyBUF_READ,
        );
        if raw_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        let mv = Bound::from_owned_ptr(py, raw_mv_ptr).cast_into::<PyMemoryView>()?;
        if let Some(fmt) = type_format {
            if fmt != "B" {
                let casted = mv.call_method1("cast", (fmt,))?;
                return Ok(casted.cast_into::<PyMemoryView>()?);
            }
        }
        Ok(mv)
    }
}

/// Creates a read-only zero-copy `float32` memoryview directly referencing host memory.
#[pyfunction]
#[pyo3(signature = (src_ptr, count))]
pub fn as_memoryview_f32<'py>(
    py: Python<'py>,
    src_ptr: usize,
    count: usize,
) -> PyResult<Bound<'py, PyMemoryView>> {
    make_zerocopy_memoryview(
        py,
        src_ptr,
        count,
        size_of::<f32>(),
        align_of::<f32>(),
        Some("f"),
    )
}

/// Creates a read-only zero-copy `int32` memoryview directly referencing host memory.
#[pyfunction]
#[pyo3(signature = (src_ptr, count))]
pub fn as_memoryview_i32<'py>(
    py: Python<'py>,
    src_ptr: usize,
    count: usize,
) -> PyResult<Bound<'py, PyMemoryView>> {
    make_zerocopy_memoryview(
        py,
        src_ptr,
        count,
        size_of::<i32>(),
        align_of::<i32>(),
        Some("i"),
    )
}

/// Creates a read-only zero-copy `uint32` memoryview directly referencing host memory.
#[pyfunction]
#[pyo3(signature = (src_ptr, count))]
pub fn as_memoryview_u32<'py>(
    py: Python<'py>,
    src_ptr: usize,
    count: usize,
) -> PyResult<Bound<'py, PyMemoryView>> {
    make_zerocopy_memoryview(
        py,
        src_ptr,
        count,
        size_of::<u32>(),
        align_of::<u32>(),
        Some("I"),
    )
}

/// Creates a read-only zero-copy `uint8` memoryview directly referencing host memory.
#[pyfunction]
#[pyo3(signature = (src_ptr, count))]
pub fn as_memoryview_u8<'py>(
    py: Python<'py>,
    src_ptr: usize,
    count: usize,
) -> PyResult<Bound<'py, PyMemoryView>> {
    make_zerocopy_memoryview(
        py,
        src_ptr,
        count,
        size_of::<u8>(),
        align_of::<u8>(),
        Some("B"),
    )
}

/// Returns the standalone adapter package semantic version string.
#[pyfunction]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the standalone adapter API version.
#[pyfunction]
pub fn api_version() -> u32 {
    API_VERSION
}

#[pymodule]
fn libmtk_blender_dna(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(copy_f32, m)?)?;
    m.add_function(wrap_pyfunction!(copy_i32, m)?)?;
    m.add_function(wrap_pyfunction!(copy_u32, m)?)?;
    m.add_function(wrap_pyfunction!(copy_u8, m)?)?;
    m.add_function(wrap_pyfunction!(as_memoryview_f32, m)?)?;
    m.add_function(wrap_pyfunction!(as_memoryview_i32, m)?)?;
    m.add_function(wrap_pyfunction!(as_memoryview_u32, m)?)?;
    m.add_function(wrap_pyfunction!(as_memoryview_u8, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(api_version, m)?)?;
    Ok(())
}
