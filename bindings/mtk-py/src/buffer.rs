//! Safe zero-copy Python buffer protocol owner and memoryview factory.
//!
//! Exposes `PyBufferOwner` which implements Python's buffer protocol (`__getbuffer__`),
//! ensuring that parent Rust/Python structures or generated buffers remain alive without copying memory.

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyMemoryView;
use std::os::raw::{c_int, c_void};

#[allow(dead_code)]
enum BufferStorage {
    Parent(Py<PyAny>),
    Bytes(Vec<u8>),
    F32(Vec<f32>),
    I32(Vec<i32>),
    U32(Vec<u32>),
    U16(Vec<u16>),
    I16(Vec<i16>),
    I8(Vec<i8>),
}

/// 16-byte aligned static dummy memory for zero-length buffers to prevent null pointer dereferences.
static DUMMY_ALIGNED: [u64; 2] = [0; 2];

/// A lightweight Python buffer protocol adapter that provides a zero-copy view of
/// memory either borrowed from a parent Python object or held in typed Rust vectors.
#[pyclass]
pub struct PyBufferOwner {
    ptr: usize,
    total_bytes: usize,
    itemsize: usize,
    format: &'static [u8], // Null-terminated format string, e.g. b"f\0"
    shape: [ffi::Py_ssize_t; 1],
    strides: [ffi::Py_ssize_t; 1],
    byte_shape: [ffi::Py_ssize_t; 1],
    byte_strides: [ffi::Py_ssize_t; 1],
    readonly: bool,
    _storage: BufferStorage,
}

#[pymethods]
impl PyBufferOwner {
    unsafe fn __getbuffer__(
        slf: &Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        if view.is_null() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "view pointer is null",
            ));
        }
        let this = slf.borrow();
        if (flags & ffi::PyBUF_WRITABLE) != 0 && this.readonly {
            return Err(pyo3::exceptions::PyBufferError::new_err(
                "buffer is read-only",
            ));
        }

        (*view).buf = this.ptr as *mut c_void;
        (*view).obj = ffi::Py_NewRef(slf.as_ptr());
        (*view).len = this.total_bytes as ffi::Py_ssize_t;
        (*view).readonly = if this.readonly { 1 } else { 0 };
        (*view).ndim = 1;
        (*view).suboffsets = std::ptr::null_mut();
        (*view).internal = std::ptr::null_mut();

        let has_format = (flags & ffi::PyBUF_FORMAT) != 0;
        let itemsize = if has_format { this.itemsize } else { 1 };
        (*view).itemsize = itemsize as ffi::Py_ssize_t;
        (*view).format = if has_format {
            this.format.as_ptr() as *mut std::os::raw::c_char
        } else {
            std::ptr::null_mut()
        };

        if (flags & ffi::PyBUF_ND) != 0 {
            if has_format {
                (*view).shape = this.shape.as_ptr() as *mut ffi::Py_ssize_t;
            } else {
                (*view).shape = this.byte_shape.as_ptr() as *mut ffi::Py_ssize_t;
            }
        } else {
            (*view).shape = std::ptr::null_mut();
        }

        if (flags & ffi::PyBUF_STRIDES) != 0 {
            if has_format {
                (*view).strides = this.strides.as_ptr() as *mut ffi::Py_ssize_t;
            } else {
                (*view).strides = this.byte_strides.as_ptr() as *mut ffi::Py_ssize_t;
            }
        } else {
            (*view).strides = std::ptr::null_mut();
        }

        Ok(())
    }

    unsafe fn __releasebuffer__(&self, _view: *mut ffi::Py_buffer) {
        // Python buffer protocol decrements view->obj automatically.
    }
}

/// Creates a typed zero-copy `PyMemoryView` referencing the slice at `ptr` with `item_count` elements,
/// keeping `parent` alive in memory as long as the memoryview is referenced.
pub fn create_zerocopy_typed_memoryview<'py>(
    py: Python<'py>,
    ptr: *const u8,
    item_count: usize,
    itemsize: usize,
    format: &'static [u8],
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let total_bytes = item_count * itemsize;
    let safe_ptr = if item_count == 0 || ptr.is_null() {
        DUMMY_ALIGNED.as_ptr() as usize
    } else {
        ptr as usize
    };

    let owner = Bound::new(
        py,
        PyBufferOwner {
            ptr: safe_ptr,
            total_bytes,
            itemsize,
            format,
            shape: [item_count as ffi::Py_ssize_t],
            strides: [itemsize as ffi::Py_ssize_t],
            byte_shape: [total_bytes as ffi::Py_ssize_t],
            byte_strides: [1],
            readonly: true,
            _storage: BufferStorage::Parent(parent.clone().unbind()),
        },
    )?;
    unsafe {
        let py_mv_ptr = ffi::PyMemoryView_FromObject(owner.as_ptr());
        if py_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, py_mv_ptr).cast_into::<PyMemoryView>()?)
    }
}

#[inline]
pub fn create_zerocopy_f32_memoryview<'py>(
    py: Python<'py>,
    ptr: *const f32,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(
        py,
        ptr as *const u8,
        count,
        std::mem::size_of::<f32>(),
        b"f\0",
        parent,
    )
}

#[inline]
pub fn create_zerocopy_u32_memoryview<'py>(
    py: Python<'py>,
    ptr: *const u32,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(
        py,
        ptr as *const u8,
        count,
        std::mem::size_of::<u32>(),
        b"I\0",
        parent,
    )
}

#[inline]
pub fn create_zerocopy_i32_memoryview<'py>(
    py: Python<'py>,
    ptr: *const i32,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(
        py,
        ptr as *const u8,
        count,
        std::mem::size_of::<i32>(),
        b"i\0",
        parent,
    )
}

#[inline]
pub fn create_zerocopy_u16_memoryview<'py>(
    py: Python<'py>,
    ptr: *const u16,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(
        py,
        ptr as *const u8,
        count,
        std::mem::size_of::<u16>(),
        b"H\0",
        parent,
    )
}

#[inline]
pub fn create_zerocopy_i16_memoryview<'py>(
    py: Python<'py>,
    ptr: *const i16,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(
        py,
        ptr as *const u8,
        count,
        std::mem::size_of::<i16>(),
        b"h\0",
        parent,
    )
}

#[inline]
pub fn create_zerocopy_i8_memoryview<'py>(
    py: Python<'py>,
    ptr: *const i8,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(
        py,
        ptr as *const u8,
        count,
        std::mem::size_of::<i8>(),
        b"b\0",
        parent,
    )
}

#[inline]
pub fn create_zerocopy_u8_memoryview<'py>(
    py: Python<'py>,
    ptr: *const u8,
    count: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_typed_memoryview(py, ptr, count, std::mem::size_of::<u8>(), b"B\0", parent)
}

/// Fallback alias for untyped byte buffers.
#[inline]
pub fn create_zerocopy_memoryview<'py>(
    py: Python<'py>,
    ptr: *const u8,
    len: usize,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    create_zerocopy_u8_memoryview(py, ptr, len, parent)
}

/// Creates a memoryview directly over an owned `Vec<u8>` without secondary `PyBytes` duplication.
pub fn create_owned_bytes_memoryview<'py>(
    py: Python<'py>,
    bytes: Vec<u8>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let item_count = bytes.len();
    let total_bytes = item_count;
    let ptr = if item_count == 0 {
        DUMMY_ALIGNED.as_ptr() as usize
    } else {
        bytes.as_ptr() as usize
    };
    let owner = Bound::new(
        py,
        PyBufferOwner {
            ptr,
            total_bytes,
            itemsize: 1,
            format: b"B\0",
            shape: [item_count as ffi::Py_ssize_t],
            strides: [1],
            byte_shape: [total_bytes as ffi::Py_ssize_t],
            byte_strides: [1],
            readonly: true,
            _storage: BufferStorage::Bytes(bytes),
        },
    )?;
    unsafe {
        let py_mv_ptr = ffi::PyMemoryView_FromObject(owner.as_ptr());
        if py_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, py_mv_ptr).cast_into::<PyMemoryView>()?)
    }
}

/// Creates a memoryview directly over an owned `Vec<f32>` without reallocating or unsafe alignment casting.
pub fn create_owned_f32_memoryview<'py>(
    py: Python<'py>,
    vec: Vec<f32>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let item_count = vec.len();
    let itemsize = std::mem::size_of::<f32>();
    let total_bytes = item_count * itemsize;
    let ptr = if item_count == 0 {
        DUMMY_ALIGNED.as_ptr() as usize
    } else {
        vec.as_ptr() as usize
    };
    let owner = Bound::new(
        py,
        PyBufferOwner {
            ptr,
            total_bytes,
            itemsize,
            format: b"f\0",
            shape: [item_count as ffi::Py_ssize_t],
            strides: [itemsize as ffi::Py_ssize_t],
            byte_shape: [total_bytes as ffi::Py_ssize_t],
            byte_strides: [1],
            readonly: true,
            _storage: BufferStorage::F32(vec),
        },
    )?;
    unsafe {
        let py_mv_ptr = ffi::PyMemoryView_FromObject(owner.as_ptr());
        if py_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, py_mv_ptr).cast_into::<PyMemoryView>()?)
    }
}

/// Creates a memoryview directly over an owned `Vec<u32>` without reallocating or unsafe alignment casting.
pub fn create_owned_u32_memoryview<'py>(
    py: Python<'py>,
    vec: Vec<u32>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let item_count = vec.len();
    let itemsize = std::mem::size_of::<u32>();
    let total_bytes = item_count * itemsize;
    let ptr = if item_count == 0 {
        DUMMY_ALIGNED.as_ptr() as usize
    } else {
        vec.as_ptr() as usize
    };
    let owner = Bound::new(
        py,
        PyBufferOwner {
            ptr,
            total_bytes,
            itemsize,
            format: b"I\0",
            shape: [item_count as ffi::Py_ssize_t],
            strides: [itemsize as ffi::Py_ssize_t],
            byte_shape: [total_bytes as ffi::Py_ssize_t],
            byte_strides: [1],
            readonly: true,
            _storage: BufferStorage::U32(vec),
        },
    )?;
    unsafe {
        let py_mv_ptr = ffi::PyMemoryView_FromObject(owner.as_ptr());
        if py_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, py_mv_ptr).cast_into::<PyMemoryView>()?)
    }
}

/// Creates a memoryview directly over an owned `Vec<i32>` without reallocating or unsafe alignment casting.
pub fn create_owned_i32_memoryview<'py>(
    py: Python<'py>,
    vec: Vec<i32>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let item_count = vec.len();
    let itemsize = std::mem::size_of::<i32>();
    let total_bytes = item_count * itemsize;
    let ptr = if item_count == 0 {
        DUMMY_ALIGNED.as_ptr() as usize
    } else {
        vec.as_ptr() as usize
    };
    let owner = Bound::new(
        py,
        PyBufferOwner {
            ptr,
            total_bytes,
            itemsize,
            format: b"i\0",
            shape: [item_count as ffi::Py_ssize_t],
            strides: [itemsize as ffi::Py_ssize_t],
            byte_shape: [total_bytes as ffi::Py_ssize_t],
            byte_strides: [1],
            readonly: true,
            _storage: BufferStorage::I32(vec),
        },
    )?;
    unsafe {
        let py_mv_ptr = ffi::PyMemoryView_FromObject(owner.as_ptr());
        if py_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, py_mv_ptr).cast_into::<PyMemoryView>()?)
    }
}

/// Creates a memoryview directly over an owned `Vec<u16>` without reallocating or unsafe alignment casting.
pub fn create_owned_u16_memoryview<'py>(
    py: Python<'py>,
    vec: Vec<u16>,
) -> PyResult<Bound<'py, PyMemoryView>> {
    let item_count = vec.len();
    let itemsize = std::mem::size_of::<u16>();
    let total_bytes = item_count * itemsize;
    let ptr = if item_count == 0 {
        DUMMY_ALIGNED.as_ptr() as usize
    } else {
        vec.as_ptr() as usize
    };
    let owner = Bound::new(
        py,
        PyBufferOwner {
            ptr,
            total_bytes,
            itemsize,
            format: b"H\0",
            shape: [item_count as ffi::Py_ssize_t],
            strides: [itemsize as ffi::Py_ssize_t],
            byte_shape: [total_bytes as ffi::Py_ssize_t],
            byte_strides: [1],
            readonly: true,
            _storage: BufferStorage::U16(vec),
        },
    )?;
    unsafe {
        let py_mv_ptr = ffi::PyMemoryView_FromObject(owner.as_ptr());
        if py_mv_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, py_mv_ptr).cast_into::<PyMemoryView>()?)
    }
}
