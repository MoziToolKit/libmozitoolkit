//! High-level reader for `.mtkscene` scene interchange packages.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyMemoryView};

use libmtk::scene::ScenePackageReader;
use mtk_voxel::{CoordinateSystem, MesherConfig};

use super::metadata::{PySceneCapabilities, PySceneMetadata, PyValidationReport};
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
    pub fn effective_capabilities(&self) -> PySceneCapabilities {
        PySceneCapabilities {
            inner: self.inner.effective_capabilities(),
        }
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
