use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;

pub enum BorrowedU8Buffer {
    Py(PyBuffer<u8>),
    Owned(Vec<u8>),
}

impl BorrowedU8Buffer {
    pub fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<u8>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<u8>>()?))
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const u8, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

pub enum BorrowedF32Buffer {
    Py(PyBuffer<f32>),
    Owned(Vec<f32>),
}

impl BorrowedF32Buffer {
    pub fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<f32>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<f32>>()?))
    }

    pub fn as_slice(&self) -> &[f32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const f32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

pub enum BorrowedU32Buffer {
    Py(PyBuffer<u32>),
    Owned(Vec<u32>),
}

impl BorrowedU32Buffer {
    pub fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<u32>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<u32>>()?))
    }

    pub fn as_slice(&self) -> &[u32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const u32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}
