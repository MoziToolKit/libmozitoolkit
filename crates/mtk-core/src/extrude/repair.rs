//! # Mesh Topology Extrude UV Repair
//!
//! Provides topological edge adjacency search, side UV inward/outward reconstruction,
//! and feature edge crease tagging.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use glam::Vec2;

use super::types::{
    compute_face_normal, ExtrudeMeshOutput, ExtrudeUvMode, FlatPolygonMesh, MeshExtrudeRepairConfig,
};
use super::uv::repair_extruded_side_uv_advanced;
use crate::geometry::Aabb2d;
use crate::uv::is_uv_collapsed_2d as is_uv_collapsed;

/// Performs complete batch UV repair and crease assignment across a mesh using contiguous flat buffers.
pub fn process_flat_mesh_extrude_repair(
    mesh: &FlatPolygonMesh,
    selected_faces: &[u32],
    pixel_steps: &[[f32; 2]],
    config: &MeshExtrudeRepairConfig,
    target_side_faces: Option<&[u32]>,
) -> ExtrudeMeshOutput {
    let mut output = ExtrudeMeshOutput::default();
    if !config.repair_uv && !config.add_crease {
        return output;
    }

    let selected_faces_set: BTreeSet<u32> = selected_faces.iter().copied().collect();
    if selected_faces_set.is_empty() {
        return output;
    }

    // Build edge -> list of (face_index, edge_vert_idx_in_face)
    let mut edge_to_faces: BTreeMap<(u32, u32), Vec<(u32, usize)>> = BTreeMap::new();
    let num_faces = mesh.face_count();
    for f_idx in 0..num_faces {
        let f_verts = mesh.face_vertices(f_idx);
        let n = f_verts.len();
        for i in 0..n {
            let v1 = f_verts[i];
            let v2 = f_verts[(i + 1) % n];
            let edge_key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
            edge_to_faces
                .entry(edge_key)
                .or_default()
                .push((f_idx as u32, i));
        }
    }

    // Edge crease tracking
    let mut modified_creases: BTreeMap<(u32, u32), f32> = BTreeMap::new();
    let mut modified_uvs: BTreeMap<u32, Vec<[f32; 2]>> = BTreeMap::new();
    let mut modified_mats: BTreeMap<u32, u32> = BTreeMap::new();

    for &top_face_idx in selected_faces {
        let top_face_idx_usize = top_face_idx as usize;
        if top_face_idx_usize >= num_faces {
            continue;
        }

        let top_verts = mesh.face_vertices(top_face_idx_usize);
        let top_uvs = mesh.face_uvs(top_face_idx_usize);

        if top_verts.len() < 3 || top_uvs.len() != top_verts.len() {
            continue;
        }

        let [step_u, step_v] = if top_face_idx_usize < pixel_steps.len() {
            pixel_steps[top_face_idx_usize]
        } else {
            [1.0 / 64.0, 1.0 / 64.0]
        };

        // Calculate top face UV bounds
        let mut min_u = f32::MAX;
        let mut max_u = f32::MIN;
        let mut min_v = f32::MAX;
        let mut max_v = f32::MIN;
        let mut uv_sum_u = 0.0f32;
        let mut uv_sum_v = 0.0f32;
        let mut vert_to_uv: BTreeMap<u32, [f32; 2]> = BTreeMap::new();

        for (i, &v) in top_verts.iter().enumerate() {
            let uv = top_uvs[i];
            vert_to_uv.insert(v, uv);
            min_u = min_u.min(uv[0]);
            max_u = max_u.max(uv[0]);
            min_v = min_v.min(uv[1]);
            max_v = max_v.max(uv[1]);
            uv_sum_u += uv[0];
            uv_sum_v += uv[1];
        }

        let top_face_bounds = Aabb2d::new(Vec2::new(min_u, min_v), Vec2::new(max_u, max_v));
        let top_face_uv_center = [
            uv_sum_u / top_verts.len() as f32,
            uv_sum_v / top_verts.len() as f32,
        ];
        let top_normal = compute_face_normal(top_verts, &mesh.positions);
        let top_material = mesh
            .face_materials
            .get(top_face_idx_usize)
            .copied()
            .unwrap_or(0);

        let n = top_verts.len();
        for i in 0..n {
            let v_top_a = top_verts[i];
            let v_top_b = top_verts[(i + 1) % n];
            let edge_key = if v_top_a < v_top_b {
                (v_top_a, v_top_b)
            } else {
                (v_top_b, v_top_a)
            };

            let linked_faces = match edge_to_faces.get(&edge_key) {
                Some(lf) => lf,
                None => continue,
            };

            for &(side_face_idx, _) in linked_faces {
                if side_face_idx == top_face_idx || selected_faces_set.contains(&side_face_idx) {
                    continue;
                }

                let side_face_idx_usize = side_face_idx as usize;
                if side_face_idx_usize >= num_faces {
                    continue;
                }
                let side_verts = mesh.face_vertices(side_face_idx_usize);
                if side_verts.len() != 4 {
                    continue;
                }

                // Material sync
                let side_mat = mesh
                    .face_materials
                    .get(side_face_idx_usize)
                    .copied()
                    .unwrap_or(0);
                if side_mat != top_material {
                    modified_mats.insert(side_face_idx, top_material);
                }

                // Identify base verts
                let base_verts: Vec<u32> = side_verts
                    .iter()
                    .copied()
                    .filter(|&v| v != v_top_a && v != v_top_b)
                    .collect();

                if base_verts.len() != 2 {
                    continue;
                }

                // Find side edges to link v_top_a -> v_base_a and v_top_b -> v_base_b
                let mut v_base_a = None;
                let mut v_base_b = None;
                for j in 0..4 {
                    let sv1 = side_verts[j];
                    let sv2 = side_verts[(j + 1) % 4];
                    if sv1 == v_top_a && base_verts.contains(&sv2) {
                        v_base_a = Some(sv2);
                    } else if sv2 == v_top_a && base_verts.contains(&sv1) {
                        v_base_a = Some(sv1);
                    }
                    if sv1 == v_top_b && base_verts.contains(&sv2) {
                        v_base_b = Some(sv2);
                    } else if sv2 == v_top_b && base_verts.contains(&sv1) {
                        v_base_b = Some(sv1);
                    }
                }

                let (v_base_a, v_base_b) = match (v_base_a, v_base_b) {
                    (Some(ba), Some(bb)) if ba != bb => (ba, bb),
                    _ => continue,
                };

                let uv_a = match vert_to_uv.get(&v_top_a) {
                    Some(&uv) => uv,
                    None => continue,
                };
                let uv_b = match vert_to_uv.get(&v_top_b) {
                    Some(&uv) => uv,
                    None => continue,
                };

                let cur_side_uvs = modified_uvs
                    .get(&side_face_idx)
                    .map(|v| v.as_slice())
                    .unwrap_or_else(|| mesh.face_uvs(side_face_idx_usize));

                if config.only_collapsed {
                    let is_tracked = target_side_faces
                        .map(|s| s.contains(&side_face_idx))
                        .unwrap_or(false);
                    if !is_tracked && !is_uv_collapsed(cur_side_uvs, Some([step_u, step_v])) {
                        continue;
                    }
                }

                let pos_ta = mesh.positions[v_top_a as usize];
                let pos_tb = mesh.positions[v_top_b as usize];
                let pos_ba = mesh.positions[v_base_a as usize];
                let pos_bb = mesh.positions[v_base_b as usize];
                let ext_vec = [
                    ((pos_ta[0] - pos_ba[0]) + (pos_tb[0] - pos_ba[0])) * 0.5,
                    ((pos_ta[1] - pos_ba[1]) + (pos_tb[1] - pos_bb[1])) * 0.5,
                    ((pos_ta[2] - pos_ba[2]) + (pos_tb[2] - pos_bb[2])) * 0.5,
                ];

                // Resolve UV mode
                let resolved_mode = match config.uv_mode {
                    ExtrudeUvMode::Smart => {
                        let dot = ext_vec[0] * top_normal[0]
                            + ext_vec[1] * top_normal[1]
                            + ext_vec[2] * top_normal[2];
                        if dot < -1e-6 {
                            ExtrudeUvMode::Outward
                        } else {
                            ExtrudeUvMode::Inward
                        }
                    }
                    other => other,
                };

                // Check outward adjacent face strip
                let mut adjacent_strip = None;
                if resolved_mode == ExtrudeUvMode::Outward {
                    let base_edge_key = if v_base_a < v_base_b {
                        (v_base_a, v_base_b)
                    } else {
                        (v_base_b, v_base_a)
                    };
                    if let Some(base_linked) = edge_to_faces.get(&base_edge_key) {
                        for &(adj_f_idx, _) in base_linked {
                            if adj_f_idx != side_face_idx
                                && !selected_faces_set.contains(&adj_f_idx)
                            {
                                let adj_f_idx_u = adj_f_idx as usize;
                                let adj_mat =
                                    mesh.face_materials.get(adj_f_idx_u).copied().unwrap_or(0);
                                if adj_mat == top_material && adj_f_idx_u < num_faces {
                                    let adj_verts = mesh.face_vertices(adj_f_idx_u);
                                    let adj_uvs = mesh.face_uvs(adj_f_idx_u);
                                    let mut adj_map = BTreeMap::new();
                                    let mut adj_sum_u = 0.0f32;
                                    let mut adj_sum_v = 0.0f32;
                                    let mut adj_min_u = f32::MAX;
                                    let mut adj_max_u = f32::MIN;
                                    let mut adj_min_v = f32::MAX;
                                    let mut adj_max_v = f32::MIN;
                                    for (ai, &av) in adj_verts.iter().enumerate() {
                                        let auv = adj_uvs.get(ai).copied().unwrap_or([0.0, 0.0]);
                                        adj_map.insert(av, auv);
                                        adj_sum_u += auv[0];
                                        adj_sum_v += auv[1];
                                        adj_min_u = adj_min_u.min(auv[0]);
                                        adj_max_u = adj_max_u.max(auv[0]);
                                        adj_min_v = adj_min_v.min(auv[1]);
                                        adj_max_v = adj_max_v.max(auv[1]);
                                    }
                                    if let (Some(&adj_uva), Some(&adj_uvb)) =
                                        (adj_map.get(&v_base_a), adj_map.get(&v_base_b))
                                    {
                                        let edge_uv =
                                            [adj_uvb[0] - adj_uva[0], adj_uvb[1] - adj_uva[1]];
                                        let edge_len = (edge_uv[0] * edge_uv[0]
                                            + edge_uv[1] * edge_uv[1])
                                            .sqrt();
                                        if edge_len > 1e-6 {
                                            let adj_cnt = adj_verts.len() as f32;
                                            let adj_mid = [
                                                (adj_uva[0] + adj_uvb[0]) * 0.5,
                                                (adj_uva[1] + adj_uvb[1]) * 0.5,
                                            ];
                                            let mut in_dir =
                                                [-edge_uv[1] / edge_len, edge_uv[0] / edge_len];
                                            if in_dir[0] * (adj_sum_u / adj_cnt - adj_mid[0])
                                                + in_dir[1] * (adj_sum_v / adj_cnt - adj_mid[1])
                                                < 0.0
                                            {
                                                in_dir = [-in_dir[0], -in_dir[1]];
                                            }
                                            let offset_u = in_dir[0] * (step_u * 0.1);
                                            let offset_v = in_dir[1] * (step_v * 0.1);
                                            let pad_u = (step_u * 0.05)
                                                .min((adj_max_u - adj_min_u).abs() * 0.1);
                                            let pad_v = (step_v * 0.05)
                                                .min((adj_max_v - adj_min_v).abs() * 0.1);
                                            let min_su = adj_min_u + pad_u;
                                            let max_su = adj_max_u - pad_u;
                                            let min_sv = adj_min_v + pad_v;
                                            let max_sv = adj_max_v - pad_v;

                                            let mut base_a = adj_uva;
                                            let mut base_b = adj_uvb;
                                            let mut top_a =
                                                [base_a[0] + offset_u, base_a[1] + offset_v];
                                            let mut top_b =
                                                [base_b[0] + offset_u, base_b[1] + offset_v];

                                            if max_su >= min_su {
                                                base_a[0] = base_a[0].clamp(min_su, max_su);
                                                base_b[0] = base_b[0].clamp(min_su, max_su);
                                                top_a[0] = top_a[0].clamp(min_su, max_su);
                                                top_b[0] = top_b[0].clamp(min_su, max_su);
                                            }
                                            if max_sv >= min_sv {
                                                base_a[1] = base_a[1].clamp(min_sv, max_sv);
                                                base_b[1] = base_b[1].clamp(min_sv, max_sv);
                                                top_a[1] = top_a[1].clamp(min_sv, max_sv);
                                                top_b[1] = top_b[1].clamp(min_sv, max_sv);
                                            }

                                            adjacent_strip = Some((base_a, base_b, top_a, top_b));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let adj_arr = adjacent_strip.map(|(ba, bb, ta, tb)| [ba, bb, tb, ta]);
                let repaired_quad = repair_extruded_side_uv_advanced(
                    uv_a,
                    uv_b,
                    top_normal,
                    ext_vec,
                    config.uv_mode,
                    step_u,
                    step_v,
                    top_face_bounds,
                    Some(top_face_uv_center),
                    adj_arr,
                );

                if config.repair_uv {
                    // Match corner vertices to repaired quad UVs
                    let mut new_uvs = cur_side_uvs.to_vec();
                    for k in 0..4 {
                        let sv = side_verts[k];
                        if sv == v_top_a {
                            new_uvs[k] = repaired_quad[0];
                        } else if sv == v_top_b {
                            new_uvs[k] = repaired_quad[1];
                        } else if sv == v_base_b {
                            new_uvs[k] = repaired_quad[2];
                        } else if sv == v_base_a {
                            new_uvs[k] = repaired_quad[3];
                        }
                    }
                    modified_uvs.insert(side_face_idx, new_uvs);
                }

                // Crease tracking
                if config.add_crease {
                    let side_edge_a = if v_top_a < v_base_a {
                        (v_top_a, v_base_a)
                    } else {
                        (v_base_a, v_top_a)
                    };
                    let side_edge_b = if v_top_b < v_base_b {
                        (v_top_b, v_base_b)
                    } else {
                        (v_base_b, v_top_b)
                    };
                    modified_creases.insert(edge_key, config.crease_val);
                    modified_creases.insert(side_edge_a, config.crease_val);
                    modified_creases.insert(side_edge_b, config.crease_val);
                }
            }
        }
    }

    output.repaired_count = modified_uvs.len();
    output.modified_face_uvs = modified_uvs.into_iter().collect();
    output.modified_face_materials = modified_mats.into_iter().collect();
    output.modified_edges = modified_creases.keys().copied().collect();
    output.modified_edge_creases = modified_creases.into_iter().collect();

    output
}
