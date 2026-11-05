//! # Python Mesh Attributes & Buffer Protocol Decoding
//!
//! Provides typed attribute buffer decoding, domain mapping, and Python conversions.

use pyo3::prelude::*;
use pyo3::types::PyList;

use mtk_core::{AttributeData, AttributeDomain};

/// Supported attribute domains matching modern DCC & OpenUSD standards.
#[pyclass(name = "AttributeDomain", eq, eq_int)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyAttributeDomain {
    Point,
    Corner,
    Face,
    Mesh,
}

/// Parses a domain string into an `AttributeDomain`.
pub fn parse_domain_str(domain: &str) -> PyResult<AttributeDomain> {
    match domain.trim().to_lowercase().as_str() {
        "point" | "vertex" => Ok(AttributeDomain::Point),
        "corner" | "loop" | "face_varying" => Ok(AttributeDomain::Corner),
        "face" | "polygon" | "uniform" => Ok(AttributeDomain::Face),
        "mesh" | "global" | "constant" => Ok(AttributeDomain::Mesh),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid attribute domain '{}'. Expected 'point', 'corner', 'face', or 'mesh'",
            domain
        ))),
    }
}

/// Generic helper to decode a contiguous raw byte slice into a typed `Vec<T>`.
fn copy_pod_vec<T: Copy>(raw_bytes: &[u8], type_name: &str) -> PyResult<Vec<T>> {
    let elem_size = std::mem::size_of::<T>();
    if raw_bytes.len() % elem_size != 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Buffer byte length not divisible by sizeof({})",
            type_name
        )));
    }
    let count = raw_bytes.len() / elem_size;
    let mut vec = Vec::with_capacity(count);
    if count > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(
                raw_bytes.as_ptr(),
                vec.as_mut_ptr() as *mut u8,
                raw_bytes.len(),
            );
            vec.set_len(count);
        }
    }
    Ok(vec)
}

/// Parses raw bytes into typed `AttributeData` based on the given dtype specifier.
pub fn parse_attribute_buffer(dtype: &str, raw_bytes: &[u8]) -> PyResult<AttributeData> {
    match dtype.trim().to_lowercase().as_str() {
        "float" | "float32" | "f32" => Ok(AttributeData::Float(copy_pod_vec::<f32>(
            raw_bytes, "float",
        )?)),
        "float2" | "vec2" => Ok(AttributeData::Float2(copy_pod_vec::<[f32; 2]>(
            raw_bytes, "float2",
        )?)),
        "float3" | "vec3" => Ok(AttributeData::Float3(copy_pod_vec::<[f32; 3]>(
            raw_bytes, "float3",
        )?)),
        "float4" | "vec4" | "color" => Ok(AttributeData::Float4(copy_pod_vec::<[f32; 4]>(
            raw_bytes, "float4",
        )?)),
        "int8" | "i8" => Ok(AttributeData::Int8(copy_pod_vec::<i8>(raw_bytes, "int8")?)),
        "int16" | "i16" => Ok(AttributeData::Int16(copy_pod_vec::<i16>(
            raw_bytes, "int16",
        )?)),
        "int32" | "int" | "i32" => Ok(AttributeData::Int32(copy_pod_vec::<i32>(
            raw_bytes, "int32",
        )?)),
        "uint8" | "u8" | "byte" => Ok(AttributeData::UInt8(copy_pod_vec::<u8>(
            raw_bytes, "uint8",
        )?)),
        "uint16" | "u16" => Ok(AttributeData::UInt16(copy_pod_vec::<u16>(
            raw_bytes, "uint16",
        )?)),
        "uint32" | "u32" => Ok(AttributeData::UInt32(copy_pod_vec::<u32>(
            raw_bytes, "uint32",
        )?)),
        "bool" | "boolean" => {
            let mut vec = Vec::with_capacity(raw_bytes.len());
            for &b in raw_bytes {
                vec.push(b != 0);
            }
            Ok(AttributeData::Bool(vec))
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Unsupported attribute data type '{}'",
            dtype
        ))),
    }
}

/// Converts `AttributeData` to a Python list.
pub fn attribute_data_to_py_list<'py>(
    py: Python<'py>,
    data: &AttributeData,
) -> PyResult<Bound<'py, PyAny>> {
    match data {
        AttributeData::Float(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Float2(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Float3(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Float4(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Int8(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Int16(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Int32(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::UInt8(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::UInt16(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::UInt32(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::Bool(v) => Ok(PyList::new(py, v)?.into_any()),
        AttributeData::String(v) => Ok(PyList::new(py, v)?.into_any()),
    }
}

/// Returns the canonical mesh attribute schema as
/// `(name, domain, data_type, role)` tuples.
///
/// Hosts must treat this as the single source of truth instead of maintaining
/// their own attribute name/type tables.
#[pyfunction]
pub fn mesh_attribute_schema() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    mtk_core::MESH_ATTRIBUTE_SPECS
        .iter()
        .map(|spec| {
            (
                spec.name,
                spec.domain.as_str(),
                spec.data_type,
                spec.role.as_str(),
            )
        })
        .collect()
}

/// Returns attribute names that must never be written and should be purged
/// from legacy mesh data.
#[pyfunction]
pub fn retired_mesh_attributes() -> Vec<&'static str> {
    mtk_core::RETIRED_MESH_ATTRIBUTES.to_vec()
}
