use super::*;
use crate::protocol::decode_packet;

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
        let stages = profile.stages();
        assert!(!stages.is_empty());
        assert!(stages.iter().all(|s| s.category() == "mesh"));
        assert!(stages
            .iter()
            .any(|s| s.name() == "extract_padded" && s.depth() == 0));
        assert!(stages
            .iter()
            .any(|s| s.name() == "parallel_meshing" && s.depth() == 0));
        assert!(stages
            .iter()
            .any(|s| s.name() == "assemble" && s.depth() == 0));
        assert!(stages
            .iter()
            .any(|s| s.name() == "merge_welded_seams" && s.depth() == 1));

        let mut unwelded_config = PyMesherConfig::default();
        unwelded_config.inner.weld_vertices = false;
        let mut unwelded_world =
            PyVoxelWorld::from_storage(&storage, Some(&unwelded_config), None, None, None);
        let unwelded_mesh = unwelded_world.rebuild_all(py, None).unwrap();
        assert_eq!(unwelded_mesh.vertex_count(), 24); // 6 faces * 4 verts for isolated unwelded block

        let mut dirty_world = PyVoxelWorld::from_storage(&storage, Some(&config), None, None, None);
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
        assert_eq!(solver.inner.rules.len(), 0);

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
fn test_python_api_version() {
    assert_eq!(api_version(), 2);
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
    let (repaired, out_uvs) = uv::repair_quad_fluid_uv(verts, inv_uvs, None, false, 0.005).unwrap();
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
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0,
            1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
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
            py_pos.as_any(),
            py_loops.as_any(),
            py_starts.as_any(),
            py_totals.as_any(),
            py_uvs.as_any(),
            None,
            false,
            0.005,
        )
        .unwrap();

        assert_eq!(count, 1);

        // Verify aliasing rejection when uvs is passed as positions or loop_verts
        let err_alias_pos = uv::repair_mesh_fluid_uv_inplace(
            py,
            py_uvs.as_any(),
            py_loops.as_any(),
            py_starts.as_any(),
            py_totals.as_any(),
            py_uvs.as_any(),
            None,
            false,
            0.005,
        );
        assert!(err_alias_pos.is_err());

        let err_alias_loop = uv::repair_mesh_fluid_uv_inplace(
            py,
            py_pos.as_any(),
            py_uvs.as_any(),
            py_starts.as_any(),
            py_totals.as_any(),
            py_uvs.as_any(),
            None,
            false,
            0.005,
        );
        assert!(err_alias_loop.is_err());

        // Verify aliasing rejection when uvs and positions are distinct objects but share overlapping memory spans
        let shared_bytes = pyo3::types::PyByteArray::new(py, &[0u8; 128]);
        let shared_mv = pyo3::types::PyMemoryView::from(&shared_bytes).unwrap();
        let shared_f32 = shared_mv.call_method1("cast", ("f",)).unwrap();
        let sub_pos = shared_f32
            .call_method1("__getitem__", (pyo3::types::PySlice::new(py, 0, 12, 1),))
            .unwrap();
        let sub_uvs = shared_f32
            .call_method1("__getitem__", (pyo3::types::PySlice::new(py, 8, 16, 1),))
            .unwrap();
        assert!(!sub_pos.is(&sub_uvs), "Slices must be distinct objects");

        let err_overlap = uv::repair_mesh_fluid_uv_inplace(
            py,
            sub_pos.as_any(),
            py_loops.as_any(),
            py_starts.as_any(),
            py_totals.as_any(),
            sub_uvs.as_any(),
            None,
            false,
            0.005,
        );
        assert!(err_overlap.is_err(), "Overlapping memory must be rejected");
        let err_msg = err_overlap.unwrap_err().to_string();
        assert!(err_msg.contains("must not overlap in memory"));
    });
}
