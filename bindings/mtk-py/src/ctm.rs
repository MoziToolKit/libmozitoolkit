//! OptiFine / Continuity Connected Textures (CTM) Python bindings.

use mtk_resource::CtmSolver;
use pyo3::prelude::*;
use std::sync::Arc;

/// Python wrapper for `CtmSolver` connected texture pattern solver.
#[pyclass(name = "CtmSolver")]
#[derive(Clone, Default)]
pub struct PyCtmSolver {
    pub(crate) inner: Arc<CtmSolver>,
}

#[pymethods]
impl PyCtmSolver {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CtmSolver::default()),
        }
    }

    fn __repr__(&self) -> String {
        format!("<CtmSolver rules={}>", self.inner.rules.len())
    }

    /// Returns the number of loaded CTM rules.
    #[getter]
    pub fn rule_count(&self) -> usize {
        self.inner.rules.len()
    }

    /// Serializes internal CTM rules to a JSON string.
    pub fn to_json(&self) -> PyResult<String> {
        self.inner
            .to_json()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Deserializes a CtmSolver from a JSON string of rules.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let solver = CtmSolver::from_json(json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(solver),
        })
    }
}
