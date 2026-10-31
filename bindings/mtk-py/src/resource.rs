//! # `mtk-py` Minecraft Resource Pack Binding
//!
//! Exposes layered resource pack loading and asset discovery to Python.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

#[cfg(feature = "zip")]
use mtk_resource::ZipPack;
use mtk_resource::{DirectoryPack, ResourcePackStack};

/// Python wrapper for layered Minecraft resource packs (`ResourcePackStack`).
#[pyclass(name = "ResourcePackStack")]
#[derive(Default)]
pub struct PyResourcePackStack {
    pub(crate) inner: ResourcePackStack,
}

#[pymethods]
impl PyResourcePackStack {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: ResourcePackStack::new(),
        }
    }

    /// Adds a folder/directory resource pack to the stack.
    #[pyo3(signature = (path, name=None))]
    pub fn add_directory_pack(&mut self, path: &str, name: Option<&str>) -> PyResult<bool> {
        let p = Path::new(path);
        if !p.exists() || !p.is_dir() {
            return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!(
                "Directory does not exist: {}",
                path
            )));
        }
        let pack_name = name.unwrap_or(path);
        let pack = DirectoryPack::new(pack_name, p);
        self.inner.append_pack(Box::new(pack));
        Ok(true)
    }

    /// Adds a `.zip` or `.jar` archive resource pack to the stack.
    pub fn add_zip_pack(&mut self, path: &str) -> PyResult<bool> {
        #[cfg(feature = "zip")]
        {
            let p = Path::new(path);
            if !p.exists() || !p.is_file() {
                return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!(
                    "ZIP archive file does not exist: {}",
                    path
                )));
            }
            let pack = ZipPack::from_file(path, p)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            self.inner.append_pack(Box::new(pack));
            Ok(true)
        }
        #[cfg(not(feature = "zip"))]
        {
            let _ = path;
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "ZIP support was not compiled in this build of mtk-py",
            ))
        }
    }

    /// Returns the number of loaded resource packs in the stack.
    pub fn get_pack_count(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the stack contains at least one pack providing vanilla minecraft client assets.
    pub fn has_vanilla_client_base(&self) -> bool {
        self.inner.has_vanilla_client_base()
    }

    /// Computes a deterministic fingerprint string for the active resource pack stack configuration.
    pub fn compute_stack_fingerprint(&self) -> String {
        self.inner.compute_stack_fingerprint()
    }

    /// Builds an indexed `CtmSolver` containing all active CTM rules loaded in this stack.
    pub fn build_ctm_solver(&self) -> crate::ctm::PyCtmSolver {
        crate::ctm::PyCtmSolver {
            inner: std::sync::Arc::new(self.inner.build_ctm_solver()),
        }
    }

    /// Returns the number of CTM rules discovered across all active packs in the stack.
    pub fn get_ctm_rule_count(&self) -> usize {
        self.inner.load_ctm_rules().len()
    }

    fn __repr__(&self) -> String {
        format!("<ResourcePackStack packs={}>", self.inner.len())
    }
}

/// Result of full-scale asset precompilation.
#[pyclass(name = "PrecompileResult")]
#[derive(Debug, Clone)]
pub struct PyPrecompileResult {
    #[pyo3(get)]
    pub success: bool,
    #[pyo3(get)]
    pub pack_count: usize,
    #[pyo3(get)]
    pub atlas_chunks: usize,
    #[pyo3(get)]
    pub baked_models: usize,
    #[pyo3(get)]
    pub fingerprint: String,
    #[pyo3(get)]
    pub package_path: String,
    #[pyo3(get)]
    pub cache_dir: String,
    pub profile: Option<mtk_core::profile::ExecutionProfile>,
}

#[pymethods]
impl PyPrecompileResult {
    /// Execution profile detailing prebake step durations.
    #[getter]
    pub fn profile(&self) -> Option<crate::profile::PyExecutionProfile> {
        self.profile.clone().map(Into::into)
    }

    fn __repr__(&self) -> String {
        format!(
            "<PrecompileResult packs={} atlas_chunks={} models={} package='{}'>",
            self.pack_count, self.atlas_chunks, self.baked_models, self.package_path
        )
    }
}

/// Reader and inspector for precompiled `.mtkcache` packages.
#[pyclass(name = "AssetCache")]
pub struct PyAssetCache {
    inner: libmtk::AssetCacheReader,
    package_path: String,
}

#[pymethods]
impl PyAssetCache {
    /// Opens an asset cache `.mtkcache` file using memory mapping.
    #[staticmethod]
    pub fn open(path: &str) -> PyResult<Self> {
        let reader = libmtk::AssetCacheReader::open(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        Ok(Self {
            inner: reader,
            package_path: path.to_string(),
        })
    }

    /// Validates whether a file at `path` matches the specified fingerprint using only the 64-byte header.
    #[staticmethod]
    pub fn is_valid(path: &str, fingerprint: &str) -> bool {
        libmtk::AssetCacheReader::is_valid_cache_file(path, fingerprint)
    }

    /// Absolute path to the opened package file.
    #[getter]
    pub fn package_path(&self) -> &str {
        &self.package_path
    }

    /// Full content fingerprint of the package.
    #[getter]
    pub fn fingerprint(&self) -> String {
        if let Ok(m) = self.inner.manifest() {
            m.fingerprint
        } else {
            let b = self.inner.fingerprint();
            format!("{:02x?}", b)
        }
    }

    /// Reads cache manifest metadata as a dictionary.
    pub fn get_manifest<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let m = self
            .inner
            .manifest()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let dict = PyDict::new(py);
        dict.set_item("format_version", m.format_version)?;
        dict.set_item("fingerprint", m.fingerprint)?;
        dict.set_item("pack_count", m.pack_count)?;
        dict.set_item("atlas_chunks", m.atlas_chunks)?;
        dict.set_item("standalone_textures", m.standalone_textures)?;
        dict.set_item("baked_models", m.baked_models)?;
        dict.set_item("created_at_epoch_secs", m.created_at_epoch_secs)?;
        Ok(dict)
    }

    /// Loads baked model database directly from package.
    pub fn load_models(&self) -> PyResult<crate::model::PyBakedModelDatabase> {
        let db = self
            .inner
            .load_models()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(crate::model::PyBakedModelDatabase {
            inner: std::sync::Arc::new(db),
        })
    }

    /// Loads baked atlas address map directly from package.
    pub fn load_atlas(&self) -> PyResult<crate::texture::PyBakedAtlas> {
        let map = self
            .inner
            .load_atlas_mapping()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let banner_baker = self
            .inner
            .load_banner_baker()
            .ok()
            .map(std::sync::Arc::new);
        Ok(crate::texture::PyBakedAtlas {
            inner: mtk_texture::BakedAtlas {
                chunks: Vec::new(),
                address_map: map,
            },
            banner_baker,
        })
    }

    /// Loads biome resolver directly from package.
    pub fn load_biome_resolver(&self) -> PyResult<crate::material::PyBiomeResolver> {
        let resolver = self
            .inner
            .load_biome_resolver()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(crate::material::PyBiomeResolver { inner: resolver })
    }

    /// Extracts a single texture chunk to the specified destination path.
    pub fn extract_atlas_texture(&self, filename: &str, output_path: &str) -> PyResult<()> {
        self.inner
            .extract_atlas_texture(filename, output_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
    }

    /// Loads raw atlas mapping JSON string directly from package.
    pub fn load_atlas_mapping(&self) -> PyResult<String> {
        self.inner
            .load_atlas_mapping_json()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Reads and decodes a texture chunk directly in memory into RGBA8 bytes: `(width, height, memoryview)`.
    /// `flip_v`: If True, flips rows vertically (V-axis inversion) natively in Rust. Defaults to true for Blender layout.
    #[pyo3(signature = (chunk_id, flip_v=true))]
    pub fn read_texture_rgba<'py>(
        &self,
        py: Python<'py>,
        chunk_id: &str,
        flip_v: bool,
    ) -> PyResult<(u32, u32, Bound<'py, pyo3::types::PyMemoryView>)> {
        let buf = self
            .inner
            .read_texture_rgba_with_orientation(chunk_id, flip_v)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let width = buf.width;
        let height = buf.height;
        let py_bytes = pyo3::types::PyBytes::new(py, &buf.pixels);
        let memview = pyo3::types::PyMemoryView::from(&py_bytes)?;
        Ok((width, height, memview))
    }

    /// Reads and decodes a texture chunk directly in memory into normalized float32 pixel memoryview: `(width, height, memoryview)`.
    /// `flip_v`: If True (default True for Blender Image.pixels convention), flips rows vertically natively in Rust.
    #[pyo3(signature = (chunk_id, flip_v=true))]
    pub fn read_texture_f32<'py>(
        &self,
        py: Python<'py>,
        chunk_id: &str,
        flip_v: bool,
    ) -> PyResult<(u32, u32, Bound<'py, pyo3::types::PyMemoryView>)> {
        let (width, height, f32_pixels) = self
            .inner
            .read_texture_f32_with_orientation(chunk_id, flip_v)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let memview = crate::buffer::create_owned_f32_memoryview(py, f32_pixels)?;
        Ok((width, height, memview))
    }
}

/// Checks if a `.mtkcache` file at `path` is valid for `fingerprint` using 64-byte header.
#[pyfunction]
pub fn is_valid_cache_package(path: &str, fingerprint: &str) -> bool {
    libmtk::AssetCacheReader::is_valid_cache_file(path, fingerprint)
}

/// Reads the manifest of a cache package or searches for one in a directory.
#[pyfunction]
pub fn get_cache_manifest<'py>(
    py: Python<'py>,
    path: &str,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let p = std::path::Path::new(path);
    if let Some(m) = libmtk::CacheManifest::read_from_dir(p) {
        let dict = PyDict::new(py);
        dict.set_item("format_version", m.format_version)?;
        dict.set_item("fingerprint", m.fingerprint)?;
        dict.set_item("pack_count", m.pack_count)?;
        dict.set_item("atlas_chunks", m.atlas_chunks)?;
        dict.set_item("standalone_textures", m.standalone_textures)?;
        dict.set_item("baked_models", m.baked_models)?;
        dict.set_item("created_at_epoch_secs", m.created_at_epoch_secs)?;
        Ok(Some(dict))
    } else {
        Ok(None)
    }
}

/// Executes unified end-to-end asset precompilation directly to persistent cache folder.
///
/// Releases Python GIL during multi-threaded baking, re-acquiring for progress callbacks.
#[pyfunction]
#[pyo3(signature = (stack, cache_dir, atlas_category="blocks", max_atlas_width=4096, max_atlas_height=4096, compile_atlas=true, compile_models=true, num_threads=None, callback=None, resolver_overrides_json=None))]
pub fn precompile_all_assets<'py>(
    py: Python<'py>,
    stack: &PyResourcePackStack,
    cache_dir: &str,
    atlas_category: &str,
    max_atlas_width: u32,
    max_atlas_height: u32,
    compile_atlas: bool,
    compile_models: bool,
    num_threads: Option<usize>,
    callback: Option<Py<PyAny>>,
    resolver_overrides_json: Option<&str>,
) -> PyResult<PyPrecompileResult> {
    let resolver_overrides = match resolver_overrides_json {
        Some(json) => Some(
            mtk_material::resolver::overrides::parse_overrides(json)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        ),
        None => None,
    };
    let cfg = libmtk::PrecompileConfig {
        max_atlas_width,
        max_atlas_height,
        atlas_category: atlas_category.to_string(),
        compile_atlas,
        compile_models,
        num_threads,
        resolver_overrides,
    };

    let cb_opt = callback.as_ref();
    let on_progress = cb_opt.map(|cb| {
        move |prog: mtk_core::progress::ProgressReport| {
            Python::attach(|py| {
                let dict = PyDict::new(py);
                let _ = dict.set_item("stage", prog.stage);
                let _ = dict.set_item("current", prog.current);
                let _ = dict.set_item("total", prog.total);
                let _ = dict.set_item("message", &prog.message);
                let _ = dict.set_item("percent", prog.percent());
                let _ = cb.call1(py, (dict,));
            });
        }
    });
    let progress_ref: Option<mtk_core::progress::ProgressCallback<'_>> = on_progress
        .as_ref()
        .map(|f| f as &(dyn Fn(mtk_core::progress::ProgressReport) + Send + Sync));

    let res = py
        .detach(|| {
            libmtk::precompile_all_assets_with_progress(&stack.inner, cache_dir, &cfg, progress_ref)
        })
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PyPrecompileResult {
        success: res.success,
        pack_count: res.pack_count,
        atlas_chunks: res.atlas_chunks,
        baked_models: res.baked_models,
        fingerprint: res.fingerprint,
        package_path: res.package_path,
        cache_dir: res.cache_dir,
        profile: res.profile,
    })
}

/// Resolve the active `.mtkcache` package for a cache directory + optional stack fingerprint.
#[pyfunction]
#[pyo3(signature = (cache_dir, fingerprint=None))]
pub fn resolve_active_cache_package(cache_dir: &str, fingerprint: Option<&str>) -> Option<String> {
    libmtk::resolve_active_cache_package(cache_dir, fingerprint)
        .map(|path| path.to_string_lossy().into_owned())
}
