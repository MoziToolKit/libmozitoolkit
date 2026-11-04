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
pub use protocol::{compute_canonical_state_hash, decode_packet};
pub use resource::{
    get_cache_manifest, is_valid_cache_package, precompile_all_assets,
    resolve_active_cache_package, PyAssetCache, PyPrecompileResult, PyResourcePackStack,
};
pub use save::PyLevelData;
pub use scene::{export_scene_package, PySceneMetadata, PyScenePackageReader, PyValidationReport};
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
    m.add_class::<buffer::PyBufferOwner>()?;
    m.add_function(wrap_pyfunction!(attributes::mesh_attribute_schema, m)?)?;
    m.add_function(wrap_pyfunction!(attributes::retired_mesh_attributes, m)?)?;
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
    m.add_function(wrap_pyfunction!(decode_packet, m)?)?;
    m.add_function(wrap_pyfunction!(compute_canonical_state_hash, m)?)?;

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
    m.add_function(wrap_pyfunction!(is_valid_cache_package, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_active_cache_package, m)?)?;
    m.add_function(wrap_pyfunction!(get_cache_manifest, m)?)?;
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
    m.add_function(wrap_pyfunction!(material::clear_cached_colormaps, m)?)?;
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
    m.add_class::<scene::PySceneCapabilities>()?;
    m.add_class::<scene::PyScenePackageReader>()?;
    m.add_class::<scene::PyValidationReport>()?;
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
mod tests {
    use super::*;

    #[test]
    fn test_python_mesh_data_inner() {
        Python::initialize();
        Python::attach(|py| {
            let pos = pyo3::types::PyList::new(
                py,
                [
                    0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0,
                ],
            )
            .unwrap();
            let loops = pyo3::types::PyList::new(py, [0u32, 1, 2, 3]).unwrap();
            let uvs =
                pyo3::types::PyList::new(py, [0.0f32, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]).unwrap();
            let starts = pyo3::types::PyList::new(py, [0u32]).unwrap();
            let totals = pyo3::types::PyList::new(py, [4u32]).unwrap();
            let mats = pyo3::types::PyList::new(py, [7u32]).unwrap();

            let mesh_data = PyMeshData::from_flat_polygons(
                pos.as_any(),
                loops.as_any(),
                uvs.as_any(),
                Some(starts.as_any()),
                Some(totals.as_any()),
                None,
                Some(mats.as_any()),
                None,
            )
            .unwrap();
            let mesh = Bound::new(py, mesh_data).unwrap();
            assert_eq!(mesh.borrow().vertex_count(), 4);
            assert_eq!(mesh.borrow().triangle_count(), 2);
            assert_eq!(mesh.borrow().face_count(), 1);

            let memview = PyMeshData::positions_memoryview(&mesh).unwrap();
            assert_eq!(memview.len().unwrap(), 4 * 3); // 4 verts * 3 floats
            assert_eq!(
                memview
                    .getattr("format")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "f"
            );

            let indices_mem = PyMeshData::indices_memoryview(&mesh).unwrap();
            assert_eq!(indices_mem.len().unwrap(), 6); // 6 indices
            assert_eq!(
                indices_mem
                    .getattr("format")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "I"
            );

            mesh.borrow_mut().clear();
            assert_eq!(mesh.borrow().vertex_count(), 0);
            assert!(mesh.borrow().is_empty());
        });
    }

    #[test]
    fn test_python_voxel_storage_and_world_mesher() {
        Python::initialize();
        Python::attach(|py| {
            let mut storage = PyVoxelStorage::new();
            storage.set_bounds(0, 0, 0, 16, 16, 16);
            storage.set_block(0, 0, 0, "minecraft:stone", None);
            assert_eq!(storage.get_block(0, 0, 0), "minecraft:stone");
            assert_eq!(storage.inner.dirty_sections.len(), 1);

            let config = PyMesherConfig::default();
            let mut world = PyVoxelWorld::from_storage(&storage, Some(&config), None, None, None);
            let mesh = world.rebuild_all(py, None).unwrap();
            assert!(!mesh.is_empty());
            assert_eq!(mesh.vertex_count(), 8); // 8 vertices for welded unit cube

            // Verify execution profile attached to mesh and world
            assert!(mesh.profile().is_some());
            let profile = mesh.profile().unwrap();
            let stage_names: Vec<String> = profile
                .stages()
                .iter()
                .map(|stage| stage.name().to_string())
                .collect();
            assert!(!stage_names.is_empty());
            assert!(stage_names
                .iter()
                .any(|name| name == "padded_array_extraction"));
            assert!(stage_names
                .iter()
                .any(|name| name == "parallel_section_meshing"));
            assert!(world.last_profile().is_some());

            let mut unwelded_config = PyMesherConfig::default();
            unwelded_config.inner.weld_vertices = false;
            let mut unwelded_world =
                PyVoxelWorld::from_storage(&storage, Some(&unwelded_config), None, None, None);
            let unwelded_mesh = unwelded_world.rebuild_all(py, None).unwrap();
            assert_eq!(unwelded_mesh.vertex_count(), 24); // 6 faces * 4 verts for isolated unwelded block

            let mut dirty_world =
                PyVoxelWorld::from_storage(&storage, Some(&config), None, None, None);
            let rebuilt = dirty_world.rebuild_dirty(py).unwrap();
            assert_eq!(rebuilt.len(), 1);
        });
    }

    #[test]
    fn test_python_ctm_bindings() {
        Python::initialize();
        Python::attach(|_py| {
            let stack = PyResourcePackStack::new();
            assert_eq!(stack.get_ctm_rule_count(), 0);
            let solver = stack.build_ctm_solver();
            assert_eq!(solver.rule_count(), 0);

            let mut config = PyMesherConfig::default();
            assert!(config.inner.ctm_solver.is_none());
            config.set_ctm_solver(Some(&solver));
            assert!(config.inner.ctm_solver.is_some());
        });
    }

    #[test]
    fn test_python_live_sync_session_and_protocol() {
        Python::initialize();
        Python::attach(|py| {
            // Test packet decode (Ping 0x0E)
            let pkt_bytes = [
                0x4D, 0x54, 0x03, 0x0E, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ];
            let dict = decode_packet(py, &pkt_bytes).unwrap();
            assert_eq!(
                dict.get_item("type")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "PING"
            );

            // Test session
            let session = PyLiveSyncSession::new(None, None, None);
            let events = session.poll_events(py, false).unwrap();
            assert_eq!(events.len(), 0);
        });
    }

    #[test]
    fn test_python_version_string() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_python_uv_functions() {
        let quad = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        assert!((uv::calculate_uv_area(quad.clone()) - 1.0).abs() < 1e-6);

        let bounds = uv::get_uv_bounds(quad.clone());
        assert_eq!(bounds, (0.0, 0.0, 1.0, 1.0, 1.0, 1.0));

        let center = uv::get_uv_center(quad.clone());
        assert_eq!(center, (0.5, 0.5));

        assert!(!uv::is_uv_collapsed(quad.clone(), None, None, None));
        assert!(uv::is_orthogonal_angle(0.0, 1e-3));
        assert!((uv::detect_uv_rotation(quad.clone(), 1e-3)).abs() < 1e-6);

        let (ang, straightened, new_uvs) = uv::straighten_uv(quad.clone(), None);
        assert!(!straightened);
        assert_eq!(ang, 0.0);
        assert_eq!(new_uvs.len(), 4);

        let scaled = uv::scale_uv(quad.clone(), 0.5);
        assert_eq!(scaled[0], (0.25, 0.25));

        let (norm, scale, loc) = uv::normalize_uv_for_atlas_tiling(quad.clone(), 1e-6);
        assert_eq!(norm.len(), 4);
        assert_eq!(scale, (1.0, 1.0, 1.0));
        assert_eq!(loc, (0.0, 0.0, 0.0));

        assert!(!uv::uv_requires_atlas_tiling(quad, 1e-4));
        assert_eq!(
            uv::restore_atlas_tiling_uv(0.5, 0.5, (1.0, 1.0, 1.0), (0.0, 0.0, 0.0), 0.0),
            (0.5, 0.5)
        );

        let verts = vec![
            (0.0, 0.0, 1.0),
            (0.0, 0.0, 0.0),
            (0.0, 0.2, 0.0),
            (0.0, 0.8, 1.0),
        ];
        let inv_uvs = vec![(1.0, 0.0), (0.0, 0.0), (0.0, 0.8), (1.0, 0.2)];
        let (repaired, out_uvs) =
            uv::repair_quad_fluid_uv(verts, inv_uvs, None, false, 0.005).unwrap();
        assert!(repaired);
        assert!((out_uvs[2].1 - 0.2).abs() < 1e-5);
        assert!((out_uvs[3].1 - 0.8).abs() < 1e-5);

        let top_uvs = uv::get_fluid_top_uvs(true, 0.0);
        assert_eq!(top_uvs.len(), 4);
        assert_eq!(top_uvs[0], (0.25, 0.25));

        let side_uvs = uv::get_fluid_side_uvs(0.8, 0.2);
        assert_eq!(side_uvs.len(), 4);
        assert!((side_uvs[0].1 - 0.1).abs() < 1e-6);

        // Test batch_repair_fluid_uv with Python list & zero-copy buffer
        Python::initialize();
        Python::attach(|py| {
            let verts_flat = pyo3::types::PyList::new(
                py,
                [
                    0.0f32, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.8, 1.0,
                ],
            )
            .unwrap();
            let uvs_flat =
                pyo3::types::PyList::new(py, [1.0f32, 0.0, 0.0, 0.0, 0.0, 0.8, 1.0, 0.2]).unwrap();
            let (count, out_uvs) = uv::batch_repair_fluid_uv(
                py,
                verts_flat.as_any(),
                uvs_flat.as_any(),
                None,
                false,
                0.005,
            )
            .unwrap();
            assert_eq!(count, 1);
            let out_vec: Vec<f32> = out_uvs.extract().unwrap();
            assert!((out_vec[5] - 0.2).abs() < 1e-5);
            assert!((out_vec[7] - 0.8).abs() < 1e-5);

            // Test with zero-copy array.array('f') buffer
            let py_code = "import array\nverts = array.array('f', [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.8, 1.0])\nuvs = array.array('f', [1.0, 0.0, 0.0, 0.0, 0.0, 0.8, 1.0, 0.2])\n";
            let locals = pyo3::types::PyDict::new(py);
            py.run(
                &std::ffi::CString::new(py_code).unwrap(),
                None,
                Some(&locals),
            )
            .unwrap();
            let py_verts = locals.get_item("verts").unwrap().unwrap();
            let py_uvs = locals.get_item("uvs").unwrap().unwrap();
            let (b_count, _) =
                uv::batch_repair_fluid_uv(py, &py_verts, &py_uvs, None, false, 0.005).unwrap();
            let uvs_after: Vec<f32> = py_uvs.extract().unwrap();
            assert_eq!(b_count, 1);
            // Verify in-place mutation
            assert!((uvs_after[5] - 0.2).abs() < 1e-5);
            assert!((uvs_after[7] - 0.8).abs() < 1e-5);
        });
    }

    #[test]
    fn test_python_pixel_grid_cut_factors() {
        let u0 = 1.3 / 16.0;
        let u1 = 4.7 / 16.0;
        let v0 = 2.4 / 16.0;
        let v1 = 5.8 / 16.0;
        let uvs = vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]];

        let (u_cuts, v_cuts) = subdivide::calculate_pixel_grid_cut_factors(uvs, 16, 16, 1.0, 64);
        assert_eq!(u_cuts.len(), 5);
        assert_eq!(v_cuts.len(), 5);
    }

    #[test]
    fn test_python_slice_face_by_pixel_grid() {
        Python::initialize();
        Python::attach(|py| {
            let positions = vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ];
            let uvs = vec![[0.5, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 0.5]];
            let (pos, out_uvs, faces, params) =
                subdivide::slice_face_by_pixel_grid(py, positions, uvs, 16, 16, 1.0, 64);
            assert!(!faces.is_empty());
            assert_eq!(pos.len(), out_uvs.len());
            assert_eq!(pos.len(), params.len());
        });
    }

    #[test]
    fn test_python_save_bindings() {
        Python::initialize();
        Python::attach(|_py| {
            let Some(test_save_path) = mtk_testkit::save_world() else {
                eprintln!("Skipping: no Minecraft save available (set MTK_TEST_SAVE)");
                return;
            };
            let test_path = test_save_path.to_string_lossy().to_string();
            let info = save::inspect_minecraft_save(&test_path).expect("Failed to inspect save");
            assert!(!info.level_name().is_empty());
            assert!(info.data_version() >= 2844);
            assert!(!info.dimensions().is_empty());
            println!("Python save inspect: {:?}", info);
        });
    }

    #[test]
    fn test_python_analyze_mesh_hard_edges() {
        Python::initialize();
        Python::attach(|py| {
            let positions = vec![
                0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
                0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
            ];
            let edge_verts = vec![
                0, 1, 1, 2, 2, 3, 3, 0, 4, 5, 5, 6, 6, 7, 7, 4, 0, 4, 1, 5, 2, 6, 3, 7,
            ];
            let face_loop_starts = vec![0, 4, 8, 12, 16, 20];
            let face_loop_totals = vec![4, 4, 4, 4, 4, 4];
            let loop_verts = vec![
                0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4, 2, 3, 7, 6, 0, 4, 7, 3, 1, 2, 6, 5,
            ];

            let py_pos = pyo3::types::PyList::new(py, &positions).unwrap();
            let py_edges = pyo3::types::PyList::new(py, &edge_verts).unwrap();
            let py_starts = pyo3::types::PyList::new(py, &face_loop_starts).unwrap();
            let py_totals = pyo3::types::PyList::new(py, &face_loop_totals).unwrap();
            let py_loops = pyo3::types::PyList::new(py, &loop_verts).unwrap();

            let bytes = mesh::analyze_mesh_hard_edges(
                py,
                &py_pos.into_any(),
                &py_edges.into_any(),
                &py_starts.into_any(),
                &py_totals.into_any(),
                &py_loops.into_any(),
                0.5,
                None,
            )
            .unwrap();

            assert_eq!(bytes.as_bytes().len(), 12);
            assert!(bytes.as_bytes().iter().all(|&b| b == 1));
        });
    }

    #[test]
    fn test_python_repair_mesh_fluid_uv_inplace() {
        Python::initialize();
        Python::attach(|py| {
            let positions = vec![
                0.0, 0.0, 1.0, // 0: Bottom right
                0.0, 0.0, 0.0, // 1: Bottom left
                0.0, 0.2, 0.0, // 2: Top left (low: 0.2)
                0.0, 0.8, 1.0, // 3: Top right (high: 0.8)
            ];
            let loop_verts = vec![0, 1, 2, 3];
            let face_loop_starts = vec![0];
            let face_loop_totals = vec![4];
            let uvs = vec![
                1.0, 0.0, // loop 0
                0.0, 0.0, // loop 1
                0.0, 0.8, // loop 2: Inverted
                1.0, 0.2, // loop 3: Inverted
            ];

            let py_pos = pyo3::types::PyList::new(py, &positions).unwrap();
            let py_loops = pyo3::types::PyList::new(py, &loop_verts).unwrap();
            let py_starts = pyo3::types::PyList::new(py, &face_loop_starts).unwrap();
            let py_totals = pyo3::types::PyList::new(py, &face_loop_totals).unwrap();
            let mut uvs_bytes = Vec::with_capacity(uvs.len() * 4);
            for &val in &uvs {
                uvs_bytes.extend_from_slice(&f32::to_le_bytes(val));
            }
            let py_uvs = pyo3::types::PyByteArray::new(py, &uvs_bytes);

            let count = uv::repair_mesh_fluid_uv_inplace(
                py,
                &py_pos.into_any(),
                &py_loops.into_any(),
                &py_starts.into_any(),
                &py_totals.into_any(),
                &py_uvs.into_any(),
                None,
                false,
                0.005,
            )
            .unwrap();

            assert_eq!(count, 1);
        });
    }
}
