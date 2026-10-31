//! Test-only APIs exposed under `unstable.testing`.
//!
//! These entry points are kept out of the stable top-level namespace. Host
//! production code must not depend on them.

use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::model::PyModelBaker;
use crate::texture::atlas::PyAtlasBuilder;
use crate::voxel::PyVoxelStorage;
use crate::{protocol, subdivide, texture, uv};

/// Helper function for unit testing: creates a minimal valid `.mtkcache` package file.
#[pyfunction]
#[pyo3(signature = (path, fingerprint, created_at=0))]
pub fn create_test_cache_package(
    path: &str,
    fingerprint: &str,
    created_at: u64,
) -> PyResult<String> {
    let p = std::path::Path::new(path);
    let fp_bytes = libmtk::fingerprint_str_to_bytes16(fingerprint);
    let mut writer = mtk_package::MtkPackageFileWriter::create(
        p,
        mtk_package::PackageProfile::AssetCache,
        fp_bytes,
    )
    .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    let manifest = libmtk::CacheManifest {
        format_version: libmtk::ASSET_CACHE_FORMAT_VERSION,
        fingerprint: fingerprint.to_string(),
        pack_count: 1,
        atlas_chunks: 1,
        standalone_textures: 0,
        baked_models: 50,
        created_at_epoch_secs: created_at,
    };
    let json_str = serde_json::to_string(&manifest)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    writer
        .add_str_chunk(
            *b"META",
            "manifest",
            &json_str,
            &mtk_package::ChunkWriteOptions::zstd_fast(),
        )
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    Ok(p.to_string_lossy().to_string())
}

/// Returns the number of dirty sections (test-only low-level accessor).
#[pyfunction]
pub fn voxel_dirty_section_count(storage: &PyVoxelStorage) -> usize {
    storage.inner.dirty_sections.len()
}

/// Ingests a section snapshot (test-only low-level accessor).
#[pyfunction]
#[pyo3(signature = (
    storage,
    sec_x,
    sec_y,
    sec_z,
    start_x,
    start_y,
    start_z,
    size_x,
    size_y,
    size_z,
    palette,
    grid_indices,
    biome_palette=None,
    biome_indices=None
))]
#[allow(clippy::too_many_arguments)]
pub fn voxel_set_section_snapshot(
    storage: &mut PyVoxelStorage,
    sec_x: i32,
    sec_y: i32,
    sec_z: i32,
    start_x: i32,
    start_y: i32,
    start_z: i32,
    size_x: i32,
    size_y: i32,
    size_z: i32,
    palette: Vec<String>,
    grid_indices: Vec<u16>,
    biome_palette: Option<Vec<String>>,
    biome_indices: Option<Vec<u16>>,
) -> bool {
    storage.inner.set_section_snapshot(
        sec_x,
        sec_y,
        sec_z,
        start_x,
        start_y,
        start_z,
        size_x,
        size_y,
        size_z,
        &palette,
        &grid_indices,
        biome_palette.as_deref(),
        biome_indices.as_deref(),
    )
}

/// Returns all populated non-air blocks (test-only low-level accessor).
#[pyfunction]
pub fn voxel_get_all_blocks<'py>(py: Python<'py>, storage: &PyVoxelStorage) -> Bound<'py, PyList> {
    let list = PyList::empty(py);
    for (x, y, z, state) in storage.inner.get_all_blocks() {
        let _ = list.append(((x, y, z), state));
    }
    list
}

/// Registers the `testing` module on `parent_module`.
pub fn register_submodule(py: Python<'_>, parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sub = PyModule::new(py, "testing")?;

    // Fixtures.
    sub.add_function(wrap_pyfunction!(create_test_cache_package, &sub)?)?;

    // Test-only classes.
    sub.add_class::<PyAtlasBuilder>()?;
    sub.add_class::<PyModelBaker>()?;

    // Split / UV / fluid / texture analyzers.
    sub.add_function(wrap_pyfunction!(
        subdivide::calculate_face_target_grid,
        &sub
    )?)?;
    sub.add_function(wrap_pyfunction!(
        subdivide::calculate_pixel_grid_cut_factors,
        &sub
    )?)?;
    sub.add_function(wrap_pyfunction!(subdivide::slice_face_by_pixel_grid, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::is_uv_collapsed, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::is_orthogonal_angle, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::detect_uv_rotation, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::straighten_uv, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::normalize_uv_for_atlas_tiling, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::uv_requires_atlas_tiling, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::restore_atlas_tiling_uv, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::repair_quad_fluid_uv, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::batch_repair_fluid_uv, &sub)?)?;
    sub.add_function(wrap_pyfunction!(uv::repair_extruded_side_uv, &sub)?)?;
    sub.add_function(wrap_pyfunction!(texture::sample_uv_alpha_f32, &sub)?)?;
    sub.add_function(wrap_pyfunction!(texture::is_face_transparent_f32, &sub)?)?;

    // Protocol encoders are server-side/test-only from the DCC client's view.
    sub.add_function(wrap_pyfunction!(protocol::encode_welcome, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_baseline_begin, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_baseline_chunk, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_baseline_end, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_delta_op, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_palette_update, &sub)?)?;

    // Voxel low-level testing helpers.
    sub.add_function(wrap_pyfunction!(voxel_dirty_section_count, &sub)?)?;
    sub.add_function(wrap_pyfunction!(voxel_set_section_snapshot, &sub)?)?;
    sub.add_function(wrap_pyfunction!(voxel_get_all_blocks, &sub)?)?;

    parent_module.add_submodule(&sub)?;
    Ok(())
}
