//! Welded meshes must still carry per-face normals.
//!
//! Unit-cube geometry is flat shaded: every vertex of a quad has to carry that quad's face
//! normal. The welder only deduplicates by position, so a corner shared by three faces keeps
//! whichever normal arrived first — invisible under Blender's flat shading, wrong for any
//! normals-based consumer (glTF export, smooth shading, normal visualisation).

use glam::IVec3;
use mtk_cull::FaceCuller;
use mtk_voxel::{MesherConfig, ModelSource, SectionMesher, VoxelStorage, VoxelWorld};

fn geometric_normal(mesh: &mtk_core::MeshData, quad: &[u32; 4]) -> [f32; 3] {
    let p0 = mesh.positions[quad[0] as usize];
    let p1 = mesh.positions[quad[1] as usize];
    let p2 = mesh.positions[quad[2] as usize];
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
    [n[0] / len, n[1] / len, n[2] / len]
}

/// Every vertex of every quad must carry that quad's own normal.
fn assert_normals_match_their_quads(mesh: &mtk_core::MeshData, context: &str) {
    let Some(quads) = mesh.quad_indices.as_ref() else {
        panic!("{context}: expected quad topology");
    };
    assert!(!quads.is_empty(), "{context}: no quads");
    for quad in quads.as_chunks::<4>().0 {
        let quad: [u32; 4] = *quad;
        let expected = geometric_normal(mesh, &quad);
        for idx in quad {
            let normal = mesh.normals[idx as usize];
            let dot = normal[0] * expected[0] + normal[1] * expected[1] + normal[2] * expected[2];
            assert!(
                dot > 0.999,
                "{context}: vertex {idx} at {:?} carries normal {normal:?} but its quad faces \
                 {expected:?}",
                mesh.positions[idx as usize]
            );
        }
    }
}

fn single_section_cube() -> mtk_core::MeshData {
    let mut storage = VoxelStorage::new();
    storage.set_bounds(0, 0, 0, 16, 16, 16);
    storage.set_block(1, 1, 1, "minecraft:stone", None);
    let padded = storage.get_section_padded_array(IVec3::new(0, 0, 0));
    let culler = FaceCuller::default();
    let config = MesherConfig::default();
    assert!(config.weld_vertices, "the default must weld");
    SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config)
}

#[test]
fn a_welded_cube_keeps_per_face_normals() {
    let mesh = single_section_cube();
    assert_normals_match_their_quads(&mesh, "single welded cube");
}

/// Two blocks in different sections share corners across the seam; the seam weld must not merge
/// vertices that belong to differently oriented faces either.
#[test]
fn seam_welding_keeps_per_face_normals() {
    let mut storage = VoxelStorage::new();
    storage.set_bounds(0, 0, 0, 32, 16, 16);
    storage.set_block(15, 0, 0, "minecraft:stone", None);
    storage.set_block(16, 0, 0, "minecraft:stone", None);

    let mut world = VoxelWorld::from_storage(storage, None, None, None);
    world.rebuild_all().expect("rebuild");
    let mesh = world.get_world_mesh().expect("mesh");
    assert!(
        !mesh.positions.is_empty(),
        "the two blocks must produce geometry"
    );
    assert_normals_match_their_quads(mesh, "seam welded blocks");
}
