//! # Mesh Face Culling for Imported / Arbitrary 3D Geometries
//!
//! High-performance spatial-hashing based face culling engine for external mesh imports
//! (e.g. Mineways, jmc2obj, Blockbench OBJ). Detects coplanar opposite-facing polygons
//! and full 2D projected occlusion to eliminate interior unseen faces.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use std::collections::HashMap;

use mtk_core::MeshData;
use mtk_core::{AttributeData, MeshAttribute};

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

/// Copies a single element from src AttributeData to dst AttributeData at index.
fn copy_attribute_element(src: &AttributeData, dst: &mut AttributeData, idx: usize) {
    match (src, dst) {
        (AttributeData::Float(s), AttributeData::Float(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Float2(s), AttributeData::Float2(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Float3(s), AttributeData::Float3(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Float4(s), AttributeData::Float4(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Int8(s), AttributeData::Int8(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Int16(s), AttributeData::Int16(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Int32(s), AttributeData::Int32(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::UInt8(s), AttributeData::UInt8(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::UInt16(s), AttributeData::UInt16(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::UInt32(s), AttributeData::UInt32(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::String(s), AttributeData::String(d)) => {
            if let Some(v) = s.get(idx) {
                d.push(v.clone());
            }
        }
        (AttributeData::Bool(s), AttributeData::Bool(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        _ => {}
    }
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

    let tol = if config.tolerance > 0.0 {
        config.tolerance
    } else {
        1e-3
    };
    let inv_tol = 1.0 / tol;

    let faces_to_cull =
        profiler.measure_with_count("spatial_plane_bucketing_and_search", face_count, || {
            let mut plane_buckets: HashMap<
                [i32; 4],
                Vec<(usize, glam::Vec3, glam::Vec3, glam::Vec3, Vec<glam::Vec3>)>,
            > = HashMap::new();
            let mut faces_to_cull: BTreeSet<usize> = BTreeSet::new();

            for face_idx in 0..face_count {
                let base = face_idx * poly_step;
                let mut center = glam::Vec3::ZERO;

                let num_verts = if is_quad_mesh { 4 } else { 3 };
                let vert_indices = if is_quad_mesh {
                    vec![
                        mesh.indices[base] as usize,
                        mesh.indices[base + 1] as usize,
                        mesh.indices[base + 2] as usize,
                        mesh.indices[base + 5] as usize,
                    ]
                } else {
                    vec![
                        mesh.indices[base] as usize,
                        mesh.indices[base + 1] as usize,
                        mesh.indices[base + 2] as usize,
                    ]
                };

                let mut face_verts = Vec::with_capacity(num_verts);
                let mut aabb_min = glam::Vec3::splat(f32::INFINITY);
                let mut aabb_max = glam::Vec3::splat(f32::NEG_INFINITY);
                for &vi in &vert_indices {
                    let p_raw = mesh.positions.get(vi).copied().unwrap_or([0.0; 3]);
                    let p = glam::Vec3::new(p_raw[0], p_raw[1], p_raw[2]);
                    face_verts.push(p);
                    center += p;
                    aabb_min = aabb_min.min(p);
                    aabb_max = aabb_max.max(p);
                }

                let inv_n = 1.0 / (num_verts as f32);
                center *= inv_n;

                // Derive true geometric face normal from vertex winding
                let normal = if face_verts.len() >= 3 {
                    let e1 = face_verts[1] - face_verts[0];
                    let e2 = face_verts[2] - face_verts[0];
                    let gn = e1.cross(e2).normalize_or_zero();
                    if gn.length_squared() > 1e-5 {
                        gn
                    } else {
                        glam::Vec3::Z
                    }
                } else {
                    glam::Vec3::Z
                };

                let d = -normal.dot(center);

                // Canonical plane: enforce normal has non-negative leading component
                let (c_norm, c_d) = if normal.x < -1e-4
                    || (normal.x.abs() <= 1e-4 && normal.y < -1e-4)
                    || (normal.x.abs() <= 1e-4 && normal.y.abs() <= 1e-4 && normal.z < -1e-4)
                {
                    (-normal, -d)
                } else {
                    (normal, d)
                };

                let plane_key = [
                    (c_norm.x * 10.0).round() as i32,
                    (c_norm.y * 10.0).round() as i32,
                    (c_norm.z * 10.0).round() as i32,
                    (c_d * inv_tol).round() as i32,
                ];

                'search: for delta_d in -1..=1 {
                    let search_key = [
                        plane_key[0],
                        plane_key[1],
                        plane_key[2],
                        plane_key[3] + delta_d,
                    ];
                    if let Some(neighbors) = plane_buckets.get(&search_key) {
                        for (other_idx, other_norm, other_aabb_min, other_aabb_max, other_verts) in
                            neighbors
                        {
                            if faces_to_cull.contains(other_idx) {
                                continue;
                            }

                            // Fast AABB rejection filter
                            if aabb_max.x < other_aabb_min.x - tol
                                || aabb_min.x > other_aabb_max.x + tol
                                || aabb_max.y < other_aabb_min.y - tol
                                || aabb_min.y > other_aabb_max.y + tol
                                || aabb_max.z < other_aabb_min.z - tol
                                || aabb_min.z > other_aabb_max.z + tol
                            {
                                continue;
                            }

                            if let Some(rel) = crate::geometry::coplanar::check_coplanar_overlap(
                                &face_verts,
                                normal,
                                other_verts,
                                *other_norm,
                                tol,
                            ) {
                                let is_same_dir = rel.alignment
                                    == crate::geometry::coplanar::FaceAlignment::SameDirection;
                                let is_opp_dir = rel.alignment
                                    == crate::geometry::coplanar::FaceAlignment::OppositeDirection;

                                match rel.overlap {
                                    crate::geometry::coplanar::CoplanarOverlap::Exact => {
                                        if config.cull_duplicates && is_same_dir {
                                            faces_to_cull.insert(face_idx);
                                            break 'search;
                                        } else if config.cull_coplanar_opposite && is_opp_dir {
                                            faces_to_cull.insert(face_idx);
                                            faces_to_cull.insert(*other_idx);
                                            break 'search;
                                        }
                                    }
                                    crate::geometry::coplanar::CoplanarOverlap::ContainedInB => {
                                        // Face A is completely covered by other face B back-to-back
                                        if config.cull_coplanar_opposite && is_opp_dir {
                                            faces_to_cull.insert(face_idx);
                                            break 'search;
                                        }
                                    }
                                    crate::geometry::coplanar::CoplanarOverlap::ContainedInA
                                        // Other face B is completely covered by Face A back-to-back
                                        if config.cull_coplanar_opposite && is_opp_dir => {
                                            faces_to_cull.insert(*other_idx);
                                        }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                plane_buckets
                    .entry(plane_key)
                    .or_default()
                    .push((face_idx, normal, aabb_min, aabb_max, face_verts));
            }

            faces_to_cull
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
        // Build culled mesh output
        let mut out = MeshData::new();
        if mesh.secondary_uvs.is_some() {
            out.secondary_uvs = Some(Vec::new());
        }
        if mesh.colors.is_some() {
            out.colors = Some(Vec::new());
        }

        for (name, attr) in &mesh.custom_attributes {
            out.add_custom_attribute(MeshAttribute {
                name: name.clone(),
                domain: attr.domain,
                data: attr.data.empty_with_capacity(0),
            });
        }

        // Copy Mesh-domain attributes directly
        for (name, attr) in &mesh.custom_attributes {
            if attr.domain == mtk_core::AttributeDomain::Mesh {
                if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                    out_attr.data = attr.data.clone();
                }
            }
        }

        let mut remaining_faces = 0;
        let mut out_quad_indices = if is_quad_mesh || mesh.quad_indices.is_some() {
            Some(Vec::new())
        } else {
            None
        };

        for face_idx in 0..face_count {
            if faces_to_cull.contains(&face_idx) {
                continue;
            }

            remaining_faces += 1;
            let base = face_idx * poly_step;
            let quad_start_v = out.positions.len() as u32;

            if is_quad_mesh {
                let vert_indices = [
                    mesh.indices[base] as usize,
                    mesh.indices[base + 1] as usize,
                    mesh.indices[base + 2] as usize,
                    mesh.indices[base + 5] as usize,
                ];

                for (step, &orig_idx) in vert_indices.iter().enumerate() {
                    let new_v_idx = out.positions.len() as u32;

                    out.positions.push(mesh.positions[orig_idx]);
                    out.normals.push(
                        mesh.normals
                            .get(orig_idx)
                            .copied()
                            .unwrap_or([0.0, 0.0, 1.0]),
                    );

                    let uv = match mesh.uv_layout {
                        mtk_core::UvLayout::PerLoop => {
                            if mesh.uvs.len() == face_count * 4 {
                                mesh.uvs.get(face_idx * 4 + step).copied()
                            } else if mesh.uvs.len() == face_count * 6 {
                                let tri_step = if step == 3 { 5 } else { step };
                                mesh.uvs.get(base + tri_step).copied()
                            } else {
                                mesh.uvs.get(orig_idx).copied()
                            }
                        }
                        mtk_core::UvLayout::PerVertex | mtk_core::UvLayout::None => {
                            mesh.uvs.get(orig_idx).copied()
                        }
                    };
                    out.uvs.push(uv.unwrap_or([0.0, 0.0]));

                    if let (Some(sec_in), Some(sec_out)) =
                        (&mesh.secondary_uvs, &mut out.secondary_uvs)
                    {
                        sec_out.push(sec_in.get(orig_idx).copied().unwrap_or([0.0, 0.0]));
                    }
                    if let (Some(col_in), Some(col_out)) = (&mesh.colors, &mut out.colors) {
                        col_out.push(
                            col_in
                                .get(orig_idx)
                                .copied()
                                .unwrap_or([1.0, 1.0, 1.0, 1.0]),
                        );
                    }

                    // Copy Point and Corner domain attributes
                    for (name, in_attr) in &mesh.custom_attributes {
                        if in_attr.domain == mtk_core::AttributeDomain::Point {
                            if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                                copy_attribute_element(&in_attr.data, &mut out_attr.data, orig_idx);
                            }
                        } else if in_attr.domain == mtk_core::AttributeDomain::Corner {
                            if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                                let c_idx = if in_attr.data.len() == face_count * 4 {
                                    face_idx * 4 + step
                                } else {
                                    base + if step == 3 { 5 } else { step }
                                };
                                copy_attribute_element(&in_attr.data, &mut out_attr.data, c_idx);
                            }
                        }
                    }

                    let _ = new_v_idx;
                }

                // Quad indices maintenance: 4 corner vertex indices
                if let Some(ref mut q_indices) = out_quad_indices {
                    q_indices.push(quad_start_v);
                    q_indices.push(quad_start_v + 1);
                    q_indices.push(quad_start_v + 2);
                    q_indices.push(quad_start_v + 3);
                }

                // Triangle indices for quads: 2 triangles (0, 1, 2) and (0, 2, 3)
                out.indices.extend_from_slice(&[
                    quad_start_v,
                    quad_start_v + 1,
                    quad_start_v + 2,
                    quad_start_v,
                    quad_start_v + 2,
                    quad_start_v + 3,
                ]);
            } else {
                // Triangle mesh (poly_step == 3)
                for step in 0..3 {
                    let orig_idx = mesh.indices[base + step] as usize;
                    let new_v_idx = out.positions.len() as u32;

                    out.positions.push(mesh.positions[orig_idx]);
                    out.normals.push(
                        mesh.normals
                            .get(orig_idx)
                            .copied()
                            .unwrap_or([0.0, 0.0, 1.0]),
                    );

                    let uv = match mesh.uv_layout {
                        mtk_core::UvLayout::PerLoop => mesh.uvs.get(base + step).copied(),
                        mtk_core::UvLayout::PerVertex | mtk_core::UvLayout::None => {
                            mesh.uvs.get(orig_idx).copied()
                        }
                    };
                    out.uvs.push(uv.unwrap_or([0.0, 0.0]));

                    if let (Some(sec_in), Some(sec_out)) =
                        (&mesh.secondary_uvs, &mut out.secondary_uvs)
                    {
                        sec_out.push(sec_in.get(orig_idx).copied().unwrap_or([0.0, 0.0]));
                    }
                    if let (Some(col_in), Some(col_out)) = (&mesh.colors, &mut out.colors) {
                        col_out.push(
                            col_in
                                .get(orig_idx)
                                .copied()
                                .unwrap_or([1.0, 1.0, 1.0, 1.0]),
                        );
                    }

                    for (name, in_attr) in &mesh.custom_attributes {
                        if in_attr.domain == mtk_core::AttributeDomain::Point {
                            if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                                copy_attribute_element(&in_attr.data, &mut out_attr.data, orig_idx);
                            }
                        } else if in_attr.domain == mtk_core::AttributeDomain::Corner {
                            if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                                copy_attribute_element(
                                    &in_attr.data,
                                    &mut out_attr.data,
                                    base + step,
                                );
                            }
                        }
                    }

                    out.indices.push(new_v_idx);
                }
            }

            if let Some(&mat_id) = mesh.face_materials.get(face_idx) {
                out.face_materials.push(mat_id);
            }
            if let Some(&tint_idx) = mesh.face_tint_indices.get(face_idx) {
                out.face_tint_indices.push(tint_idx);
            }

            // Copy Face domain attributes
            for (name, in_attr) in &mesh.custom_attributes {
                if in_attr.domain == mtk_core::AttributeDomain::Face {
                    if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                        copy_attribute_element(&in_attr.data, &mut out_attr.data, face_idx);
                    }
                }
            }
        }

        out.uv_layout = mesh.uv_layout;
        out.quad_indices = out_quad_indices;
        (out, remaining_faces)
    });

    MeshCullResult {
        initial_faces: face_count,
        culled_faces: faces_to_cull.len(),
        remaining_faces,
        mesh: out,
        profile: Some(profiler.finish()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
