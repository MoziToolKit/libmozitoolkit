//! # Python Bindings for Execution Profiling
//!
//! Exposes `mtk_core::ExecutionProfile` and `mtk_core::StageSpan` to Python,
//! enabling detailed stage-by-stage timing inspection for performance debugging.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use mtk_core::{ExecutionProfile, StageSpan};

/// Python representation of an individual execution stage span.
#[pyclass(name = "StageSpan")]
#[derive(Debug, Clone)]
pub struct PyStageSpan {
    pub(crate) inner: StageSpan,
}

#[pymethods]
impl PyStageSpan {
    /// Stage identifier, e.g. "prep", "parallel_meshing", "assemble".
    #[getter]
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// Domain/pipeline category, e.g. "mesh", "io", "bake", "cull", "subdivide", "host".
    #[getter]
    pub fn category(&self) -> &str {
        &self.inner.category
    }

    /// Nesting depth (0 = top-level, 1 = sub-stage, etc.).
    #[getter]
    pub fn depth(&self) -> u32 {
        self.inner.depth
    }

    /// Start offset from the operation start in microseconds.
    #[getter]
    pub fn start_offset_us(&self) -> u64 {
        self.inner.start_offset_us
    }

    /// Start offset from the operation start in milliseconds.
    #[getter]
    pub fn start_offset_ms(&self) -> f64 {
        self.inner.start_offset_ms()
    }

    /// Elapsed duration in milliseconds.
    #[getter]
    pub fn duration_ms(&self) -> f64 {
        self.inner.duration_ms()
    }

    /// Physical items processed during this stage, if applicable.
    #[getter]
    pub fn items_count(&self) -> Option<usize> {
        self.inner.items_count
    }

    fn __repr__(&self) -> String {
        let indent = "  ".repeat(self.inner.depth as usize);
        match self.inner.items_count {
            Some(c) => format!(
                "{}<StageSpan depth={} [{}] \"{}\" {:.2}ms ({} items)>",
                indent,
                self.inner.depth,
                self.inner.category,
                self.inner.name,
                self.inner.duration_ms(),
                c
            ),
            None => format!(
                "{}<StageSpan depth={} [{}] \"{}\" {:.2}ms>",
                indent,
                self.inner.depth,
                self.inner.category,
                self.inner.name,
                self.inner.duration_ms()
            ),
        }
    }
}

/// Consolidated performance report containing total duration and stage breakdowns.
#[pyclass(name = "ExecutionProfile")]
#[derive(Debug, Clone, Default)]
pub struct PyExecutionProfile {
    pub(crate) inner: ExecutionProfile,
}

impl From<ExecutionProfile> for PyExecutionProfile {
    fn from(inner: ExecutionProfile) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyExecutionProfile {
    /// Total duration of the operation in milliseconds.
    #[getter]
    pub fn total_duration_ms(&self) -> f64 {
        self.inner.total_duration_ms()
    }

    /// Accounted top-level duration in milliseconds.
    #[getter]
    pub fn accounted_duration_ms(&self) -> f64 {
        self.inner.accounted_duration_ms()
    }

    /// Unaccounted gap duration in milliseconds.
    #[getter]
    pub fn unaccounted_duration_ms(&self) -> f64 {
        self.inner.unaccounted_duration_ms()
    }

    /// Sequential list of stage spans ordered chronologically.
    #[getter]
    pub fn stages(&self) -> Vec<PyStageSpan> {
        self.inner
            .stages
            .iter()
            .cloned()
            .map(|inner| PyStageSpan { inner })
            .collect()
    }

    /// Converts the full profile report into a Python dictionary.
    pub fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("total_duration_us", self.inner.total_duration_us)?;
        dict.set_item("total_duration_ms", self.inner.total_duration_ms())?;
        dict.set_item("accounted_duration_ms", self.inner.accounted_duration_ms())?;
        dict.set_item(
            "unaccounted_duration_ms",
            self.inner.unaccounted_duration_ms(),
        )?;

        let stages_list = pyo3::types::PyList::empty(py);
        for stage in &self.inner.stages {
            let stage_dict = PyDict::new(py);
            stage_dict.set_item("name", stage.name.as_ref())?;
            stage_dict.set_item("category", stage.category.as_ref())?;
            stage_dict.set_item("depth", stage.depth)?;
            stage_dict.set_item("start_offset_us", stage.start_offset_us)?;
            stage_dict.set_item("start_offset_ms", stage.start_offset_ms())?;
            stage_dict.set_item("duration_us", stage.duration_us)?;
            stage_dict.set_item("duration_ms", stage.duration_ms())?;
            stage_dict.set_item("items_count", stage.items_count)?;
            stages_list.append(stage_dict)?;
        }
        dict.set_item("stages", stages_list)?;

        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "<ExecutionProfile total={:.2}ms accounted={:.2}ms stages={}>",
            self.inner.total_duration_ms(),
            self.inner.accounted_duration_ms(),
            self.inner.stages.len()
        )
    }
}
