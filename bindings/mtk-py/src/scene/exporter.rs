//! Scene interchange package export bindings.

use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use libmtk::{
    export_scene_package as libmtk_export_scene, CtmMode, SceneExportRequest, SceneExporterConfig,
};
use mtk_voxel::SparseVoxelPayload;

use super::metadata::PySceneMetadata;
use crate::point_cloud::PyVoxelPointCloud;
use crate::resource::PyAssetCache;

/// End-to-end exporter: builds Scene Mini-Atlas, remaps UVs, tree-shakes models, and writes `.mtkscene`.
/// Supports structured `options` dictionary or keyword arguments.
#[pyfunction]
#[pyo3(signature = (output_path, point_cloud, asset_cache, options=None, **kwargs))]
pub fn export_scene_package<'py>(
    _py: Python<'py>,
    output_path: &str,
    point_cloud: &PyVoxelPointCloud,
    asset_cache: &mut PyAssetCache,
    options: Option<&Bound<'py, PyDict>>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<PySceneMetadata> {
    let get_val = |key: &str| -> Option<Bound<'py, PyAny>> {
        if let Some(opts) = options {
            if let Ok(Some(val)) = opts.get_item(key) {
                if !val.is_none() {
                    return Some(val);
                }
            }
        }
        if let Some(kw) = kwargs {
            if let Ok(Some(val)) = kw.get_item(key) {
                if !val.is_none() {
                    return Some(val);
                }
            }
        }
        None
    };

    let max_atlas_size = match get_val("max_atlas_size") {
        Some(v) => v.extract::<u32>()?,
        None => 2048,
    };
    let mip_level = match get_val("mip_level") {
        Some(v) => v.extract::<u32>()?,
        None => 0,
    };

    let dynamic_entity_atlases: Option<HashMap<u16, Vec<u8>>> =
        match get_val("dynamic_entity_atlases") {
            Some(v) => {
                let map: HashMap<u16, Bound<'py, PyBytes>> = v.extract()?;
                Some(
                    map.into_iter()
                        .map(|(k, bytes)| (k, bytes.as_bytes().to_vec()))
                        .collect(),
                )
            }
            None => None,
        };

    let dynamic_entity_slots: Option<HashMap<String, (u16, u16)>> =
        match get_val("dynamic_entity_slots") {
            Some(v) => Some(v.extract()?),
            None => None,
        };

    let ctm_solver_bound: Option<Bound<'py, crate::ctm::PyCtmSolver>> = match get_val("ctm_solver")
    {
        Some(v) => Some(v.extract()?),
        None => None,
    };

    let enable_ctm: Option<bool> = match get_val("enable_ctm") {
        Some(v) => Some(v.extract()?),
        None => None,
    };

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
        max_atlas_size,
        mip_level,
    };

    let solver_ref = ctm_solver_bound.as_ref().map(|s| s.borrow());
    let resolved_ctm_mode = if let Some(ref s) = solver_ref {
        if enable_ctm == Some(false) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Conflicting CTM arguments: explicit ctm_solver was provided, but enable_ctm is False",
            ));
        }
        CtmMode::Explicit(s.inner.rules.as_slice())
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
        dynamic_entity_atlases: dynamic_entity_atlases.as_ref(),
        dynamic_entity_slots: dynamic_entity_slots.as_ref(),
        ctm_mode: resolved_ctm_mode,
    };

    let result = libmtk_export_scene(output_path, request)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PySceneMetadata {
        inner: result.metadata,
    })
}
