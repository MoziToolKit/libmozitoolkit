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

/// Loads a bounded 3D selection from an Anvil save directly into a `VoxelStorage` without requiring model meshing.
#[pyfunction]
#[pyo3(signature = (
    world_dir,
    dimension="overworld",
    min_block=(0, -64, 0),
    max_block=(15, 64, 15),
))]
pub fn load_minecraft_save_storage(
    world_dir: &str,
    dimension: &str,
    min_block: (i32, i32, i32),
    max_block: (i32, i32, i32),
) -> PyResult<PyVoxelStorage> {
    let path = Path::new(world_dir);
    let min_coord = IVec3::new(min_block.0, min_block.1, min_block.2);
    let max_coord = IVec3::new(max_block.0, max_block.1, max_block.2);
    let mut storage = VoxelStorage::new();
    SaveLoader::load_box_into_storage(path, dimension, min_coord, max_coord, &mut storage)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(PyVoxelStorage { inner: storage })
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
    let (level_data, dimensions) = profiler.measure("read_level_metadata", || {
        let ld = SaveLoader::read_level_data(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        let dims = SaveLoader::discover_dimensions(path);
        Ok::<_, PyErr>((ld, dims))
    })?;

    let min_coord = IVec3::new(min_block.0, min_block.1, min_block.2);
    let max_coord = IVec3::new(max_block.0, max_block.1, max_block.2);

    let cb_opt = callback.as_ref();
    let on_progress = cb_opt.map(|cb| {
        move |prog: mtk_core::ProgressReport| {
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
    let progress_ref: Option<mtk_core::ProgressCallback<'_>> = on_progress
        .as_ref()
        .map(|f| f as &(dyn Fn(mtk_core::ProgressReport) + Send + Sync));

    let mut storage = VoxelStorage::new();
    let loaded_sections = profiler.measure("load_region_chunks", || {
        SaveLoader::load_box_into_storage_with_progress(
            path,
            dimension,
            min_coord,
            max_coord,
            &mut storage,
            progress_ref,
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
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

    let mut world = VoxelWorld::from_storage_with_threads(storage.clone(), cfg, cul, mdb, threads);
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

    let (mesh, world_profile, dynamic_entity_atlases, dynamic_entity_slots) = py
        .detach(|| {
            world.ensure_dynamic_entities_resolved_sync_with_progress(progress_ref);
            let m = world.rebuild_all_with_progress(progress_ref)?.clone();
            let p = world.last_profile.clone();
            let (dyn_atlases, dyn_slots) = world.dynamic_entity_atlases_if_used();
            Ok::<_, mtk_voxel::VoxelError>((m, p, dyn_atlases, dyn_slots))
        })
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    if let Some(wp) = world_profile {
        for stage in wp.stages {
            profiler.record_stage(stage.name, stage.duration_us, stage.items_count);
        }
    } else {
        profiler.record_stage("mesh_voxel_world", 0, Some(loaded_sections));
    }

    let final_profile = profiler.finish();

    Ok((
        PyMeshData {
            inner: mesh,
            profile: Some(final_profile),
            dynamic_entity_atlases,
            dynamic_entity_slots,
        },
        PyLevelData {
            inner: level_data,
            dimensions,
        },
        PyVoxelStorage { inner: storage },
    ))
}
