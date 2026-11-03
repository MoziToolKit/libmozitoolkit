//! # `mtk-py` Scene Interchange Bindings
//!
//! Exposes `.mtkscene` scene package reading and end-to-end scene interchange export
//! to Python.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyMemoryView};

use libmtk::scene::{
    export_scene_package as libmtk_export_scene, SceneExporterConfig, SceneMetadata,
    ScenePackageReader,
};
use mtk_voxel::SparseVoxelPayload;

use crate::model::PyBakedModelDatabase;
use crate::point_cloud::PyVoxelPointCloud;
use crate::resource::PyAssetCache;
use crate::texture::PyBakedAtlas;

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
    pub fn read_point_cloud(&self) -> PyResult<PyVoxelPointCloud> {
        let sparse = self
            .inner
            .read_sparse_voxels()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let cloud = sparse.to_point_cloud(None);
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
#[pyo3(signature = (output_path, point_cloud, asset_cache, max_atlas_size=2048, mip_level=0, dynamic_entity_atlases=None))]
pub fn export_scene_package(
    output_path: &str,
    point_cloud: &PyVoxelPointCloud,
    asset_cache: &mut PyAssetCache,
    max_atlas_size: Option<u32>,
    mip_level: Option<u32>,
    dynamic_entity_atlases: Option<std::collections::HashMap<u16, Bound<'_, PyBytes>>>,
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

    let result = libmtk_export_scene(
        output_path,
        &sparse_voxels,
        &models,
        &atlas_map,
        &mut asset_cache.inner,
        &config,
        converted_dyn_atlases.as_ref(),
        None,
    )
    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PySceneMetadata {
        inner: result.metadata,
    })
}
