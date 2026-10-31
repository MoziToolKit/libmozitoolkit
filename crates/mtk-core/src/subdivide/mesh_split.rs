//! # Mesh-Level Adaptive Pixel Grid Subdivider
//!
//! Applies pixel-density aware quad face subdivision to a `MeshData` buffer,
//! preserving quad topology for axis-aligned UV tiles while strictly slicing
//! rotated and skewed UVs along texture pixel grid lines.

use alloc::vec;
use alloc::vec::Vec;

use super::clip::slice_face_by_pixel_grid;
use super::grid::{
    calculate_face_target_grid, calculate_pixel_grid_cut_factors, interpolate_bilinear_2d,
    interpolate_bilinear_3d, interpolate_bilinear_4d,
};
use super::quad::{emit_quad_copy, QuadAccumulator};
use crate::attributes::MeshAttribute;
use crate::mesh::{MeshData, UvLayout};

/// Checks if 4 UV quad corners form an axis-aligned rectangle in texture pixel space
/// (including 0, 90, 180, and 270 degree rotations).
fn is_axis_aligned_uv_rect(uvs: &[[f32; 2]; 4], tex_w: u32, tex_h: u32) -> bool {
    let p0 = [uvs[0][0] * tex_w as f32, uvs[0][1] * tex_h as f32];
    let p1 = [uvs[1][0] * tex_w as f32, uvs[1][1] * tex_h as f32];
    let p2 = [uvs[2][0] * tex_w as f32, uvs[2][1] * tex_h as f32];
    let p3 = [uvs[3][0] * tex_w as f32, uvs[3][1] * tex_h as f32];

    let v01 = [p1[0] - p0[0], p1[1] - p0[1]];
    let v32 = [p2[0] - p3[0], p2[1] - p3[1]];
    let v03 = [p3[0] - p0[0], p3[1] - p0[1]];
    let v12 = [p2[0] - p1[0], p2[1] - p1[1]];

    // Parallelogram check (opposite edges equal within tolerance)
    let is_para = (v01[0] - v32[0]).abs() < 1e-2
        && (v01[1] - v32[1]).abs() < 1e-2
        && (v03[0] - v12[0]).abs() < 1e-2
        && (v03[1] - v12[1]).abs() < 1e-2;

    if !is_para {
        return false;
    }

    // Perpendicular check
    let dot = v01[0] * v03[0] + v01[1] * v03[1];
    let len_sq01 = v01[0] * v01[0] + v01[1] * v01[1];
    let len_sq03 = v03[0] * v03[0] + v03[1] * v03[1];
    if len_sq01 > 1e-4 && len_sq03 > 1e-4 {
        let cos_angle_sq = (dot * dot) / (len_sq01 * len_sq03);
        if cos_angle_sq > 1e-3 {
            return false;
        }
    }

    // Axis alignment check: either v01 horizontal (dy ≈ 0) & v03 vertical (dx ≈ 0),
    // OR v01 vertical (dx ≈ 0) & v03 horizontal (dy ≈ 0)
    let case_a = v01[1].abs() < 1e-2 && v03[0].abs() < 1e-2;
    let case_b = v01[0].abs() < 1e-2 && v03[1].abs() < 1e-2;
    case_a || case_b
}

/// Performs adaptive pixel grid subdivision on quad faces in a `MeshData` buffer.
///
/// For axis-aligned and 90-degree rotated UV tiles, performs high-performance quad
/// grid subdivision with integer pixel snapping. For arbitrarily rotated or skewed
/// UVs, performs exact 2D texture pixel-grid polygon slicing.
pub fn adaptive_pixel_split_mesh(
    mesh: &MeshData,
    face_resolutions: &[Option<(u32, u32)>],
    default_resolution: (u32, u32),
    pixels_per_face: f32,
    max_subdivisions: u32,
    weld_dist: f32,
) -> MeshData {
    let face_count = if mesh.is_quad_topology() {
        mesh.quad_count()
    } else {
        mesh.indices.len() / 6
    };
    if face_count == 0 {
        return mesh.clone();
    }

    let mut out = MeshData::new();
    if mesh.secondary_uvs.is_some() {
        out.secondary_uvs = Some(Vec::new());
    }
    if mesh.colors.is_some() {
        out.colors = Some(Vec::new());
    }

    // Clone custom attribute headers
    for (name, attr) in &mesh.custom_attributes {
        out.add_custom_attribute(MeshAttribute {
            name: name.clone(),
            domain: attr.domain,
            data: attr.data.empty_with_capacity(0),
        });
    }

    let mut quads = QuadAccumulator::default();

    let mut sub_tri_indices: Vec<[u32; 3]> = Vec::new();
    let mut sub_tri_uvs: Vec<[[f32; 2]; 3]> = Vec::new();
    let mut sub_tri_materials: Vec<u16> = Vec::new();
    let mut sub_tri_tints: Vec<i16> = Vec::new();

    for face_idx in 0..face_count {
        let (idx0, idx1, idx2, idx3) = if let Some(ref quads) = mesh.quad_indices {
            (
                quads[face_idx * 4] as usize,
                quads[face_idx * 4 + 1] as usize,
                quads[face_idx * 4 + 2] as usize,
                quads[face_idx * 4 + 3] as usize,
            )
        } else {
            let base_tri = face_idx * 6;
            (
                mesh.indices[base_tri] as usize,
                mesh.indices[base_tri + 1] as usize,
                mesh.indices[base_tri + 2] as usize,
                mesh.indices[base_tri + 5] as usize,
            )
        };

        let mat_id = mesh.face_materials.get(face_idx).copied().unwrap_or(0);
        let tint_idx = mesh.face_tint_indices.get(face_idx).copied().unwrap_or(-1);

        let p0 = mesh.positions[idx0];
        let p1 = mesh.positions[idx1];
        let p2 = mesh.positions[idx2];
        let p3 = mesh.positions[idx3];

        let n0 = mesh.normals.get(idx0).copied().unwrap_or([0.0, 0.0, 1.0]);
        let n1 = mesh.normals.get(idx1).copied().unwrap_or([0.0, 0.0, 1.0]);
        let n2 = mesh.normals.get(idx2).copied().unwrap_or([0.0, 0.0, 1.0]);
        let n3 = mesh.normals.get(idx3).copied().unwrap_or([0.0, 0.0, 1.0]);

        // Safely extract UVs based on explicit UV layout
        let (uv0, uv1, uv2, uv3) = if mesh.uv_layout == UvLayout::PerLoop {
            let lb = face_idx * 4;
            (
                mesh.uvs.get(lb).copied().unwrap_or([0.0, 0.0]),
                mesh.uvs.get(lb + 1).copied().unwrap_or([1.0, 0.0]),
                mesh.uvs.get(lb + 2).copied().unwrap_or([1.0, 1.0]),
                mesh.uvs.get(lb + 3).copied().unwrap_or([0.0, 1.0]),
            )
        } else {
            (
                mesh.uvs.get(idx0).copied().unwrap_or([0.0, 0.0]),
                mesh.uvs.get(idx1).copied().unwrap_or([1.0, 0.0]),
                mesh.uvs.get(idx2).copied().unwrap_or([1.0, 1.0]),
                mesh.uvs.get(idx3).copied().unwrap_or([0.0, 1.0]),
            )
        };

        let sec_quad = mesh.secondary_uvs.as_ref().map(|sec| {
            if mesh.uv_layout == UvLayout::PerLoop {
                // Never mix layouts: a short PerLoop buffer is a data bug, not a reason
                // to silently read the secondary UVs by vertex index.
                let lb = face_idx * 4;
                debug_assert!(
                    sec.len() >= lb + 4,
                    "PerLoop secondary UV buffer shorter than the loop count"
                );
                (
                    sec.get(lb).copied().unwrap_or([0.0, 0.0]),
                    sec.get(lb + 1).copied().unwrap_or([0.0, 0.0]),
                    sec.get(lb + 2).copied().unwrap_or([0.0, 0.0]),
                    sec.get(lb + 3).copied().unwrap_or([0.0, 0.0]),
                )
            } else {
                (
                    sec.get(idx0).copied().unwrap_or([0.0, 0.0]),
                    sec.get(idx1).copied().unwrap_or([1.0, 0.0]),
                    sec.get(idx2).copied().unwrap_or([1.0, 1.0]),
                    sec.get(idx3).copied().unwrap_or([0.0, 1.0]),
                )
            }
        });

        let col_quad = mesh.colors.as_ref().map(|cols| {
            if mesh.uv_layout == UvLayout::PerLoop {
                let lb = face_idx * 4;
                debug_assert!(
                    cols.len() >= lb + 4,
                    "PerLoop color buffer shorter than the loop count"
                );
                (
                    cols.get(lb).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    cols.get(lb + 1).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    cols.get(lb + 2).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    cols.get(lb + 3).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                )
            } else {
                (
                    cols.get(idx0).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    cols.get(idx1).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    cols.get(idx2).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    cols.get(idx3).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                )
            }
        });

        let resolution = if face_resolutions.is_empty() {
            Some(default_resolution)
        } else {
            face_resolutions.get(face_idx).copied().flatten()
        };

        let (tex_w, tex_h) = match resolution {
            Some(res) if res.0 > 0 && res.1 > 0 => res,
            _ => (0, 0),
        };

        if tex_w == 0 || tex_h == 0 {
            emit_quad_copy(
                &mut out,
                &mut quads,
                [p0, p1, p2, p3],
                [n0, n1, n2, n3],
                sec_quad.map(|(s0, s1, s2, s3)| [s0, s1, s2, s3]),
                col_quad.map(|(c0, c1, c2, c3)| [c0, c1, c2, c3]),
                [uv0, uv1, uv2, uv3],
                mat_id,
                tint_idx,
            );
            continue;
        }

        let is_axis_aligned = is_axis_aligned_uv_rect(&[uv0, uv1, uv2, uv3], tex_w, tex_h);

        if is_axis_aligned {
            let (u_factors, v_factors) = if pixels_per_face <= 1.0 {
                calculate_pixel_grid_cut_factors(
                    &[uv0, uv1, uv2, uv3],
                    tex_w,
                    tex_h,
                    pixels_per_face,
                    max_subdivisions,
                )
            } else {
                let (c_count, r_count) = calculate_face_target_grid(
                    &[uv0, uv1, uv2, uv3],
                    tex_w,
                    tex_h,
                    pixels_per_face,
                    max_subdivisions,
                );
                let u_f = (0..=c_count).map(|c| c as f32 / c_count as f32).collect();
                let v_f = (0..=r_count).map(|r| r as f32 / r_count as f32).collect();
                (u_f, v_f)
            };

            let cols = u_factors.len() - 1;
            let rows = v_factors.len() - 1;

            if cols <= 1 && rows <= 1 {
                emit_quad_copy(
                    &mut out,
                    &mut quads,
                    [p0, p1, p2, p3],
                    [n0, n1, n2, n3],
                    sec_quad.map(|(s0, s1, s2, s3)| [s0, s1, s2, s3]),
                    col_quad.map(|(c0, c1, c2, c3)| [c0, c1, c2, c3]),
                    [uv0, uv1, uv2, uv3],
                    mat_id,
                    tint_idx,
                );
                continue;
            }

            let mut grid_indices: Vec<Vec<u32>> = vec![vec![0; cols + 1]; rows + 1];

            for r in 0..=rows {
                let v_factor = v_factors[r];
                for c in 0..=cols {
                    let u_factor = u_factors[c];
                    let vert_idx = out.positions.len() as u32;
                    grid_indices[r][c] = vert_idx;

                    let pos = interpolate_bilinear_3d(p0, p1, p2, p3, u_factor, v_factor);
                    let norm = interpolate_bilinear_3d(n0, n1, n2, n3, u_factor, v_factor);

                    out.positions.push(pos);
                    out.normals.push(norm);

                    if let Some((s0, s1, s2, s3)) = sec_quad {
                        out.secondary_uvs
                            .as_mut()
                            .unwrap()
                            .push(interpolate_bilinear_2d(s0, s1, s2, s3, u_factor, v_factor));
                    }

                    if let Some((c0, c1, c2, c3)) = col_quad {
                        out.colors
                            .as_mut()
                            .unwrap()
                            .push(interpolate_bilinear_4d(c0, c1, c2, c3, u_factor, v_factor));
                    }
                }
            }

            for r in 0..rows {
                for c in 0..cols {
                    let v00 = grid_indices[r][c];
                    let v10 = grid_indices[r][c + 1];
                    let v11 = grid_indices[r + 1][c + 1];
                    let v01 = grid_indices[r + 1][c];

                    let uv_00 =
                        interpolate_bilinear_2d(uv0, uv1, uv2, uv3, u_factors[c], v_factors[r]);
                    let uv_10 =
                        interpolate_bilinear_2d(uv0, uv1, uv2, uv3, u_factors[c + 1], v_factors[r]);
                    let uv_11 = interpolate_bilinear_2d(
                        uv0,
                        uv1,
                        uv2,
                        uv3,
                        u_factors[c + 1],
                        v_factors[r + 1],
                    );
                    let uv_01 =
                        interpolate_bilinear_2d(uv0, uv1, uv2, uv3, u_factors[c], v_factors[r + 1]);

                    quads.push(
                        [v00, v10, v11, v01],
                        [uv_00, uv_10, uv_11, uv_01],
                        mat_id,
                        tint_idx,
                    );
                }
            }
        } else {
            // Arbitrarily rotated or skewed UV: strict 2D texture pixel grid polygon slicing
            let sliced = slice_face_by_pixel_grid(
                &[p0, p1, p2, p3],
                &[uv0, uv1, uv2, uv3],
                tex_w,
                tex_h,
                pixels_per_face,
                max_subdivisions,
            );

            if sliced.faces.len() <= 1 {
                emit_quad_copy(
                    &mut out,
                    &mut quads,
                    [p0, p1, p2, p3],
                    [n0, n1, n2, n3],
                    sec_quad.map(|(s0, s1, s2, s3)| [s0, s1, s2, s3]),
                    col_quad.map(|(c0, c1, c2, c3)| [c0, c1, c2, c3]),
                    [uv0, uv1, uv2, uv3],
                    mat_id,
                    tint_idx,
                );
                continue;
            }

            let base_v = out.positions.len() as u32;
            for (k, &pos) in sliced.positions.iter().enumerate() {
                let st = sliced.param_coords[k];
                let norm = interpolate_bilinear_3d(n0, n1, n2, n3, st[0], st[1]);
                out.positions.push(pos);
                out.normals.push(norm);

                if let Some((s0, s1, s2, s3)) = sec_quad {
                    out.secondary_uvs
                        .as_mut()
                        .unwrap()
                        .push(interpolate_bilinear_2d(s0, s1, s2, s3, st[0], st[1]));
                }

                if let Some((c0, c1, c2, c3)) = col_quad {
                    out.colors
                        .as_mut()
                        .unwrap()
                        .push(interpolate_bilinear_4d(c0, c1, c2, c3, st[0], st[1]));
                }
            }

            for poly in &sliced.faces {
                if poly.len() == 4 {
                    let v0 = base_v + poly[0];
                    let v1 = base_v + poly[1];
                    let v2 = base_v + poly[2];
                    let v3 = base_v + poly[3];
                    quads.push(
                        [v0, v1, v2, v3],
                        [
                            sliced.uvs[poly[0] as usize],
                            sliced.uvs[poly[1] as usize],
                            sliced.uvs[poly[2] as usize],
                            sliced.uvs[poly[3] as usize],
                        ],
                        mat_id,
                        tint_idx,
                    );
                } else if poly.len() == 3 {
                    let v0 = base_v + poly[0];
                    let v1 = base_v + poly[1];
                    let v2 = base_v + poly[2];
                    sub_tri_indices.push([v0, v1, v2]);
                    sub_tri_uvs.push([
                        sliced.uvs[poly[0] as usize],
                        sliced.uvs[poly[1] as usize],
                        sliced.uvs[poly[2] as usize],
                    ]);
                    sub_tri_materials.push(mat_id);
                    sub_tri_tints.push(tint_idx);
                }
            }
        }
    }

    if sub_tri_indices.is_empty() {
        // Pure quad mesh
        let quad_count = quads.indices.len();
        let mut flat_quads = Vec::with_capacity(quad_count * 4);
        out.indices.reserve(quad_count * 6);
        out.uvs.reserve(quad_count * 4);

        for i in 0..quad_count {
            let q = quads.indices[i];
            let u = quads.uvs[i];
            flat_quads.extend_from_slice(&q);
            out.indices
                .extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
            out.uvs.extend_from_slice(&u);
        }

        out.quad_indices = Some(flat_quads);
        out.face_materials = quads.materials;
        out.face_tint_indices = quads.tints;
    } else {
        // Triangulate quads to guarantee uniform triangle topology for DCC injection
        let total_tris = quads.indices.len() * 2 + sub_tri_indices.len();
        out.indices.reserve(total_tris * 3);
        out.uvs.reserve(total_tris * 3);
        out.face_materials.reserve(total_tris);
        out.face_tint_indices.reserve(total_tris);

        for i in 0..quads.indices.len() {
            let q = quads.indices[i];
            let u = quads.uvs[i];
            let mat = quads.materials[i];
            let tint = quads.tints[i];

            out.indices.extend_from_slice(&[q[0], q[1], q[2]]);
            out.uvs.extend_from_slice(&[u[0], u[1], u[2]]);
            out.face_materials.push(mat);
            out.face_tint_indices.push(tint);

            out.indices.extend_from_slice(&[q[0], q[2], q[3]]);
            out.uvs.extend_from_slice(&[u[0], u[2], u[3]]);
            out.face_materials.push(mat);
            out.face_tint_indices.push(tint);
        }

        for i in 0..sub_tri_indices.len() {
            let t = sub_tri_indices[i];
            let u = sub_tri_uvs[i];
            let mat = sub_tri_materials[i];
            let tint = sub_tri_tints[i];

            out.indices.extend_from_slice(&t);
            out.uvs.extend_from_slice(&u);
            out.face_materials.push(mat);
            out.face_tint_indices.push(tint);
        }

        out.quad_indices = None;
    }

    out.uv_layout = UvLayout::PerLoop;

    if weld_dist > 0.0 {
        out.weld_vertices(weld_dist);
    }

    out
}
