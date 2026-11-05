//! Test-only APIs exposed under `unstable.testing`.
//!
//! These entry points are kept out of the stable top-level namespace. Host
//! production code must not depend on them.

use pyo3::prelude::*;

use crate::attributes;
use crate::model::{PyBakedModelDatabase, PyModelBaker};
use crate::resource::PyAssetCache;
use crate::texture::atlas::PyAtlasBuilder;
use crate::texture::PyBakedAtlas;
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

/// Helper to extract atlas texture from asset cache.
#[pyfunction]
pub fn extract_cache_atlas_texture(
    cache: &PyAssetCache,
    filename: &str,
    output_path: &str,
) -> PyResult<()> {
    cache
        .inner
        .extract_atlas_texture(filename, output_path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
}

/// Test helper to get all blockstate keys from model database.
#[pyfunction]
pub fn model_database_get_states(db: &PyBakedModelDatabase) -> Vec<String> {
    db.inner.keys().cloned().collect()
}

/// Test helper to remap model database to atlas.
#[pyfunction]
pub fn model_database_remap_to_atlas(
    py: Python<'_>,
    db: &mut PyBakedModelDatabase,
    atlas: &PyBakedAtlas,
) {
    let atlas_map = &atlas.inner.address_map;
    py.detach(|| {
        if let Some(inner) = std::sync::Arc::get_mut(&mut db.inner) {
            inner.remap_to_atlas_with(|tex| {
                mtk_material::MaterialResolver::resolve(tex, None, atlas_map)
                    .map(|(_, sp)| (sp.frame_0_uv_bounds, sp.chunk_id, sp.texture_id))
            });
        } else {
            let mut cloned = (*db.inner).clone();
            cloned.remap_to_atlas_with(|tex| {
                mtk_material::MaterialResolver::resolve(tex, None, atlas_map)
                    .map(|(_, sp)| (sp.frame_0_uv_bounds, sp.chunk_id, sp.texture_id))
            });
            db.inner = std::sync::Arc::new(cloned);
        }
    });
}

/// Test helper to get all non-empty sections from voxel storage.
#[pyfunction]
pub fn voxel_storage_get_non_empty_sections<'py>(
    py: Python<'py>,
    storage: &PyVoxelStorage,
) -> Bound<'py, pyo3::types::PyList> {
    let list = pyo3::types::PyList::empty(py);
    for coord in storage.inner.get_all_non_empty_sections() {
        let _ = list.append((coord.x, coord.y, coord.z));
    }
    list
}

/// Test helper to get block entity JSON from voxel storage.
#[pyfunction]
pub fn voxel_storage_get_block_entity_json(
    storage: &PyVoxelStorage,
    x: i32,
    y: i32,
    z: i32,
) -> Option<String> {
    storage
        .inner
        .get_block_entity(x, y, z)
        .and_then(|be| serde_json::to_string(be).ok())
}

/// Test helper to set block entity JSON on voxel storage.
#[pyfunction]
#[pyo3(signature = (storage, x, y, z, json_str=None))]
pub fn voxel_storage_set_block_entity_json(
    storage: &mut PyVoxelStorage,
    x: i32,
    y: i32,
    z: i32,
    json_str: Option<&str>,
) -> PyResult<()> {
    let entity = if let Some(s) = json_str {
        let parsed: mtk_model::BlockEntityData = serde_json::from_str(s)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Some(parsed)
    } else {
        None
    };
    storage.inner.set_block_entity(x, y, z, entity);
    Ok(())
}

/// Test helper to synchronously ensure all dynamic entities in a VoxelWorld are resolved.
#[pyfunction]
pub fn voxel_world_ensure_dynamic_entities_resolved_sync(
    world: &mut crate::voxel::world::PyVoxelWorld,
) -> bool {
    world.inner.ensure_dynamic_entities_resolved_sync()
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

    // Protocol / Packet
    sub.add_function(wrap_pyfunction!(protocol::decode_packet, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_welcome, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_baseline_begin, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_baseline_chunk, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_baseline_end, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_delta_op, &sub)?)?;
    sub.add_function(wrap_pyfunction!(protocol::encode_palette_update, &sub)?)?;

    // Mesh attributes schema
    sub.add_function(wrap_pyfunction!(attributes::mesh_attribute_schema, &sub)?)?;
    sub.add_function(wrap_pyfunction!(attributes::retired_mesh_attributes, &sub)?)?;

    // Cache / Textures
    sub.add_function(wrap_pyfunction!(extract_cache_atlas_texture, &sub)?)?;

    // Model database helpers
    sub.add_function(wrap_pyfunction!(model_database_get_states, &sub)?)?;
    sub.add_function(wrap_pyfunction!(model_database_remap_to_atlas, &sub)?)?;

    // Voxel storage testing helpers
    sub.add_function(wrap_pyfunction!(voxel_dirty_section_count, &sub)?)?;
    sub.add_function(wrap_pyfunction!(voxel_set_section_snapshot, &sub)?)?;
    sub.add_function(wrap_pyfunction!(
        voxel_storage_get_non_empty_sections,
        &sub
    )?)?;
    sub.add_function(wrap_pyfunction!(voxel_storage_get_block_entity_json, &sub)?)?;
    sub.add_function(wrap_pyfunction!(voxel_storage_set_block_entity_json, &sub)?)?;
    sub.add_function(wrap_pyfunction!(
        voxel_world_ensure_dynamic_entities_resolved_sync,
        &sub
    )?)?;

    parent_module.add_submodule(&sub)?;
    Ok(())
}
