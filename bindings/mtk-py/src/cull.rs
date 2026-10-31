//! # `mtk-py` Face Culling Engine Binding
//!
//! Exposes `FaceCuller` and occlusion rules to Python.

use pyo3::prelude::*;

use mtk_cull::types::{GlassCullMode, LeavesCullMode};
use mtk_cull::FaceCuller;

/// Python wrapper for `FaceCuller` occlusion and geometry optimization engine.
#[pyclass(name = "FaceCuller")]
#[derive(Debug, Clone)]
pub struct PyFaceCuller {
    pub(crate) inner: FaceCuller,
}

#[pymethods]
impl PyFaceCuller {
    #[new]
    #[pyo3(signature = (leaves_cull_mode=0, glass_cull_mode=0))]
    pub fn new(leaves_cull_mode: u8, glass_cull_mode: u8) -> PyResult<Self> {
        let leaves = match leaves_cull_mode {
            0 => LeavesCullMode::SingleFace,
            1 => LeavesCullMode::Fancy,
            2 => LeavesCullMode::Fast,
            3 => LeavesCullMode::None,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Invalid leaves_cull_mode (0: SingleFace, 1: Fancy, 2: Fast, 3: None)",
                ))
            }
        };

        let glass = match glass_cull_mode {
            0 => GlassCullMode::Group,
            1 => GlassCullMode::SameBlock,
            2 => GlassCullMode::None,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Invalid glass_cull_mode (0: Group, 1: SameBlock, 2: None)",
                ))
            }
        };

        Ok(Self {
            inner: FaceCuller::new(leaves, glass),
        })
    }

    /// Clears the internal block culling metadata cache.
    pub fn clear_cache(&self) {
        self.inner.clear_cache();
    }

    fn __repr__(&self) -> String {
        format!("<FaceCuller cached_entries={}>", self.inner.cache_len())
    }
}
