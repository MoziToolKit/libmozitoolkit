//! # `mtk-py` Scene Interchange Bindings
//!
//! Exposes `.mtkscene` scene package reading and end-to-end scene interchange export
//! to Python.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyMemoryView};

use libmtk::scene::{
    export_scene_package as libmtk_export_scene, CtmMode, SceneExportRequest, SceneExporterConfig,
    SceneMetadata, ScenePackageReader,
};
use mtk_voxel::{CoordinateSystem, MesherConfig, SparseVoxelPayload};

use crate::model::PyBakedModelDatabase;
use crate::point_cloud::PyVoxelPointCloud;
use crate::resource::PyAssetCache;
use crate::texture::PyBakedAtlas;

/// Capabilities declared by a scene package.
#[pyclass(name = "SceneCapabilities")]
#[derive(Debug, Clone)]
pub struct PySceneCapabilities {
    pub(crate) inner: libmtk::SceneCapabilities,
}

#[pymethods]
impl PySceneCapabilities {
    #[getter]
    pub fn has_colormaps(&self) -> bool {
        self.inner.has_colormaps
    }

    #[getter]
    pub fn has_ctm(&self) -> bool {
        self.inner.has_ctm
    }

    #[getter]
    pub fn has_pbr(&self) -> bool {
        self.inner.has_pbr
    }

    #[getter]
    pub fn has_dynamic_entities(&self) -> bool {
        self.inner.has_dynamic_entities
    }

    pub fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("has_colormaps", self.inner.has_colormaps)?;
        dict.set_item("has_ctm", self.inner.has_ctm)?;
        dict.set_item("has_pbr", self.inner.has_pbr)?;
        dict.set_item("has_dynamic_entities", self.inner.has_dynamic_entities)?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "SceneCapabilities(has_colormaps={}, has_ctm={}, has_pbr={}, has_dynamic_entities={})",
            self.inner.has_colormaps,
            self.inner.has_ctm,
            self.inner.has_pbr,
            self.inner.has_dynamic_entities
        )
    }
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
        let dict = PyDict::new(py);
        dict.set_item("has_colormaps", self.inner.capabilities.has_colormaps)?;
        dict.set_item("has_ctm", self.inner.capabilities.has_ctm)?;
        dict.set_item("has_pbr", self.inner.capabilities.has_pbr)?;
        dict.set_item(
            "has_dynamic_entities",
            self.inner.capabilities.has_dynamic_entities,
        )?;
        Ok(dict)
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
    pub fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
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

/// High-level reader for `.mtkscene` scene interchange packages.
#[pyclass(name = "ScenePackageReader")]
pub struct PyScenePackageReader {
    pub(crate) inner: ScenePackageReader,
    package_path: String,
}

#[pymethods]
impl PyScenePackageReader {
    /// Opens an `.mtkscene` package from a file path.
    #[staticmethod]
    pub fn open(path: &str) -> PyResult<Self> {
        let reader = ScenePackageReader::open_file(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        Ok(Self {
            inner: reader,
            package_path: path.to_string(),
        })
    }

    /// Absolute path of the opened package.
    #[getter]
    pub fn package_path(&self) -> &str {
        &self.package_path
    }

    /// Returns the global scene metadata.
    #[getter]
    pub fn metadata(&self) -> PySceneMetadata {
        PySceneMetadata {
            inner: self.inner.metadata().clone(),
        }
    }

    /// Reads and deserializes the sparse voxels as a `VoxelPointCloud`.
    #[pyo3(signature = (origin_centered=true, z_up=true))]
    pub fn read_point_cloud(
        &self,
        origin_centered: bool,
        z_up: bool,
    ) -> PyResult<PyVoxelPointCloud> {
        let sparse = self
            .inner
            .read_sparse_voxels()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let config = MesherConfig {
            origin_centered,
            coordinate_system: if z_up {
                CoordinateSystem::ZUpRightHanded
            } else {
                CoordinateSystem::Minecraft
            },
            ..Default::default()
        };
        let cloud = sparse.to_point_cloud(Some(&config));
        Ok(PyVoxelPointCloud { inner: cloud })
    }

    /// Reads the tree-shaken baked model database if present in the package.
    pub fn read_models(&self) -> PyResult<Option<PyBakedModelDatabase>> {
        let db = self
            .inner
            .read_models()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(db.map(|d| PyBakedModelDatabase {
            inner: std::sync::Arc::new(d),
        }))
    }

    /// Reads the scene mini-atlas address map if present in the package.
    pub fn read_atlas(&self) -> PyResult<Option<PyBakedAtlas>> {
        let map = self
            .inner
            .read_atlas_mapping()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(map.map(|m| PyBakedAtlas {
            inner: mtk_texture::BakedAtlas {
                chunks: Vec::new(),
                address_map: m,
            },
            banner_baker: None,
        }))
    }

    /// Reads the raw atlas mapping JSON string if present.
    pub fn read_atlas_mapping_json(&self) -> PyResult<Option<String>> {
        if !self.inner.inner_reader().has_chunk("atlas/mapping") {
            return Ok(None);
        }
        let json = self
            .inner
            .inner_reader()
            .read_chunk_str("atlas/mapping")
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Some(json))
    }

    /// Checks whether custom colormaps are carried in this package.
    pub fn has_colormaps(&self) -> bool {
        self.inner.has_colormaps()
    }

    /// Reads raw PNG bytes of a packaged colormap if present.
    pub fn read_colormap<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let opt = self
            .inner
            .read_colormap(name)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        Ok(opt.map(|bytes| PyBytes::new(py, &bytes)))
    }

    /// Performs an integrity check on models, atlas, colormaps, and CTM rules.
    pub fn validate(&self) -> PyResult<PyValidationReport> {
        let report = self
            .inner
            .validate()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyValidationReport {
            is_valid: report.is_valid,
            errors: report.errors,
            warnings: report.warnings,
        })
    }

    /// Reads packaged colormaps into a parsed VanillaColormaps structure.
    pub fn read_colormaps(&self) -> PyResult<Option<crate::material::PyVanillaColormaps>> {
        let cms = self
            .inner
            .read_colormaps()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(cms.map(|c| crate::material::PyVanillaColormaps { inner: c }))
    }

    /// Re-installs or updates the resident colormaps from this package into the active palette cache.
    pub fn install_colormaps(&self) -> PyResult<()> {
        self.inner
            .install_colormaps()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Returns the effective capabilities of this package with chunk probing fallback for legacy packages.
    pub fn effective_capabilities<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let caps = self.inner.effective_capabilities();
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("has_colormaps", caps.has_colormaps)?;
        dict.set_item("has_ctm", caps.has_ctm)?;
        dict.set_item("has_pbr", caps.has_pbr)?;
        dict.set_item("has_dynamic_entities", caps.has_dynamic_entities)?;
        Ok(dict)
    }

    /// Checks whether dynamic entity slots mapping is carried in this package.
    pub fn has_dynamic_entity_slots(&self) -> bool {
        self.inner.has_dynamic_entity_slots()
    }

    /// Reads dynamic entity slot mapping if present in the package.
    pub fn read_dynamic_entity_slots(
        &self,
    ) -> PyResult<Option<std::collections::HashMap<String, (u16, u16)>>> {
        self.inner
            .read_dynamic_entity_slots()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Reads the packaged BiomeResolver if present in the package.
    pub fn read_biome_resolver(&self) -> PyResult<Option<crate::material::PyBiomeResolver>> {
        let resolver = self
            .inner
            .read_biome_resolver()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(resolver.map(|r| crate::material::PyBiomeResolver { inner: r }))
    }

    /// Checks whether custom CTM rules are carried in this package.
    pub fn has_ctm_rules(&self) -> bool {
        self.inner.has_ctm_rules()
    }

    /// Reads raw CTM rules JSON from this package if present.
    pub fn read_ctm_rules_json(&self) -> PyResult<Option<String>> {
        self.inner
            .read_ctm_rules_json()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Reads packaged CTM rules and constructs an indexed CtmSolver.
    pub fn read_ctm_solver(&self) -> PyResult<Option<crate::ctm::PyCtmSolver>> {
        let solver = self
            .inner
            .read_ctm_solver()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(solver.map(|s| crate::ctm::PyCtmSolver {
            inner: std::sync::Arc::new(s),
        }))
    }

    /// Returns list of all texture chunk identifiers stored in the package.
    pub fn get_atlas_texture_names(&self) -> Vec<String> {
        self.inner
            .inner_reader()
            .toc()
            .iter_by_type(*b"ATLS")
            .filter(|e| e.identifier.starts_with("atlas/textures/"))
            .map(|e| e.identifier.clone())
            .collect()
    }

    /// Reads and decodes a texture chunk (PNG) directly in memory into RGBA8 bytes: `(width, height, memoryview)`.
    /// `flip_v`: If True (default True for Blender layout), flips rows vertically natively in Rust.
    #[pyo3(signature = (chunk_id, flip_v=true))]
    pub fn read_texture_rgba<'py>(
        &self,
        py: Python<'py>,
        chunk_id: &str,
        flip_v: bool,
    ) -> PyResult<(u32, u32, Bound<'py, PyMemoryView>)> {
        let buf = self
            .inner
            .read_texture_rgba_with_orientation(chunk_id, flip_v)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let width = buf.width;
        let height = buf.height;
        let py_bytes = PyBytes::new(py, &buf.pixels);
        let memview = PyMemoryView::from(&py_bytes)?;
        Ok((width, height, memview))
    }

    /// Reads and decodes a texture chunk directly in memory into normalized float32 pixel memoryview: `(width, height, memoryview)`.
    /// `flip_v`: If True (default True for Blender layout), flips rows vertically natively in Rust.
    #[pyo3(signature = (chunk_id, flip_v=true))]
    pub fn read_texture_f32<'py>(
        &self,
        py: Python<'py>,
        chunk_id: &str,
        flip_v: bool,
    ) -> PyResult<(u32, u32, Bound<'py, PyMemoryView>)> {
        let (width, height, f32_pixels) = self
            .inner
            .read_texture_f32_with_orientation(chunk_id, flip_v)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let memview = crate::buffer::create_owned_f32_memoryview(py, f32_pixels)?;
        Ok((width, height, memview))
    }

    /// Extracts a single texture chunk to disk.
    pub fn extract_atlas_texture(&self, chunk_id: &str, output_path: &str) -> PyResult<()> {
        self.inner
            .extract_atlas_texture(chunk_id, output_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "<ScenePackageReader path=\"{}\" voxels={}>",
            self.package_path,
            self.inner.metadata().voxel_count
        )
    }
}

/// End-to-end exporter: builds Scene Mini-Atlas, remaps UVs, tree-shakes models, and writes `.mtkscene`.
#[pyfunction]
#[pyo3(signature = (output_path, point_cloud, asset_cache, *, max_atlas_size=2048, mip_level=0, dynamic_entity_atlases=None, ctm_solver=None, enable_ctm=None, dynamic_entity_slots=None))]
pub fn export_scene_package(
    output_path: &str,
    point_cloud: &PyVoxelPointCloud,
    asset_cache: &mut PyAssetCache,
    max_atlas_size: Option<u32>,
    mip_level: Option<u32>,
    dynamic_entity_atlases: Option<std::collections::HashMap<u16, Bound<'_, PyBytes>>>,
    ctm_solver: Option<&crate::ctm::PyCtmSolver>,
    enable_ctm: Option<bool>,
    dynamic_entity_slots: Option<std::collections::HashMap<String, (u16, u16)>>,
) -> PyResult<PySceneMetadata> {
    let sparse_voxels = SparseVoxelPayload::from_point_cloud(&point_cloud.inner);
    let models = asset_cache
        .inner
        .load_models()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    let atlas_map = asset_cache
        .inner
        .load_atlas_mapping()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let config = SceneExporterConfig {
        max_atlas_size: max_atlas_size.unwrap_or(2048),
        mip_level: mip_level.unwrap_or(0),
    };

    let converted_dyn_atlases: Option<std::collections::HashMap<u16, Vec<u8>>> =
        dynamic_entity_atlases.map(|map| {
            map.into_iter()
                .map(|(k, v)| (k, v.as_bytes().to_vec()))
                .collect()
        });

    let resolved_ctm_mode = if let Some(solver) = ctm_solver {
        if enable_ctm == Some(false) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Conflicting CTM arguments: explicit ctm_solver was provided, but enable_ctm is False",
            ));
        }
        CtmMode::Explicit(solver.inner.rules.as_slice())
    } else {
        match enable_ctm {
            Some(true) => CtmMode::Auto,
            Some(false) | None => CtmMode::Disabled,
        }
    };

    let request = SceneExportRequest {
        voxels: &sparse_voxels,
        source_models: &models,
        source_atlas_map: &atlas_map,
        atlas_source: &mut asset_cache.inner,
        config: &config,
        dynamic_entity_atlases: converted_dyn_atlases.as_ref(),
        dynamic_entity_slots: dynamic_entity_slots.as_ref(),
        ctm_mode: resolved_ctm_mode,
    };

    let result = libmtk_export_scene(output_path, request, None)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PySceneMetadata {
        inner: result.metadata,
    })
}
