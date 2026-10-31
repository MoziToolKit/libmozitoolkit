//! Attribute access and management for `MeshData`.

use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::PyMemoryView;

use super::PyMeshData;

/// Read-only zero-copy memoryview of a numeric custom attribute by name.
pub fn attribute_memoryview<'py>(
    mesh: &Bound<'py, PyMeshData>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyMemoryView>>> {
    let py = mesh.py();
    let inner = &mesh.borrow().inner;
    if let Some(attr) = inner.get_custom_attribute(name) {
        if let Some(bytes_slice) = attr.as_bytes() {
            let ptr = bytes_slice.as_ptr();
            let len = bytes_slice.len();
            return Ok(Some(crate::buffer::create_zerocopy_memoryview(
                py,
                ptr,
                len,
                mesh.as_any(),
            )?));
        }
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

    let mut raw_buf = std::mem::MaybeUninit::<pyo3::ffi::Py_buffer>::uninit();
    let ret = unsafe {
        pyo3::ffi::PyObject_GetBuffer(
            buffer.as_ptr(),
            raw_buf.as_mut_ptr(),
            pyo3::ffi::PyBUF_SIMPLE,
        )
    };
    if ret != 0 {
        return Err(PyErr::fetch(buffer.py()));
    }
    let managed = RawPyBuffer(unsafe { raw_buf.assume_init() });
    let raw_bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(managed.0.buf as *const u8, managed.0.len as usize) };

    let attr_data = crate::attributes::parse_attribute_buffer(dtype, raw_bytes)?;

    mesh.inner
        .add_custom_attribute(mtk_core::attributes::MeshAttribute {
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

    mesh.inner
        .add_custom_attribute(mtk_core::attributes::MeshAttribute {
            name: name.to_string(),
            domain: domain_enum,
            data: mtk_core::attributes::AttributeData::String(values),
        });

    Ok(())
}

/// Add a string custom attribute from a compact palette + u32 index buffer.
///
/// Avoids materializing one Python string object per face on the host side;
/// the paletted values are expanded inside the native core.
pub fn add_string_attribute_from_palette(
    mesh: &mut PyMeshData,
    name: &str,
    domain: &str,
    palette: Vec<String>,
    indices: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let palette_len = palette.len();
    let values: Vec<String> = read_u32_indices(indices)?
        .into_iter()
        .map(|idx| {
            palette.get(idx as usize).cloned().unwrap_or_else(|| {
                debug_assert!(
                    (idx as usize) < palette_len,
                    "source texture palette index {idx} out of range"
                );
                String::new()
            })
        })
        .collect();
    add_string_attribute(mesh, name, domain, values)
}

fn read_u32_indices(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u32>> {
    if let Ok(buf) = PyBuffer::<u32>::get(obj) {
        if buf.is_c_contiguous() {
            let count = buf.item_count();
            let slice = unsafe { std::slice::from_raw_parts(buf.buf_ptr() as *const u32, count) };
            return Ok(slice.to_vec());
        }
    }
    obj.extract::<Vec<u32>>()
}

/// Retrieve a string custom attribute's values.
pub fn get_string_attribute(mesh: &PyMeshData, name: &str) -> Option<Vec<String>> {
    if let Some(attr) = mesh.inner.get_custom_attribute(name) {
        if let mtk_core::attributes::AttributeData::String(ref vals) = attr.data {
            return Some(vals.clone());
        }
    }
    None
}

/// Check if a custom attribute exists.
pub fn has_attribute(mesh: &PyMeshData, name: &str) -> bool {
    mesh.inner.has_custom_attribute(name)
}

/// Remove a custom attribute by name. Returns True if existed and removed.
pub fn remove_attribute(mesh: &mut PyMeshData, name: &str) -> bool {
    mesh.inner.remove_custom_attribute(name).is_some()
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
