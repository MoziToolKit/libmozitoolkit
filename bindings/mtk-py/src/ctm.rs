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

    /// Number of active CTM rules loaded in this solver.
    #[getter]
    pub fn rule_count(&self) -> usize {
        self.inner.rules.len()
    }

    fn __repr__(&self) -> String {
        format!("<CtmSolver rules={}>", self.inner.rules.len())
    }
}
