//! Attribute access and management for `MeshData`.

use pyo3::prelude::*;
use pyo3::types::PyMemoryView;

use super::PyMeshData;

/// Read-only zero-copy typed memoryview of a numeric custom attribute by name.
pub fn attribute_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if let Some(attr) = inner.get_custom_attribute(name) {
        let mv = match &attr.data {
            mtk_core::AttributeData::Float(v) => crate::buffer::create_zerocopy_f32_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Float2(v) => crate::buffer::create_zerocopy_f32_memoryview(
                py,
                v.as_ptr() as *const f32,
                v.len() * 2,
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Float3(v) => crate::buffer::create_zerocopy_f32_memoryview(
                py,
                v.as_ptr() as *const f32,
                v.len() * 3,
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Float4(v) => crate::buffer::create_zerocopy_f32_memoryview(
                py,
                v.as_ptr() as *const f32,
                v.len() * 4,
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Int8(v) => crate::buffer::create_zerocopy_i8_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Int16(v) => crate::buffer::create_zerocopy_i16_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Int32(v) => crate::buffer::create_zerocopy_i32_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::UInt8(v) => crate::buffer::create_zerocopy_u8_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::UInt16(v) => crate::buffer::create_zerocopy_u16_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::UInt32(v) => crate::buffer::create_zerocopy_u32_memoryview(
                py,
                v.as_ptr(),
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::Bool(v) => crate::buffer::create_zerocopy_u8_memoryview(
                py,
                v.as_ptr() as *const u8,
                v.len(),
                mesh.as_any(),
                &mesh.borrow().views,
            )?,
            mtk_core::AttributeData::String(_) => return Ok(None),
        };
        return Ok(Some(mv));
    }
    Ok(None)
}

struct RawPyBuffer(pyo3::ffi::Py_buffer);

impl Drop for RawPyBuffer {
    fn drop(&mut self) {
        unsafe { pyo3::ffi::PyBuffer_Release(&mut self.0) };
    }
}

/// Add a numeric custom attribute from any Python object supporting the Buffer Protocol.
pub fn add_attribute_from_buffer(
    mesh: &mut PyMeshData,
    name: &str,
    domain: &str,
    dtype: &str,
    buffer: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let domain_enum = crate::attributes::parse_domain_str(domain)?;
    let spec = crate::attributes::dtype_spec(dtype).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!("Unknown attribute dtype '{dtype}'"))
    })?;
    let expected_itemsize = spec.component_size;
    let expects_float = spec.is_float;

    let mut raw_buf = std::mem::MaybeUninit::<pyo3::ffi::Py_buffer>::uninit();
    // C-contiguous + FORMAT so a strided view or a different element size is visible instead of
    // being reinterpreted as tightly packed bytes of the requested dtype.
    let ret = unsafe {
        pyo3::ffi::PyObject_GetBuffer(
            buffer.as_ptr(),
            raw_buf.as_mut_ptr(),
            pyo3::ffi::PyBUF_FORMAT | pyo3::ffi::PyBUF_C_CONTIGUOUS,
        )
    };
    if ret != 0 {
        return Err(PyErr::fetch(buffer.py()));
    }
    let managed = RawPyBuffer(unsafe { raw_buf.assume_init() });

    let actual_itemsize = managed.0.itemsize.max(0) as usize;
    let format = if managed.0.format.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(managed.0.format) }
            .to_string_lossy()
            .into_owned()
    };
    if actual_itemsize != expected_itemsize {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Attribute buffer itemsize mismatch for dtype '{dtype}': expected {expected_itemsize} \
             bytes per component, buffer reports {actual_itemsize} (format '{format}')"
        )));
    }
    // Category check: an integer buffer must not be decoded as float and vice versa.
    let buffer_is_float = format.starts_with('f') || format.starts_with('d');
    let buffer_is_int = format
        .chars()
        .next()
        .is_some_and(|c| "iIhHbBlLqQ?".contains(c));
    if !format.is_empty()
        && ((expects_float && !buffer_is_float) || (!expects_float && !buffer_is_int))
    {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Attribute buffer format '{format}' is incompatible with dtype '{dtype}'"
        )));
    }

    let raw_bytes: &[u8] = if managed.0.len <= 0 {
        &[]
    } else if managed.0.buf.is_null() {
        return Err(pyo3::exceptions::PyBufferError::new_err(
            "Attribute buffer reported a null pointer with a non-zero length",
        ));
    } else {
        unsafe { std::slice::from_raw_parts(managed.0.buf as *const u8, managed.0.len as usize) }
    };

    let attr_data = crate::attributes::parse_attribute_buffer(dtype, raw_bytes)?;

    mesh.inner.add_custom_attribute(mtk_core::MeshAttribute {
        name: name.to_string(),
        domain: domain_enum,
        data: attr_data,
    });

    Ok(())
}

/// Add a string custom attribute.
pub fn add_string_attribute(
    mesh: &mut PyMeshData,
    name: &str,
    domain: &str,
    values: Vec<String>,
) -> PyResult<()> {
    let domain_enum = crate::attributes::parse_domain_str(domain)?;

    mesh.inner.add_custom_attribute(mtk_core::MeshAttribute {
        name: name.to_string(),
        domain: domain_enum,
        data: mtk_core::AttributeData::String(values),
    });

    Ok(())
}

/// Retrieve a string custom attribute's values.
pub fn get_string_attribute(mesh: &PyMeshData, name: &str) -> Option<Vec<String>> {
    if let Some(attr) = mesh.inner.get_custom_attribute(name) {
        if let mtk_core::AttributeData::String(ref vals) = attr.data {
            return Some(vals.clone());
        }
    }
    None
}

/// List all custom attribute names.
pub fn attribute_names(mesh: &PyMeshData) -> Vec<String> {
    mesh.inner.custom_attribute_names()
}

/// Get attribute metadata: (domain_name, type_name, element_count).
pub fn attribute_info(mesh: &PyMeshData, name: &str) -> Option<(String, String, usize)> {
    mesh.inner.get_custom_attribute(name).map(|attr| {
        (
            attr.domain.as_str().to_string(),
            attr.data.type_name().to_string(),
            attr.len(),
        )
    })
}
