//! Scene Interchange metadata, capabilities, and validation reports.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use libmtk::scene::SceneMetadata;

/// Helper converting internal SceneCapabilities into a standard Python dictionary.
pub(crate) fn capabilities_to_dict<'py>(
    py: Python<'py>,
    caps: &libmtk::SceneCapabilities,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("has_colormaps", caps.has_colormaps)?;
    dict.set_item("has_ctm", caps.has_ctm)?;
    dict.set_item("has_pbr", caps.has_pbr)?;
    dict.set_item("has_dynamic_entities", caps.has_dynamic_entities)?;
    Ok(dict)
}

/// Python wrapper for Scene Interchange metadata (`META` chunk).
#[pyclass(name = "SceneMetadata")]
#[derive(Debug, Clone)]
pub struct PySceneMetadata {
    pub(crate) inner: SceneMetadata,
}

#[pymethods]
impl PySceneMetadata {
    #[getter]
    pub fn format_version(&self) -> u32 {
        self.inner.format_version
    }

    #[getter]
    pub fn bounds(&self) -> [i32; 6] {
        self.inner.bounds
    }

    #[getter]
    pub fn voxel_count(&self) -> usize {
        self.inner.voxel_count
    }

    #[getter]
    pub fn unique_states_count(&self) -> usize {
        self.inner.unique_states_count
    }

    #[getter]
    pub fn unique_biomes_count(&self) -> usize {
        self.inner.unique_biomes_count
    }

    #[getter]
    pub fn unique_entities_count(&self) -> usize {
        self.inner.unique_entities_count
    }

    #[getter]
    pub fn atlas_resolution(&self) -> [u32; 2] {
        self.inner.atlas_resolution
    }

    #[getter]
    pub fn has_animated_atlas(&self) -> bool {
        self.inner.has_animated_atlas
    }

    #[getter]
    pub fn generator(&self) -> &str {
        &self.inner.generator
    }

    #[getter]
    pub fn created_at_epoch_secs(&self) -> u64 {
        self.inner.created_at_epoch_secs
    }

    #[getter]
    pub fn capabilities<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        capabilities_to_dict(py, &self.inner.capabilities)
    }

    /// Converts metadata into a Python dictionary.
    pub fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("format_version", self.inner.format_version)?;
        dict.set_item("bounds", self.inner.bounds)?;
        dict.set_item("voxel_count", self.inner.voxel_count)?;
        dict.set_item("unique_states_count", self.inner.unique_states_count)?;
        dict.set_item("unique_biomes_count", self.inner.unique_biomes_count)?;
        dict.set_item("unique_entities_count", self.inner.unique_entities_count)?;
        dict.set_item("atlas_resolution", self.inner.atlas_resolution)?;
        dict.set_item("has_animated_atlas", self.inner.has_animated_atlas)?;
        dict.set_item("generator", &self.inner.generator)?;
        dict.set_item("created_at_epoch_secs", self.inner.created_at_epoch_secs)?;

        let cap_dict = PyDict::new(py);
        cap_dict.set_item("has_colormaps", self.inner.capabilities.has_colormaps)?;
        cap_dict.set_item("has_ctm", self.inner.capabilities.has_ctm)?;
        cap_dict.set_item("has_pbr", self.inner.capabilities.has_pbr)?;
        cap_dict.set_item(
            "has_dynamic_entities",
            self.inner.capabilities.has_dynamic_entities,
        )?;
        dict.set_item("capabilities", cap_dict)?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "<SceneMetadata v{} voxels={} states={} res=({}, {})>",
            self.inner.format_version,
            self.inner.voxel_count,
            self.inner.unique_states_count,
            self.inner.atlas_resolution[0],
            self.inner.atlas_resolution[1]
        )
    }
}

/// Result of scene package integrity and capability validation.
#[pyclass(name = "ValidationReport")]
#[derive(Debug, Clone)]
pub struct PyValidationReport {
    #[pyo3(get)]
    pub is_valid: bool,
    #[pyo3(get)]
    pub errors: Vec<String>,
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

#[pymethods]
impl PyValidationReport {
    pub fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("is_valid", self.is_valid)?;
        dict.set_item("errors", self.errors.clone())?;
        dict.set_item("warnings", self.warnings.clone())?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationReport(is_valid={}, errors={}, warnings={})",
            self.is_valid,
            self.errors.len(),
            self.warnings.len()
        )
    }
}
