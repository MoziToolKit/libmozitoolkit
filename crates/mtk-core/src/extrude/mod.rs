//! # Auto Extrude, UV Repair & Random Noise Extrude Engine
//!
//! Provides geometric UV reconstruction for newly extruded side faces, collapsed UV detection,
//! Atlas Safe Padding Clamping, 3D Perlin / Cellular noise generators for terrain extrusion,
//! and batch whole-mesh topological analysis and feature edge crease tagging.

pub mod noise;
pub mod repair;
pub mod types;
pub mod uv;

pub use noise::{cellular_noise_3d, generate_extrude_heights, hash_3d, perlin_noise_3d};
pub use repair::process_flat_mesh_extrude_repair;
pub use types::{
    ExtrudeMeshOutput, ExtrudeNoiseType, ExtrudeUvMode, FlatPolygonMesh, MeshExtrudeRepairConfig,
};
pub use uv::{repair_extruded_side_uv, repair_extruded_side_uv_advanced};

pub use crate::uv::{
    calculate_uv_area_2d as calculate_uv_area, is_uv_collapsed_2d as is_uv_collapsed,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perlin_and_cellular_noise_bounds() {
        for i in 0..10 {
            let val = perlin_noise_3d(i as f32 * 0.3, i as f32 * 0.5, 0.0, 42);
            assert!((-1.0..=1.0).contains(&val));

            let cell = cellular_noise_3d(i as f32 * 0.3, i as f32 * 0.5, 0.0, 42);
            assert!((0.0..=1.0).contains(&cell));
        }
    }

    #[test]
    fn test_generate_extrude_heights_discrete() {
        let centers = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]];
        let heights = generate_extrude_heights(
            &centers,
            ExtrudeNoiseType::UniformRandom,
            0.0,
            1.0,
            1.0,
            123,
            Some(3), // 3 steps: 0.0, 0.5, 1.0
        );
        for h in heights {
            assert!(h == 0.0 || (h - 0.5).abs() < 1e-4 || (h - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn test_flat_polygon_mesh_and_flat_repair() {
        let positions_flat: Vec<f32> = vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0,
            1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ];

        let loop_vertices = vec![
            4, 5, 6, 7, // Top face 0
            0, 1, 5, 4, // Side face 1
            1, 2, 6, 5, // Side face 2
            2, 3, 7, 6, // Side face 3
            3, 0, 4, 7, // Side face 4
        ];

        let top_uvs = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let col_uvs = vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let mut loop_uvs = Vec::new();
        loop_uvs.extend_from_slice(&top_uvs);
        for _ in 0..4 {
            loop_uvs.extend_from_slice(&col_uvs);
        }

        let face_loop_starts = vec![0, 4, 8, 12, 16];
        let face_loop_totals = vec![4, 4, 4, 4, 4];
        let face_materials = vec![0, 0, 0, 0, 0];

        let flat_mesh = FlatPolygonMesh::from_flat_buffers(
            &positions_flat,
            loop_vertices,
            &loop_uvs,
            face_loop_starts,
            face_loop_totals,
            face_materials,
        );

        assert_eq!(flat_mesh.face_count(), 5);

        let pixel_steps = vec![[1.0 / 16.0, 1.0 / 16.0]; 5];
        let cfg = MeshExtrudeRepairConfig {
            uv_mode: ExtrudeUvMode::Smart,
            repair_uv: true,
            add_crease: false,
            crease_val: 0.0,
            only_collapsed: true,
        };

        let output = process_flat_mesh_extrude_repair(&flat_mesh, &[0], &pixel_steps, &cfg, None);
        assert_eq!(output.repaired_count, 4);
    }
}
