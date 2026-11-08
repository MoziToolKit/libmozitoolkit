use mtk_core::{AttributeData, MeshAttribute, MeshData};
use mtk_cull::{cull_mesh_faces, MeshCullConfig};


    #[test]
    fn test_cull_coplanar_opposite_faces() {
        let mut mesh = MeshData::new();
        // Quad 1: Facing +Z
        mesh.positions.extend_from_slice(&[
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ]);
        mesh.normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 4]);
        mesh.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        mesh.face_materials.push(0);
        mesh.face_tint_indices.push(-1);

        // Quad 2: Touching same plane, facing -Z
        mesh.positions.extend_from_slice(&[
            [-0.5, -0.5, 0.0],
            [-0.5, 0.5, 0.0],
            [0.5, 0.5, 0.0],
            [0.5, -0.5, 0.0],
        ]);
        mesh.normals.extend_from_slice(&[[0.0, 0.0, -1.0]; 4]);
        mesh.uvs
            .extend_from_slice(&[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        mesh.indices.extend_from_slice(&[4, 5, 6, 4, 6, 7]);
        mesh.face_materials.push(0);
        mesh.face_tint_indices.push(-1);

        let res = cull_mesh_faces(&mesh, &MeshCullConfig::default());
        assert_eq!(res.initial_faces, 2);
        assert_eq!(
            res.culled_faces, 2,
            "Both contacting interior faces must be culled!"
        );
        assert_eq!(res.remaining_faces, 0);
    }

    #[test]
    fn test_cull_across_quantization_boundary() {
        let mut mesh = MeshData::new();
        // Quad 1: Center at x = 0.00049 (rounds to cell 0 under inv_tol=1000)
        let offset1 = 0.00049f32;
        mesh.positions.extend_from_slice(&[
            [offset1, -0.5, -0.5],
            [offset1, 0.5, -0.5],
            [offset1, 0.5, 0.5],
            [offset1, -0.5, 0.5],
        ]);
        mesh.normals.extend_from_slice(&[[1.0, 0.0, 0.0]; 4]);
        mesh.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        mesh.face_materials.push(0);
        mesh.face_tint_indices.push(-1);

        // Quad 2: Center at x = 0.00051 (rounds to cell 1 under inv_tol=1000)
        // Actual distance between faces is only 0.00002 << 1e-3 tolerance
        let offset2 = 0.00051f32;
        mesh.positions.extend_from_slice(&[
            [offset2, -0.5, -0.5],
            [offset2, -0.5, 0.5],
            [offset2, 0.5, 0.5],
            [offset2, 0.5, -0.5],
        ]);
        mesh.normals.extend_from_slice(&[[-1.0, 0.0, 0.0]; 4]);
        mesh.uvs
            .extend_from_slice(&[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        mesh.indices.extend_from_slice(&[4, 5, 6, 4, 6, 7]);
        mesh.face_materials.push(0);
        mesh.face_tint_indices.push(-1);

        let res = cull_mesh_faces(&mesh, &MeshCullConfig::default());
        assert_eq!(res.initial_faces, 2);
        assert_eq!(
            res.culled_faces, 2,
            "Faces across quantization boundary must be detected and culled!"
        );
        assert_eq!(res.remaining_faces, 0);
    }

    #[test]
    fn test_cull_duplicate_faces_preserving_attributes() {
        let mut mesh = MeshData::new();
        // Quad 1: Facing +Z
        mesh.positions.extend_from_slice(&[
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ]);
        mesh.normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 4]);
        mesh.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        mesh.face_materials.push(10);
        mesh.face_tint_indices.push(-1);

        // Quad 2: Identical duplicate face at same position facing +Z
        mesh.positions.extend_from_slice(&[
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ]);
        mesh.normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 4]);
        mesh.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        mesh.indices.extend_from_slice(&[4, 5, 6, 4, 6, 7]);
        mesh.face_materials.push(20);
        mesh.face_tint_indices.push(-1);

        mesh.add_custom_attribute(MeshAttribute::new(
            "test_attr",
            mtk_core::AttributeDomain::Face,
            AttributeData::String(vec!["face_0".to_string(), "face_1".to_string()]),
        ));

        let res = cull_mesh_faces(&mesh, &MeshCullConfig::default());
        assert_eq!(res.initial_faces, 2);
        assert_eq!(
            res.culled_faces, 1,
            "Duplicate face must be culled to eliminate Z-fighting"
        );
        assert_eq!(res.remaining_faces, 1);
        assert_eq!(res.mesh.face_materials, vec![10]);

        // Verify custom attributes preserved
        let attr = res
            .mesh
            .custom_attributes
            .get("test_attr")
            .expect("test_attr must exist");
        if let AttributeData::String(ref vals) = attr.data {
            assert_eq!(vals.len(), 1);
            assert_eq!(vals[0], "face_0");
        } else {
            panic!("Expected String attribute data");
        }
    }

    #[test]
    fn test_cull_with_shared_vertices_and_per_loop_uvs() {
        let mut mesh = MeshData::new();
        // 6 shared vertices
        // v0, v1, v2, v3 form a quad at z = 0.
        // v4, v5 form a connected quad at z = 1.
        mesh.positions = vec![
            [-0.5, -0.5, 0.0], // 0
            [0.5, -0.5, 0.0],  // 1
            [0.5, 0.5, 0.0],   // 2
            [-0.5, 0.5, 0.0],  // 3
            [0.5, 1.5, 0.0],   // 4
            [-0.5, 1.5, 0.0],  // 5
        ];
        mesh.normals = vec![[0.0, 0.0, 1.0]; 6];
        mesh.uv_layout = mtk_core::UvLayout::PerLoop;

        // Quad 1 (z=0, facing +Z): vertices [0, 1, 2, 3]
        // Quad 2 (z=0, touching back-to-back facing -Z): vertices [0, 3, 2, 1]
        // Quad 3 (z=0..1, facing +Z): vertices [3, 2, 4, 5]
        mesh.indices = vec![
            0, 1, 2, 0, 2, 3, // Quad 1
            0, 3, 2, 0, 2, 1, // Quad 2
            3, 2, 4, 3, 4, 5, // Quad 3
        ];
        mesh.face_materials = vec![1, 2, 3];
        mesh.face_tint_indices = vec![-1, -1, -1];

        // Per-loop UVs: 4 UVs per quad (12 UVs total)
        let q1_uvs = [[0.1, 0.1], [0.2, 0.1], [0.2, 0.2], [0.1, 0.2]];
        let q2_uvs = [[0.3, 0.3], [0.4, 0.3], [0.4, 0.4], [0.3, 0.4]];
        let q3_uvs = [[0.8, 0.8], [0.9, 0.8], [0.9, 0.9], [0.8, 0.9]];
        mesh.uvs = [q1_uvs, q2_uvs, q3_uvs].concat();
        mesh.quad_indices = Some(vec![0, 1, 2, 3, 0, 3, 2, 1, 3, 2, 4, 5]);

        let res = cull_mesh_faces(&mesh, &MeshCullConfig::default());
        assert_eq!(res.initial_faces, 3);
        assert_eq!(res.culled_faces, 2, "Quad 1 and 2 touch and must be culled");
        assert_eq!(res.remaining_faces, 1, "Quad 3 must survive");
        assert_eq!(res.mesh.face_materials, vec![3]);

        // Quad 3 must have exactly 4 vertices, 4 UVs, 6 triangle indices, and 4 quad indices!
        assert_eq!(res.mesh.positions.len(), 4);
        assert_eq!(res.mesh.uvs.len(), 4);
        assert_eq!(res.mesh.uv_layout, mtk_core::UvLayout::PerLoop);
        assert_eq!(
            res.mesh.uvs,
            q3_uvs.to_vec(),
            "UVs of Quad 3 must be preserved exactly!"
        );
        assert_eq!(res.mesh.indices, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(res.mesh.quad_indices, Some(vec![0, 1, 2, 3]));
    }
