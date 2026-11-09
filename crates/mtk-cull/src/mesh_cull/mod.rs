//! # Mesh Face Culling for Imported / Arbitrary 3D Geometries
//!
//! High-performance spatial-hashing based face culling engine for external mesh imports
//! (e.g. Mineways, jmc2obj, Blockbench OBJ). Detects coplanar opposite-facing polygons
//! and full 2D projected occlusion to eliminate interior unseen faces.

pub mod rebuild;
pub mod search;

use mtk_core::MeshData;
use rebuild::rebuild_culled_mesh;
use search::find_faces_to_cull;

/// Configuration options for mesh face culling.
#[derive(Debug, Clone)]
pub struct MeshCullConfig {
    /// Tolerance distance for vertex position equality (default 1e-3).
    pub tolerance: f32,
    /// Whether to cull double-sided coplanar overlapping faces.
    pub cull_coplanar_opposite: bool,
    /// Whether to cull identical overlapping faces with the same normal (duplicate faces).
    pub cull_duplicates: bool,
}

impl Default for MeshCullConfig {
    fn default() -> Self {
        Self {
            tolerance: 1e-3,
            cull_coplanar_opposite: true,
            cull_duplicates: true,
        }
    }
}

/// Result statistics from a mesh face culling operation.
#[derive(Debug, Clone, Default)]
pub struct MeshCullResult {
    pub initial_faces: usize,
    pub culled_faces: usize,
    pub remaining_faces: usize,
    pub mesh: MeshData,
    pub profile: Option<mtk_core::ExecutionProfile>,
}

/// Performs face culling on a `MeshData` buffer.
///
/// Operates on quads (6 indices per quad) or triangles (3 indices per tri).
pub fn cull_mesh_faces(mesh: &MeshData, config: &MeshCullConfig) -> MeshCullResult {
    let mut profiler = mtk_core::ExecutionProfiler::start();
    let tri_count = mesh.indices.len() / 3;
    if tri_count == 0 {
        return MeshCullResult {
            mesh: mesh.clone(),
            profile: Some(profiler.finish()),
            ..Default::default()
        };
    }

    let is_quad_mesh = mesh.indices.len().is_multiple_of(6)
        && (mesh.face_materials.len() == mesh.indices.len() / 6);
    let poly_step = if is_quad_mesh { 6 } else { 3 };
    let face_count = mesh.indices.len() / poly_step;

    let faces_to_cull =
        profiler.measure_with_count("spatial_plane_bucketing_and_search", face_count, || {
            find_faces_to_cull(mesh, face_count, poly_step, is_quad_mesh, config)
        });

    if faces_to_cull.is_empty() {
        return MeshCullResult {
            initial_faces: face_count,
            culled_faces: 0,
            remaining_faces: face_count,
            mesh: mesh.clone(),
            profile: Some(profiler.finish()),
        };
    }

    let (out, remaining_faces) = profiler.measure("rebuild_culled_mesh", || {
        rebuild_culled_mesh(mesh, face_count, poly_step, is_quad_mesh, &faces_to_cull)
    });

    MeshCullResult {
        initial_faces: face_count,
        culled_faces: faces_to_cull.len(),
        remaining_faces,
        mesh: out,
        profile: Some(profiler.finish()),
    }
}
