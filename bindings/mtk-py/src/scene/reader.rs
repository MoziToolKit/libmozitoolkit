//! High-level reader for `.mtkscene` scene interchange packages.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyMemoryView};

use libmtk::ScenePackageReader;
use mtk_voxel::{CoordinateSystem, MesherConfig};

use super::metadata::PySceneMetadata;
use crate::model::PyBakedModelDatabase;
use crate::point_cloud::PyVoxelPointCloud;
use crate::texture::PyBakedAtlas;

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

    /// Returns the global scene metadata.
    #[getter]
    pub fn metadata(&self) -> PySceneMetadata {
        PySceneMetadata {
            inner: self.inner.metadata().clone(),
        }
    }

    /// Unpacks all major scene components into a high-level dictionary in a single pass.
    #[pyo3(signature = (origin_centered=true, z_up=true))]
    pub fn unpack<'py>(
        &self,
        py: Python<'py>,
        origin_centered: bool,
        z_up: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let bundle = self
            .inner
            .unpack()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let dict = PyDict::new(py);

        let meta_obj = PySceneMetadata {
            inner: bundle.metadata,
        };
        dict.set_item("metadata", meta_obj.to_dict(py)?)?;

        let config = MesherConfig {
            origin_centered,
            coordinate_system: if z_up {
                CoordinateSystem::ZUpRightHanded
            } else {
                CoordinateSystem::Minecraft
            },
            ..Default::default()
        };
        let cloud = bundle.sparse_voxels.to_point_cloud(Some(&config));
        dict.set_item("point_cloud", PyVoxelPointCloud { inner: cloud })?;

        let models_val: Option<PyBakedModelDatabase> =
            bundle.models.map(|d| PyBakedModelDatabase {
                inner: std::sync::Arc::new(d),
            });
        dict.set_item("models", models_val)?;

        let atlas_val: Option<PyBakedAtlas> = bundle.atlas_mapping.map(|m| PyBakedAtlas {
            inner: mtk_texture::BakedAtlas {
                chunks: Vec::new(),
                address_map: m,
            },
            banner_baker: None,
        });
        dict.set_item("atlas", atlas_val)?;

        dict.set_item("atlas_mapping_json", bundle.atlas_mapping_json)?;
        dict.set_item("texture_names", bundle.texture_names)?;

        let resolver_val: Option<crate::material::PyBiomeResolver> = bundle
            .biome_resolver
            .map(|r| crate::material::PyBiomeResolver { inner: r });
        dict.set_item("biome_resolver", resolver_val)?;

        let colormaps_val: Option<crate::material::PyVanillaColormaps> = bundle
            .colormaps
            .map(|c| crate::material::PyVanillaColormaps { inner: c });
        dict.set_item("colormaps", colormaps_val)?;

        dict.set_item("dynamic_entity_slots", bundle.dynamic_entity_slots)?;

        let solver_val: Option<crate::ctm::PyCtmSolver> =
            bundle.ctm_solver.map(|s| crate::ctm::PyCtmSolver {
                inner: std::sync::Arc::new(s),
            });
        dict.set_item("ctm_solver", solver_val)?;

        Ok(dict)
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

    fn __repr__(&self) -> String {
        format!(
            "<ScenePackageReader path=\"{}\" voxels={}>",
            self.package_path,
            self.inner.metadata().voxel_count
        )
    }
}
