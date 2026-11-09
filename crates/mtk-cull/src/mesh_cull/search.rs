use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use std::collections::HashMap;

use mtk_core::MeshData;

use super::MeshCullConfig;
use crate::geometry::{check_coplanar_overlap, CoplanarOverlap, FaceAlignment};

/// Finds all face indices that should be culled using spatial plane bucketing.
pub(crate) fn find_faces_to_cull(
    mesh: &MeshData,
    face_count: usize,
    poly_step: usize,
    is_quad_mesh: bool,
    config: &MeshCullConfig,
) -> BTreeSet<usize> {
    let tol = if config.tolerance > 0.0 {
        config.tolerance
    } else {
        1e-3
    };
    let inv_tol = 1.0 / tol;

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

                    if let Some(rel) =
                        check_coplanar_overlap(&face_verts, normal, other_verts, *other_norm, tol)
                    {
                        let is_same_dir = rel.alignment == FaceAlignment::SameDirection;
                        let is_opp_dir = rel.alignment == FaceAlignment::OppositeDirection;

                        match rel.overlap {
                            CoplanarOverlap::Exact => {
                                if config.cull_duplicates && is_same_dir {
                                    faces_to_cull.insert(face_idx);
                                    break 'search;
                                } else if config.cull_coplanar_opposite && is_opp_dir {
                                    faces_to_cull.insert(face_idx);
                                    faces_to_cull.insert(*other_idx);
                                    break 'search;
                                }
                            }
                            CoplanarOverlap::ContainedInB => {
                                // Face A is completely covered by other face B back-to-back
                                if config.cull_coplanar_opposite && is_opp_dir {
                                    faces_to_cull.insert(face_idx);
                                    break 'search;
                                }
                            }
                            CoplanarOverlap::ContainedInA
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
}
