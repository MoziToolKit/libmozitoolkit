//! # `mtk-py` (Python Bindings for libmozitoolkit)
//!
//! Exposes `libmtk` data structures and high-performance algorithms to Python via PyO3.

pub mod attributes;
pub mod ctm;
pub mod cull;
pub mod extrude;
pub mod material;
pub mod mesh;
pub mod model;
pub mod point_cloud;
pub mod protocol;
pub mod resource;
pub mod save;
pub mod scene;
pub mod subdivide;
pub mod sync;
pub mod texture;
pub mod uv;
pub mod voxel;

pub mod buffer;
pub mod profile;
pub mod unstable;

use pyo3::prelude::*;

/// Integer API version understood by this native extension.
pub const API_VERSION: u32 = 2;

pub use ctm::PyCtmSolver;
pub use cull::PyFaceCuller;
pub use material::{
    build_material_palette, compute_biome_tint_attributes, compute_biome_tint_attributes_paletted,
    compute_mesh_biome_attributes_from_storage, get_all_biomes, get_biome_meta,
    reapply_uniform_biome_tint, PyBiomeResolver, PyGridAtlasSpec, PyMaterialResolver,
    PyVanillaColormaps,
};
pub use mesh::{PyAttributeDomain, PyMeshData};
pub use model::PyBakedModelDatabase;
pub use point_cloud::PyVoxelPointCloud;
pub use profile::{PyExecutionProfile, PyStageSpan};
pub use resource::{
    evaluate_cache_status, path_provides_vanilla_client, precompile_all_assets, probe_pack,
    resolve_active_cache_package, PyAssetCache, PyPrecompileResult, PyResourcePackStack,
};
pub use save::PyLevelData;
pub use scene::{export_scene_package, PySceneMetadata, PyScenePackageReader};
pub use sync::PyLiveSyncSession;
pub use texture::PyBakedAtlas;
pub use voxel::{PyMesherConfig, PyVoxelStorage, PyVoxelWorld};

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
    // 1. Profiling & Diagnostics
    m.add_class::<profile::PyStageSpan>()?;
    m.add_class::<profile::PyExecutionProfile>()?;

    // 2. Mesh & Core
    m.add_class::<PyAttributeDomain>()?;
    m.add_class::<PyMeshData>()?;
    m.add_function(wrap_pyfunction!(subdivide::adaptive_pixel_split_mesh, m)?)?;
    m.add_function(wrap_pyfunction!(mesh::analyze_mesh_hard_edges, m)?)?;

    // 3. Culling
    m.add_class::<PyFaceCuller>()?;

    // 4. Voxel Storage & Config & World & Point Cloud
    m.add_class::<PyVoxelStorage>()?;
    m.add_class::<PyMesherConfig>()?;
    m.add_class::<PyVoxelWorld>()?;
    m.add_class::<PyVoxelPointCloud>()?;

    // 5. Live Sync & Protocol
    m.add_class::<PyLiveSyncSession>()?;

    // 6. Resource Pack & Textures
    m.add_class::<PyResourcePackStack>()?;
    m.add_class::<PyCtmSolver>()?;
    m.add_class::<PyBakedAtlas>()?;
    m.add_class::<PyPrecompileResult>()?;
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

    // 7. Material & UV Remapper & Biome
    m.add_class::<PyGridAtlasSpec>()?;
    m.add_class::<PyMaterialResolver>()?;
    m.add_class::<PyBiomeResolver>()?;
    m.add_class::<PyVanillaColormaps>()?;
    m.add_function(wrap_pyfunction!(build_material_palette, m)?)?;
    m.add_function(wrap_pyfunction!(compute_biome_tint_attributes, m)?)?;
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

    // 8. Model Baker
    m.add_class::<PyBakedModelDatabase>()?;

    // 9. UV Geometry Algorithms & Extrude
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

    // 10. Minecraft Save Loader
    m.add_class::<save::PyLevelData>()?;
    m.add_function(wrap_pyfunction!(save::inspect_minecraft_save, m)?)?;
    m.add_function(wrap_pyfunction!(save::load_and_mesh_minecraft_save, m)?)?;

    // 11. Scene Interchange (.mtkscene)
    m.add_class::<scene::PySceneMetadata>()?;
    m.add_class::<scene::PyScenePackageReader>()?;
    m.add_function(wrap_pyfunction!(scene::export_scene_package, m)?)?;

    // 12. Metadata
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(api_version, m)?)?;

    // 13. Unstable and experimental APIs
    {
        let py = m.py();
        unstable::register_submodule(py, m)?;
    }

    Ok(())
}


#[cfg(test)]
mod tests;
