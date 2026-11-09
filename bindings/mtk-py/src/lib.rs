//! # `mtk-py` (Python Bindings for libmozitoolkit)
//!
//! Exposes `libmtk` data structures and high-performance algorithms to Python via PyO3.

mod attributes;
mod ctm;
mod cull;
mod extrude;
mod material;
mod mesh;
mod model;
mod point_cloud;
mod protocol;
mod resource;
mod save;
mod scene;
mod subdivide;
mod sync;
mod texture;
mod uv;
mod voxel;

mod buffer;
mod profile;
mod unstable;

use pyo3::prelude::*;

/// Integer API version understood by this native extension.
pub const API_VERSION: u32 = 2;

pub(crate) use ctm::PyCtmSolver;
pub(crate) use cull::PyFaceCuller;
pub(crate) use material::{
    build_material_palette, compute_biome_tint_attributes_paletted,
    compute_mesh_biome_attributes_from_storage, get_all_biomes, get_biome_meta,
    reapply_uniform_biome_tint, PyBiomeResolver, PyGridAtlasSpec, PyMaterialResolver,
    PyVanillaColormaps,
};
pub(crate) use mesh::{PyAttributeDomain, PyMeshData};
pub(crate) use model::PyBakedModelDatabase;
pub(crate) use point_cloud::PyVoxelPointCloud;
pub(crate) use resource::{
    evaluate_cache_status, path_provides_vanilla_client, precompile_all_assets, probe_pack,
    resolve_active_cache_package, PyAssetCache, PyResourcePackStack,
};
pub(crate) use sync::PyLiveSyncSession;
pub(crate) use texture::PyBakedAtlas;
pub(crate) use voxel::{PyMesherConfig, PyVoxelStorage, PyVoxelWorld};

/// Returns libmtk version string.
#[pyfunction]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the integer API contract version expected by the Blender glue layer.
#[pyfunction]
pub fn api_version() -> u32 {
    API_VERSION
}

/// Normalizes a legacy banner pattern short-code or identifier to standard pattern stem.
#[pyfunction]
pub fn normalize_banner_pattern(code_or_name: &str) -> &'static str {
    mtk_resource::banner::normalize_banner_pattern(code_or_name)
}

/// Cleans a raw DCC material or texture name into a normalized identifier stem.
#[pyfunction]
pub fn clean_material_identifier(raw: &str) -> String {
    mtk_material::clean_identifier(raw)
}

/// Returns the built-in list of exporter biome suffixes (Mineways / jmc2obj and similar).
#[pyfunction]
pub fn builtin_biome_suffixes() -> Vec<String> {
    mtk_material::builtin_biome_suffixes().to_vec()
}

/// Main entrypoint for Python module `libmtk_py`.
#[pymodule]
fn libmtk_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // 1. Mesh & Core
    m.add_class::<PyAttributeDomain>()?;
    m.add_class::<PyMeshData>()?;
    m.add_function(wrap_pyfunction!(subdivide::adaptive_pixel_split_mesh, m)?)?;
    m.add_function(wrap_pyfunction!(mesh::analyze_mesh_hard_edges, m)?)?;

    // 2. Culling
    m.add_class::<PyFaceCuller>()?;

    // 3. Voxel Storage & Config & World & Point Cloud
    m.add_class::<PyVoxelStorage>()?;
    m.add_class::<PyMesherConfig>()?;
    m.add_class::<PyVoxelWorld>()?;
    m.add_class::<PyVoxelPointCloud>()?;

    // 4. Live Sync & Protocol
    m.add_class::<PyLiveSyncSession>()?;

    // 5. Resource Pack & Textures
    m.add_class::<PyResourcePackStack>()?;
    m.add_class::<PyCtmSolver>()?;
    m.add_class::<PyBakedAtlas>()?;
    m.add_class::<PyAssetCache>()?;
    m.add_function(wrap_pyfunction!(precompile_all_assets, m)?)?;
    m.add_function(wrap_pyfunction!(normalize_banner_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(clean_material_identifier, m)?)?;
    m.add_function(wrap_pyfunction!(builtin_biome_suffixes, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_active_cache_package, m)?)?;
    m.add_function(wrap_pyfunction!(probe_pack, m)?)?;
    m.add_function(wrap_pyfunction!(path_provides_vanilla_client, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate_cache_status, m)?)?;
    m.add_function(wrap_pyfunction!(
        texture::batch_analyze_transparent_faces_f32,
        m
    )?)?;

    // 6. Material & UV Remapper & Biome
    m.add_class::<PyGridAtlasSpec>()?;
    m.add_class::<PyMaterialResolver>()?;
    m.add_class::<PyBiomeResolver>()?;
    m.add_class::<PyVanillaColormaps>()?;
    m.add_function(wrap_pyfunction!(build_material_palette, m)?)?;
    m.add_function(wrap_pyfunction!(compute_biome_tint_attributes_paletted, m)?)?;
    m.add_function(wrap_pyfunction!(
        compute_mesh_biome_attributes_from_storage,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(reapply_uniform_biome_tint, m)?)?;
    m.add_function(wrap_pyfunction!(get_biome_meta, m)?)?;
    m.add_function(wrap_pyfunction!(get_all_biomes, m)?)?;
    m.add_function(wrap_pyfunction!(material::sample_colormap_colors, m)?)?;
    m.add("TINT_TYPE_NONE", mtk_material::TINT_TYPE_NONE)?;
    m.add("TINT_TYPE_GRASS", mtk_material::TINT_TYPE_GRASS)?;
    m.add("TINT_TYPE_FOLIAGE", mtk_material::TINT_TYPE_FOLIAGE)?;
    m.add("TINT_TYPE_WATER", mtk_material::TINT_TYPE_WATER)?;
    m.add("TINT_TYPE_HARDCODED", mtk_material::TINT_TYPE_HARDCODED)?;
    m.add("TINT_TYPE_DRY_FOLIAGE", mtk_material::TINT_TYPE_DRY_FOLIAGE)?;

    // 7. Model Baker & Painting
    m.add_class::<PyBakedModelDatabase>()?;
    m.add_function(wrap_pyfunction!(model::build_painting_mesh, m)?)?;
    m.add_function(wrap_pyfunction!(model::get_painting_dimension, m)?)?;
    m.add_function(wrap_pyfunction!(model::get_painting_variant, m)?)?;
    m.add_function(wrap_pyfunction!(model::get_all_painting_variants, m)?)?;
    m.add_function(wrap_pyfunction!(model::calculate_painting_position, m)?)?;

    // 8. UV Geometry Algorithms & Extrude
    m.add_function(wrap_pyfunction!(uv::calculate_uv_area, m)?)?;
    m.add_function(wrap_pyfunction!(uv::get_uv_bounds, m)?)?;
    m.add_function(wrap_pyfunction!(uv::get_uv_center, m)?)?;
    m.add_function(wrap_pyfunction!(uv::scale_uv, m)?)?;
    m.add_function(wrap_pyfunction!(uv::repair_mesh_fluid_uv_inplace, m)?)?;
    m.add_function(wrap_pyfunction!(uv::get_fluid_top_uvs, m)?)?;
    m.add_function(wrap_pyfunction!(uv::get_fluid_side_uvs, m)?)?;
    m.add_function(wrap_pyfunction!(uv::generate_random_extrude_heights, m)?)?;
    m.add_function(wrap_pyfunction!(
        extrude::process_flat_mesh_extrude_repair,
        m
    )?)?;

    // 9. Minecraft Save Loader
    m.add_function(wrap_pyfunction!(save::inspect_minecraft_save, m)?)?;
    m.add_function(wrap_pyfunction!(save::load_minecraft_save_storage, m)?)?;
    m.add_function(wrap_pyfunction!(save::load_and_mesh_minecraft_save, m)?)?;

    // 10. Scene Interchange (.mtkscene)
    m.add_class::<scene::PyScenePackageReader>()?;
    m.add_function(wrap_pyfunction!(scene::export_scene_package, m)?)?;

    // 11. Metadata
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(api_version, m)?)?;

    // 12. Unstable and experimental APIs
    {
        let py = m.py();
        unstable::register_submodule(py, m)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
