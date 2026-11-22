use super::*;
use crate::attributes::{AttributeData, AttributeDomain};
use crate::direction::Direction;
use crate::FlatPolygonMesh;

#[test]
fn test_mesh_append_quad() {
    let mut mesh = MeshData::new();
    let quad = Quad::unit_cube_face(Direction::Up);
    let attr = FaceAttributes {
        material_slot: 1,
        tint_index: 0,
        ..Default::default()
    };

    mesh.append_quad(&quad, &attr);
    assert_eq!(mesh.vertex_count(), 4);
    assert_eq!(mesh.triangle_count(), 2);
    assert_eq!(mesh.face_count(), 1);
    assert_eq!(mesh.face_materials[0], 1);
    assert_eq!(mesh.face_tint_indices[0], 0);

    let pos_flat = mesh.positions_flat();
    assert_eq!(pos_flat.len(), 12);
    assert_eq!(pos_flat[0], mesh.positions[0][0]);
    assert_eq!(pos_flat[1], mesh.positions[0][1]);
    assert_eq!(pos_flat[2], mesh.positions[0][2]);

    let norm_flat = mesh.normals_flat();
    assert_eq!(norm_flat.len(), 12);
    assert_eq!(norm_flat[0], 0.0);
    assert_eq!(norm_flat[1], 1.0);
    assert_eq!(norm_flat[2], 0.0);

    let uvs_flat = mesh.uvs_flat();
    assert_eq!(uvs_flat.len(), 8);
}

#[test]
fn test_from_flat_polygons_preserves_shared_vertices_and_loop_uvs() {
    let flat = FlatPolygonMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
        ],
        loop_vertices: vec![0, 1, 2, 3, 1, 4, 5, 2],
        loop_uvs: vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ],
        face_loop_starts: vec![0, 4],
        face_loop_totals: vec![4, 4],
        face_materials: vec![0, 1],
    };

    let mesh = MeshData::from_flat_polygons(&flat, None).unwrap();
    assert_eq!(mesh.vertex_count(), 6);
    assert_eq!(mesh.loop_count(), 8);
    assert_eq!(mesh.uv_layout, UvLayout::PerLoop);
    assert_eq!(mesh.uvs, flat.loop_uvs);
    assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3, 1, 4, 5, 1, 5, 2]);
    assert_eq!(mesh.face_materials, vec![0, 1]);
    assert_eq!(mesh.loop_uvs_expanded(), flat.loop_uvs);
}

#[test]
fn test_weld_vertices_policy_follows_uv_layout() {
    let mut per_loop = MeshData {
        positions: vec![
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [0.0, 0.0]],
        uv_layout: UvLayout::PerLoop,
        indices: vec![0, 1, 2, 0, 2, 3],
        quad_indices: Some(vec![0, 1, 2, 3]),
        face_materials: vec![0],
        ..Default::default()
    };
    per_loop.weld_vertices(1e-4);
    assert_eq!(per_loop.vertex_count(), 2);
    assert_eq!(per_loop.uvs.len(), 4, "per-loop UVs must survive welding");

    let mut per_vertex = MeshData {
        positions: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        normals: vec![[0.0, 1.0, 0.0]; 2],
        uvs: vec![[0.0, 0.0], [0.0, 0.0]],
        uv_layout: UvLayout::PerVertex,
        indices: vec![0, 1, 1],
        face_materials: vec![0],
        ..Default::default()
    };
    per_vertex.weld_vertices(1e-4);
    assert_eq!(per_vertex.vertex_count(), 1);
    assert_eq!(per_vertex.uvs.len(), 1);
    assert_eq!(per_vertex.indices, vec![0, 0, 0]);
}

#[test]
fn test_mesh_custom_attributes() {
    let mut mesh = MeshData::new();
    mesh.add_custom_attribute(MeshAttribute::new(
        "mtk_atlas_chunk_id",
        AttributeDomain::Face,
        AttributeData::Int32(vec![0, 1, 2]),
    ));
    mesh.add_custom_attribute(MeshAttribute::new(
        "mtk_custom_note",
        AttributeDomain::Face,
        AttributeData::String(vec!["minecraft:block/stone".to_string()]),
    ));

    assert!(mesh.has_custom_attribute("mtk_atlas_chunk_id"));
    assert_eq!(
        mesh.get_custom_attribute("mtk_atlas_chunk_id")
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        mesh.get_custom_attribute("mtk_atlas_chunk_id")
            .unwrap()
            .as_bytes()
            .unwrap()
            .len(),
        12
    );
    assert_eq!(
        mesh.get_custom_attribute("mtk_custom_note")
            .unwrap()
            .as_bytes(),
        None
    );

    let mut other = MeshData::new();
    other.add_custom_attribute(MeshAttribute::new(
        "mtk_atlas_chunk_id",
        AttributeDomain::Face,
        AttributeData::Int32(vec![3, 4]),
    ));

    mesh.append_mesh(&other);
    let merged_chunk_attr = mesh.get_custom_attribute("mtk_atlas_chunk_id").unwrap();
    assert_eq!(merged_chunk_attr.len(), 5);
    if let AttributeData::Int32(ref vals) = merged_chunk_attr.data {
        assert_eq!(vals, &vec![0, 1, 2, 3, 4]);
    } else {
        panic!("Expected Int32 attribute data");
    }
}

#[test]
fn test_mesh_used_and_compact_materials() {
    let mut mesh = MeshData::new();
    mesh.face_materials = vec![5, 0, 5, 12, 0];
    assert_eq!(mesh.used_materials(), vec![0, 5, 12]);

    let original_mapping = mesh.compact_materials();
    assert_eq!(original_mapping, vec![0, 5, 12]);
    assert_eq!(mesh.face_materials, vec![1, 0, 1, 2, 0]);
    assert_eq!(mesh.used_materials(), vec![0, 1, 2]);
}

#[test]
fn test_mesh_flat_accessors() {
    let mut mesh = MeshData::new();
    mesh.positions = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
    mesh.normals = vec![[0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    mesh.uvs = vec![[0.1, 0.2], [0.3, 0.4]];
    mesh.colors = Some(vec![[1.0, 0.0, 0.0, 1.0]]);

    assert_eq!(mesh.positions_flat(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(mesh.normals_flat(), &[0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    assert_eq!(mesh.uvs_flat(), &[0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn test_mesh_merge_welded_sections() {
    let mut m1 = MeshData::new();
    m1.positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
    m1.indices = vec![0, 1, 0];
    m1.uvs = vec![[0.0, 0.0], [1.0, 0.0]];
    m1.face_materials = vec![0];
    m1.face_tint_indices = vec![-1];

    let mut m2 = MeshData::new();
    // Co-located vertex at [1.0, 0.0, 0.0] along boundary!
    m2.positions = vec![[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    m2.indices = vec![0, 1, 0];
    m2.uvs = vec![[0.0, 0.0], [1.0, 0.0]];
    m2.face_materials = vec![1];
    m2.face_tint_indices = vec![-1];

    let merged = MeshData::merge_welded_sections(&[&m1, &m2], 1e-4);
    // [1.0, 0.0, 0.0] should be welded into shared topology!
    assert_eq!(merged.vertex_count(), 3);
    assert_eq!(merged.positions[0], [0.0, 0.0, 0.0]);
    assert_eq!(merged.positions[1], [1.0, 0.0, 0.0]);
    assert_eq!(merged.positions[2], [2.0, 0.0, 0.0]);
    // m2's index 0 should point to merged index 1
    assert_eq!(merged.indices, vec![0, 1, 0, 1, 2, 1]);
    assert_eq!(merged.face_materials, vec![0, 1]);
}

#[test]
fn test_mesh_merge_welded_sections_filtered() {
    let mut m1 = MeshData::new();
    m1.positions = vec![[0.5, 0.0, 0.0], [1.0, 0.0, 0.0]];
    m1.indices = vec![0, 1, 0];

    let mut m2 = MeshData::new();
    // [0.5, 0.0, 0.0] is an interior duplicate (not a Section seam candidate);
    // [1.0, 0.0, 0.0] lies on the Section seam and must still be welded.
    m2.positions = vec![[0.5, 0.0, 0.0], [1.0, 0.0, 0.0]];
    m2.indices = vec![0, 1, 0];

    let merged =
        MeshData::merge_welded_sections_filtered(&[&m1, &m2], 1e-4, |key| key == [10_000, 0, 0]);

    assert_eq!(merged.vertex_count(), 3);
    assert_eq!(merged.positions[0], [0.5, 0.0, 0.0]);
    assert_eq!(merged.positions[1], [1.0, 0.0, 0.0]);
    assert_eq!(merged.positions[2], [0.5, 0.0, 0.0]);
    // m2's seam vertex remaps to m1's index 1; its interior vertex stays unique at index 2.
    assert_eq!(merged.indices, vec![0, 1, 0, 2, 1, 2]);
}

#[test]
fn test_mesh_merge_palette_attribute_remap() {
    use crate::attributes::{constants::*, AttributeData, AttributeDomain, MeshAttribute};

    let mut m1 = MeshData::new();
    m1.positions = vec![[0.0, 0.0, 0.0]; 3];
    m1.indices = vec![0, 1, 2];
    m1.face_materials = vec![0];
    m1.face_tint_indices = vec![-1];
    m1.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_PALETTE,
        AttributeDomain::Mesh,
        AttributeData::String(vec!["bedrock".to_string(), "grass_block_top".to_string()]),
    ));
    m1.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_IDX,
        AttributeDomain::Face,
        AttributeData::Int32(vec![1]), // points to grass_block_top
    ));

    let mut m2 = MeshData::new();
    m2.positions = vec![[1.0, 0.0, 0.0]; 3];
    m2.indices = vec![0, 1, 2];
    m2.face_materials = vec![0];
    m2.face_tint_indices = vec![-1];
    m2.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_PALETTE,
        AttributeDomain::Mesh,
        AttributeData::String(vec![
            "bedrock".to_string(),
            "dirt".to_string(),
            "grass_block_top".to_string(),
        ]),
    ));
    m2.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_IDX,
        AttributeDomain::Face,
        AttributeData::Int32(vec![2, 1]), // face 0 -> grass_block_top (idx 2), face 1 -> dirt (idx 1)
    ));

    let merged = MeshData::merge_all_refs(&[&m1, &m2]);
    let pal_attr = merged
        .get_custom_attribute(ATTR_SOURCE_TEXTURE_PALETTE)
        .unwrap();
    let idx_attr = merged
        .get_custom_attribute(ATTR_SOURCE_TEXTURE_IDX)
        .unwrap();

    let AttributeData::String(ref pal) = pal_attr.data else {
        panic!()
    };
    let AttributeData::Int32(ref indices) = idx_attr.data else {
        panic!()
    };

    // Palette must be deduplicated
    assert_eq!(pal, &["bedrock", "grass_block_top", "dirt"]);
    // m1 face 0 was grass_block_top -> remapped to 1
    // m2 face 0 was grass_block_top (idx 2 in m2) -> remapped to 1 in merged
    // m2 face 1 was dirt (idx 1 in m2) -> remapped to 2 in merged
    assert_eq!(indices, &[1, 1, 2]);
}

#[test]
fn test_mesh_data_transform_coordinate_system() {
    let mut mesh = MeshData::new();
    mesh.positions.push([1.0, 2.0, 3.0]);
    mesh.normals.push([0.0, 1.0, 0.0]); // +Y normal

    // 1. Minecraft / Y-up leaves positions and normals intact
    mesh.transform_coordinate_system(CoordinateSystem::Minecraft);
    assert_eq!(mesh.positions[0], [1.0, 2.0, 3.0]);
    assert_eq!(mesh.normals[0], [0.0, 1.0, 0.0]);

    // 2. Z-up right-handed transforms (x, y, z) -> (x, -z, y)
    mesh.transform_coordinate_system(CoordinateSystem::ZUpRightHanded);
    assert_eq!(mesh.positions[0], [1.0, -3.0, 2.0]);
    assert_eq!(mesh.normals[0], [0.0, 0.0, 1.0]); // +Y becomes +Z
}

fn make_sample_quad_mesh(flipped_diag: bool) -> MeshData {
    use crate::attributes::constants::*;
    let indices = if flipped_diag {
        vec![1, 2, 3, 0, 1, 3]
    } else {
        vec![0, 1, 2, 0, 2, 3]
    };
    let mut mesh = MeshData {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        indices,
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        uv_layout: UvLayout::PerLoop,
        secondary_uvs: Some(vec![[0.1, 0.1], [0.2, 0.1], [0.2, 0.2], [0.1, 0.2]]),
        colors: Some(vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        ]),
        face_materials: vec![7],
        face_tint_indices: vec![2],
        quad_indices: Some(vec![0, 1, 2, 3]),
        custom_attributes: HashMap::new(),
    };
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_PALETTE,
        AttributeDomain::Mesh,
        AttributeData::String(vec!["minecraft:block/stone".to_string()]),
    ));
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_IDX,
        AttributeDomain::Face,
        AttributeData::Int32(vec![0]),
    ));
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_AO,
        AttributeDomain::Corner,
        AttributeData::Float(vec![0.25, 0.5, 0.75, 1.0]),
    ));
    mesh
}

fn make_sample_tri_mesh() -> MeshData {
    use crate::attributes::constants::*;
    let mut mesh = MeshData {
        positions: vec![[2.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.5, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        indices: vec![0, 1, 2],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
        uv_layout: UvLayout::PerLoop,
        secondary_uvs: Some(vec![[0.3, 0.3], [0.4, 0.3], [0.35, 0.4]]),
        colors: Some(vec![[0.5, 0.5, 0.5, 1.0]; 3]),
        face_materials: vec![9],
        face_tint_indices: vec![-1],
        quad_indices: None,
        custom_attributes: HashMap::new(),
    };
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_PALETTE,
        AttributeDomain::Mesh,
        AttributeData::String(vec!["mod:block/obj_part".to_string()]),
    ));
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_SOURCE_TEXTURE_IDX,
        AttributeDomain::Face,
        AttributeData::Int32(vec![0]),
    ));
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_AO,
        AttributeDomain::Corner,
        AttributeData::Float(vec![0.9, 0.8, 0.7]),
    ));
    mesh
}

#[test]
fn test_convert_to_triangles_expands_both_diagonal_orientations() {
    use crate::attributes::constants::*;

    let mut standard = make_sample_quad_mesh(false);
    standard.convert_to_triangles();
    assert!(!standard.is_quad_topology());
    assert!(standard.quad_indices.is_none());
    assert_eq!(standard.triangle_count(), 2);
    assert_eq!(standard.face_materials, vec![7, 7]);
    assert_eq!(standard.face_tint_indices, vec![2, 2]);
    assert_eq!(
        standard.uvs,
        vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ]
    );
    assert_eq!(standard.colors.as_ref().unwrap().len(), 6);
    assert_eq!(
        standard
            .get_custom_attribute(ATTR_SOURCE_TEXTURE_IDX)
            .unwrap()
            .data,
        AttributeData::Int32(vec![0, 0])
    );
    assert_eq!(
        standard.get_custom_attribute(ATTR_AO).unwrap().data,
        AttributeData::Float(vec![0.25, 0.5, 0.75, 0.25, 0.75, 1.0])
    );

    let mut flipped = make_sample_quad_mesh(true);
    flipped.convert_to_triangles();
    assert_eq!(
        flipped.uvs,
        vec![
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
        ]
    );
    assert_eq!(
        flipped.get_custom_attribute(ATTR_AO).unwrap().data,
        AttributeData::Float(vec![0.5, 0.75, 1.0, 0.25, 0.5, 1.0])
    );
}

#[test]
fn test_mixed_quad_and_triangle_merging_all_paths() {
    use crate::attributes::constants::*;

    let quad = make_sample_quad_mesh(false);
    let tri = make_sample_tri_mesh();

    // 1. append_mesh (quad + tri)
    let mut appended = quad.clone();
    appended.append_mesh(&tri);
    assert!(!appended.is_quad_topology());
    assert!(appended.quad_indices.is_none());
    assert_eq!(appended.triangle_count(), 3);
    assert_eq!(appended.face_materials, vec![7, 7, 9]);
    assert_eq!(appended.uvs.len(), 9);
    assert_eq!(appended.colors.as_ref().unwrap().len(), 9);
    assert_eq!(
        appended
            .get_custom_attribute(ATTR_SOURCE_TEXTURE_IDX)
            .unwrap()
            .len(),
        3
    );
    assert_eq!(appended.get_custom_attribute(ATTR_AO).unwrap().len(), 9);

    // 2. merge_all_refs (tri + quad)
    let merged = MeshData::merge_all_refs(&[&tri, &quad]);
    assert!(!merged.is_quad_topology());
    assert!(merged.quad_indices.is_none());
    assert_eq!(merged.triangle_count(), 3);
    assert_eq!(merged.face_materials, vec![9, 7, 7]);
    assert_eq!(merged.uvs.len(), 9);
    assert_eq!(merged.colors.as_ref().unwrap().len(), 9);

    // 3. merge_welded_sections (quad + tri)
    let welded = MeshData::merge_welded_sections(&[&quad, &tri], 1e-4);
    assert!(!welded.is_quad_topology());
    assert!(welded.quad_indices.is_none());
    assert_eq!(welded.triangle_count(), 3);
    assert_eq!(welded.face_materials, vec![7, 7, 9]);
    assert_eq!(welded.uvs.len(), 9);
}

#[test]
fn test_weld_vertices_avoids_duplicate_face_index_sets() {
    use std::collections::HashSet;

    // Two overlapping triangles sharing coordinates (opposite winding) plus an adjacent shared triangle
    let mut tri_mesh = MeshData {
        positions: vec![
            // Face 0 (overlay A)
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            // Face 1 (coplanar back/overlay at identical positions)
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            // Face 2 (adjacent triangle sharing edge [1,0,0]-[0,1,0])
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 9],
        indices: vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
        uvs: vec![[0.0, 0.0]; 9],
        uv_layout: UvLayout::PerLoop,
        face_materials: vec![0, 0, 0],
        face_tint_indices: vec![-1, -1, -1],
        quad_indices: None,
        ..Default::default()
    };

    tri_mesh.weld_vertices(1e-4);
    // Adjacent edge welded cleanly, while colliding overlay split 1 vertex -> 5 unique vertices instead of 4 or 9
    assert_eq!(tri_mesh.vertex_count(), 5);
    let mut tri_sets = HashSet::new();
    for tri in tri_mesh.indices.as_chunks::<3>().0 {
        let mut s = [tri[0], tri[1], tri[2]];
        s.sort_unstable();
        assert!(tri_sets.insert(s), "duplicate unordered triangle set {s:?}");
    }

    // Two identical-position overlapping quads
    let mut quad_mesh = MeshData {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 8],
        indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
        quad_indices: Some(vec![0, 1, 2, 3, 4, 5, 6, 7]),
        uvs: vec![[0.0, 0.0]; 8],
        uv_layout: UvLayout::PerLoop,
        face_materials: vec![0, 0],
        face_tint_indices: vec![-1, -1],
        ..Default::default()
    };

    quad_mesh.weld_vertices(1e-4);
    assert_eq!(quad_mesh.vertex_count(), 5);
    let quads = quad_mesh.quad_indices.as_ref().unwrap();
    let mut quad_sets = HashSet::new();
    for q in quads.as_chunks::<4>().0 {
        let mut s = [q[0], q[1], q[2], q[3]];
        s.sort_unstable();
        assert!(quad_sets.insert(s), "duplicate unordered quad set {s:?}");
    }
}
