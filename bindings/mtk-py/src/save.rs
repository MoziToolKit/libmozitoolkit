//! # `mtk-py` Minecraft Save Loader Bindings
//!
//! Exposes `mtk-save` level metadata, spatial chunk querying, and direct
//! world meshing pipelines to Python.

use glam::IVec3;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

use mtk_save::{LevelData, SaveLoader};
use mtk_voxel::{VoxelStorage, VoxelWorld};

use crate::cull::PyFaceCuller;
use crate::mesh::PyMeshData;
use crate::model::PyBakedModelDatabase;
use crate::progress::PyProgressCallback;
use crate::threading::run_detached;
use crate::voxel::{PyMesherConfig, PyVoxelStorage};

/// Python wrapper for Minecraft world metadata from `level.dat`.
#[pyclass(name = "LevelData")]
#[derive(Debug, Clone)]
pub struct PyLevelData {
    pub(crate) inner: LevelData,
    pub(crate) dimensions: Vec<String>,
}

#[pymethods]
impl PyLevelData {
    #[getter]
    pub fn level_name(&self) -> &str {
        &self.inner.level_name
    }

    #[getter]
    pub fn version_name(&self) -> &str {
        &self.inner.version_name
    }

    #[getter]
    pub fn data_version(&self) -> i32 {
        self.inner.data_version
    }

    #[getter]
    pub fn spawn(&self) -> (i32, i32, i32) {
        (self.inner.spawn.x, self.inner.spawn.y, self.inner.spawn.z)
    }

    #[getter]
    pub fn time(&self) -> i64 {
        self.inner.time
    }

    #[getter]
    pub fn day_time(&self) -> i64 {
        self.inner.day_time
    }

    #[getter]
    pub fn hardcore(&self) -> bool {
        self.inner.hardcore
    }

    #[getter]
    pub fn game_type(&self) -> i32 {
        self.inner.game_type
    }

    #[getter]
    pub fn dimensions(&self) -> Vec<String> {
        self.dimensions.clone()
    }

    /// Converts metadata into a Python dictionary.
    pub fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("level_name", &self.inner.level_name)?;
        dict.set_item("version_name", &self.inner.version_name)?;
        dict.set_item("data_version", self.inner.data_version)?;
        dict.set_item(
            "spawn",
            (self.inner.spawn.x, self.inner.spawn.y, self.inner.spawn.z),
        )?;
        dict.set_item("time", self.inner.time)?;
        dict.set_item("day_time", self.inner.day_time)?;
        dict.set_item("hardcore", self.inner.hardcore)?;
        dict.set_item("game_type", self.inner.game_type)?;
        dict.set_item("dimensions", self.dimensions.clone())?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "<LevelData name=\"{}\" ver=\"{}\" dv={} spawn=({}, {}, {}) dims={:?}>",
            self.inner.level_name,
            self.inner.version_name,
            self.inner.data_version,
            self.inner.spawn.x,
            self.inner.spawn.y,
            self.inner.spawn.z,
            self.dimensions
        )
    }
}

/// Inspects a Minecraft save folder, reading `level.dat` and discovering available dimensions.
#[pyfunction]
pub fn inspect_minecraft_save(world_dir: &str) -> PyResult<PyLevelData> {
    let path = Path::new(world_dir);
    let level_data = SaveLoader::read_level_data(path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    let dimensions = SaveLoader::discover_dimensions(path);

    Ok(PyLevelData {
        inner: level_data,
        dimensions,
    })
}

/// End-to-end pipeline: Loads bounded 3D world selection from save, feeds into `VoxelWorld`,
/// and meshes into `PyMeshData` with Atlas UVs and Ambient Occlusion.
#[pyfunction]
#[pyo3(signature = (
    world_dir,
    dimension="overworld",
    min_block=(-64, -64, -64),
    max_block=(64, 320, 64),
    config=None,
    culler=None,
    model_db=None,
    num_threads=None,
    callback=None,
    skin_cache_dir=None,
    cache_package_path=None,
    force=false
))]
pub fn load_and_mesh_minecraft_save(
    py: Python<'_>,
    world_dir: &str,
    dimension: &str,
    min_block: (i32, i32, i32),
    max_block: (i32, i32, i32),
    config: Option<&PyMesherConfig>,
    culler: Option<&PyFaceCuller>,
    model_db: Option<&PyBakedModelDatabase>,
    num_threads: Option<usize>,
    callback: Option<Py<PyAny>>,
    skin_cache_dir: Option<String>,
    cache_package_path: Option<String>,
    force: Option<bool>,
) -> PyResult<(PyMeshData, PyLevelData, PyVoxelStorage)> {
    let mut profiler = mtk_core::ExecutionProfiler::start();

    let path = Path::new(world_dir);
    let (level_data, dimensions) = profiler.measure("read_level_metadata", "io", |_| {
        let ld = SaveLoader::read_level_data(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        let dims = SaveLoader::discover_dimensions(path);
        Ok::<_, PyErr>((ld, dims))
    })?;

    let min_coord = IVec3::new(min_block.0, min_block.1, min_block.2);
    let max_coord = IVec3::new(max_block.0, max_block.1, max_block.2);

    let progress_cb = callback.as_ref().map(PyProgressCallback::new);
    let on_progress = progress_cb
        .as_ref()
        .map(PyProgressCallback::as_core_callback);
    let progress_ref: Option<mtk_core::ProgressCallback<'_>> = on_progress.as_deref();

    let mut storage = VoxelStorage::new();
    // Must detach: the loader decodes regions on rayon workers, and any progress callback
    // re-attaches to Python. Holding the GIL here deadlocks the process the moment the first
    // progress tick fires from a worker (observed as "import save freezes Blender").
    let _loaded_sections = profiler.measure("load_region_chunks", "io", |_| {
        run_detached(py, || {
            SaveLoader::load_box_into_storage_with_progress(
                path,
                dimension,
                min_coord,
                max_coord,
                &mut storage,
                progress_ref,
            )
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    })?;

    // Asset cache input gate: valid model database and atlas mapping are strictly required
    if model_db.is_none() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Active asset cache (model_db) is strictly required to mesh Minecraft save geometry",
        ));
    }
    if config.is_none()
        || config
            .and_then(|c| c.inner.atlas_address_map.as_ref())
            .is_none()
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Active asset cache (atlas) is strictly required to mesh Minecraft save geometry",
        ));
    }

    let force_proceed = force.unwrap_or(false);
    if !force_proceed {
        let unknown = storage.scan_unknown_blocks(
            model_db.map(|m| m.inner.as_ref()),
            config.and_then(|c| c.inner.atlas_address_map.as_deref()),
            None,
        );
        if !unknown.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Found {} unknown block(s) without models or textures: {}. To proceed with fallback cubes, set force=True.",
                unknown.len(),
                unknown.join(", ")
            )));
        }
    }

    let cfg = config.map(|c| c.inner.clone());
    let cul = culler.map(|c| c.inner.clone());
    let mdb = model_db.map(|db| db.inner.clone());
    let threads = num_threads.or_else(|| config.and_then(|c| c.num_threads));
    let mut world = VoxelWorld::from_storage(storage.clone(), cfg, cul, mdb);
    world.num_threads = threads;
    crate::voxel::world::attach_block_entity_textures(&mut world, config);
    if let Some(ref dir) = skin_cache_dir {
        world.set_skin_cache_dir(Some(std::path::PathBuf::from(dir)));
    }
    if let Some(ref pkg_path) = cache_package_path {
        if let Ok(reader) = libmtk::AssetCacheReader::open(pkg_path) {
            if let Ok(baker) = reader.load_banner_baker() {
                world.set_banner_baker(baker);
            }
        }
    }

    let (mesh, _world_profile, dynamic_entity_atlases, dynamic_entity_slots, _active_config) =
        profiler.measure("mesh_voxel_world", "mesh", |p| {
            let res = run_detached(py, || {
                world.ensure_dynamic_entities_resolved_sync_with_progress(progress_ref);
                let m = world.rebuild_all_with_progress(progress_ref)?.clone();
                let prof = world.last_profile.clone();
                let (dyn_atlases, dyn_slots) = world.dynamic_entity_atlases_if_used();
                Ok::<_, mtk_voxel::VoxelError>((
                    m,
                    prof,
                    dyn_atlases,
                    dyn_slots,
                    world.config.clone(),
                ))
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            if let Some(wp) = res.1.clone() {
                p.attach_sub_profile(wp, Some("mesh"));
            }

            Ok::<_, PyErr>(res)
        })?;

    let final_profile = profiler.finish();

    Ok((
        PyMeshData {
            inner: mesh,
            profile: Some(final_profile),
            dynamic_entity_atlases,
            dynamic_entity_slots,
            materials_are_slots: true,

            views: Default::default(),
        },
        PyLevelData {
            inner: level_data,
            dimensions,
        },
        PyVoxelStorage { inner: storage },
    ))
}
