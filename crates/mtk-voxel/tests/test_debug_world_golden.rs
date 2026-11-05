//! Golden output test for the embedded debug world.
//!
//! Guards the mesher refactors (palette preparation, tint classification, merge
//! pre-allocation) against silent geometry, UV, material or custom-attribute drift.
//!
//! Section enumeration order is normalized (`get_all_non_empty_sections` comes from a
//! `HashMap`) so the vertex order is reproducible across processes.

use mtk_core::MeshData;
use mtk_cull::FaceCuller;
use mtk_voxel::{MesherConfig, ModelSource, SectionMesher, VoxelStorage};

/// Expected checksum of the pure-code fallback debug world (385 states / 2,310 quads).
const EXPECTED_QUADS: usize = 2310;
const EXPECTED_VERTS: usize = 3080;
const EXPECTED_CHECKSUM: u64 = 0xa380_6cfe_974f_1a8b;

fn mesh_checksum(m: &MeshData) -> u64 {
    fn feed(h: &mut u64, bytes: &[u8]) {
        for &b in bytes {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    fn feed_f32(h: &mut u64, values: &[f32]) {
        for v in values {
            feed(h, &v.to_bits().to_le_bytes());
        }
    }

    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    feed_f32(
        &mut h,
        &m.positions.iter().flatten().copied().collect::<Vec<_>>(),
    );
    feed_f32(
        &mut h,
        &m.normals.iter().flatten().copied().collect::<Vec<_>>(),
    );
    feed_f32(&mut h, &m.uvs.iter().flatten().copied().collect::<Vec<_>>());
    for i in &m.indices {
        feed(&mut h, &i.to_le_bytes());
    }
    for slot in &m.face_materials {
        feed(&mut h, &slot.to_le_bytes());
    }
    for idx in &m.face_tint_indices {
        feed(&mut h, &idx.to_le_bytes());
    }
    let mut names: Vec<_> = m.custom_attributes.keys().cloned().collect();
    names.sort();
    for name in names {
        feed(&mut h, name.as_bytes());
        feed(
            &mut h,
            format!("{:?}", m.custom_attributes[&name]).as_bytes(),
        );
    }
    h
}

#[test]
fn debug_world_output_is_stable() {
    let storage = match VoxelStorage::create_debug_world() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skipping: debug world unavailable ({e})");
            return;
        }
    };

    // Only the in-memory pure-code fallback world is covered; environments that point
    // `MC_ASSETS_DIR`/`MC_DIR` at real assets produce a much larger world.
    let mut coords = storage.get_all_non_empty_sections();
    if coords.len() != 9 || storage.size_x != 42 || storage.size_y != 3 || storage.size_z != 42 {
        eprintln!(
            "skipping: expected the 42x3x42 pure-code debug world, got {}x{}x{} / {} sections",
            storage.size_x,
            storage.size_y,
            storage.size_z,
            coords.len()
        );
        return;
    }
    coords.sort_by_key(|c| (c.x, c.y, c.z));

    let padded: Vec<_> = coords
        .iter()
        .map(|&coord| storage.get_section_padded_array(coord))
        .collect();
    let culler = FaceCuller::default();
    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        enable_ao: true,
        ..Default::default()
    };
    let results = SectionMesher::mesh_sections_parallel_with_source_and_progress(
        &padded,
        &culler,
        |_st| ModelSource::None,
        &config,
        None,
        None,
    )
    .expect("section meshing failed");
    let refs: Vec<&MeshData> = results.iter().map(|(_, m)| m).collect();
    let merged = MeshData::merge_welded_sections(&refs, 1e-4);

    assert_eq!(merged.quad_count(), EXPECTED_QUADS, "quad count changed");
    assert_eq!(
        merged.vertex_count(),
        EXPECTED_VERTS,
        "vertex count changed"
    );
    assert_eq!(
        mesh_checksum(&merged),
        EXPECTED_CHECKSUM,
        "debug-world mesh output drifted (positions/uvs/indices/attributes)"
    );
}
