//! # Geometric UV Reconstruction for Extruded Side Faces
//!
//! Provides topological UV corner reconstruction, smart projection, collapsed detection,
//! and Atlas Safe Padding Clamping.

use super::types::ExtrudeUvMode;
use crate::geometry::Aabb2d;

/// Computes reconstructed 4 UV corner coordinates for an extruded side quad with advanced topological awareness.
///
/// Order of returned UVs matches `[uv_base_a, uv_base_b, uv_top_b, uv_top_a]`.
pub(crate) fn repair_extruded_side_uv_advanced(
    uv_base_a: [f32; 2],
    uv_base_b: [f32; 2],
    top_normal: [f32; 3],
    extrude_vec: [f32; 3],
    mode: ExtrudeUvMode,
    step_u: f32,
    step_v: f32,
    top_uv_bounds: Aabb2d,
    top_face_uv_center: Option<[f32; 2]>,
    adjacent_uv_strip: Option<[[f32; 2]; 4]>,
) -> [[f32; 2]; 4] {
    let resolved_mode = match mode {
        ExtrudeUvMode::Smart => {
            let dot = extrude_vec[0] * top_normal[0]
                + extrude_vec[1] * top_normal[1]
                + extrude_vec[2] * top_normal[2];
            if dot < -1e-6 {
                ExtrudeUvMode::Outward
            } else {
                ExtrudeUvMode::Inward
            }
        }
        other => other,
    };

    if resolved_mode == ExtrudeUvMode::Outward {
        if let Some(strip) = adjacent_uv_strip {
            return strip;
        }
    }

    let mut base_a = uv_base_a;
    let mut base_b = uv_base_b;

    let edge_du = uv_base_b[0] - uv_base_a[0];
    let edge_dv = uv_base_b[1] - uv_base_a[1];

    let norm_len = (edge_du * edge_du + edge_dv * edge_dv).sqrt();
    let uv_outward_dir = if norm_len > 1e-6 {
        let mut perp = [-edge_dv / norm_len, edge_du / norm_len];
        if let Some(center) = top_face_uv_center {
            let edge_mid = [
                (uv_base_a[0] + uv_base_b[0]) * 0.5,
                (uv_base_a[1] + uv_base_b[1]) * 0.5,
            ];
            let v_out = [edge_mid[0] - center[0], edge_mid[1] - center[1]];
            if perp[0] * v_out[0] + perp[1] * v_out[1] < 0.0 {
                perp = [-perp[0], -perp[1]];
            }
        }
        perp
    } else {
        [0.0, 1.0]
    };

    // Anisotropic pixel grid boundary alignment on the base edge
    if uv_outward_dir[0].abs() > 0.5 {
        if uv_outward_dir[0] > 0.0 {
            base_a[0] = (uv_base_a[0] / step_u - 1e-5).ceil() * step_u;
            base_b[0] = (uv_base_b[0] / step_u - 1e-5).ceil() * step_u;
        } else {
            base_a[0] = (uv_base_a[0] / step_u + 1e-5).floor() * step_u;
            base_b[0] = (uv_base_b[0] / step_u + 1e-5).floor() * step_u;
        }
    } else if uv_outward_dir[1].abs() > 0.5 {
        if uv_outward_dir[1] > 0.0 {
            base_a[1] = (uv_base_a[1] / step_v - 1e-5).ceil() * step_v;
            base_b[1] = (uv_base_b[1] / step_v - 1e-5).ceil() * step_v;
        } else {
            base_a[1] = (uv_base_a[1] / step_v + 1e-5).floor() * step_v;
            base_b[1] = (uv_base_b[1] / step_v + 1e-5).floor() * step_v;
        }
    }

    let dir_multiplier = match resolved_mode {
        ExtrudeUvMode::Inward => -1.0,
        _ => 1.0,
    };

    let offset_u = uv_outward_dir[0] * dir_multiplier * (step_u * 0.1);
    let offset_v = uv_outward_dir[1] * dir_multiplier * (step_v * 0.1);

    let mut top_a = [base_a[0] + offset_u, base_a[1] + offset_v];
    let mut top_b = [base_b[0] + offset_u, base_b[1] + offset_v];

    // Safety Padding Clamping for all 4 vertices to form a neat rectangle with margins on 4 sides
    let pad_u = (step_u * 0.05).min((top_uv_bounds.max.x - top_uv_bounds.min.x).abs() * 0.1);
    let pad_v = (step_v * 0.05).min((top_uv_bounds.max.y - top_uv_bounds.min.y).abs() * 0.1);

    let min_safe_u = top_uv_bounds.min.x + pad_u;
    let max_safe_u = top_uv_bounds.max.x - pad_u;
    let min_safe_v = top_uv_bounds.min.y + pad_v;
    let max_safe_v = top_uv_bounds.max.y - pad_v;

    if max_safe_u >= min_safe_u {
        base_a[0] = base_a[0].clamp(min_safe_u, max_safe_u);
        base_b[0] = base_b[0].clamp(min_safe_u, max_safe_u);
        top_a[0] = top_a[0].clamp(min_safe_u, max_safe_u);
        top_b[0] = top_b[0].clamp(min_safe_u, max_safe_u);
    }
    if max_safe_v >= min_safe_v {
        base_a[1] = base_a[1].clamp(min_safe_v, max_safe_v);
        base_b[1] = base_b[1].clamp(min_safe_v, max_safe_v);
        top_a[1] = top_a[1].clamp(min_safe_v, max_safe_v);
        top_b[1] = top_b[1].clamp(min_safe_v, max_safe_v);
    }

    [base_a, base_b, top_b, top_a]
}

/// Computes reconstructed 4 UV corner coordinates for an extruded side quad.
///
/// Order of returned UVs matches `[uv_base_a, uv_base_b, uv_top_b, uv_top_a]`.
#[inline]
pub fn repair_extruded_side_uv(
    uv_base_a: [f32; 2],
    uv_base_b: [f32; 2],
    top_normal: [f32; 3],
    extrude_vec: [f32; 3],
    mode: ExtrudeUvMode,
    step_u: f32,
    step_v: f32,
    top_uv_bounds: Aabb2d,
    adjacent_uv_strip: Option<[[f32; 2]; 4]>,
) -> [[f32; 2]; 4] {
    repair_extruded_side_uv_advanced(
        uv_base_a,
        uv_base_b,
        top_normal,
        extrude_vec,
        mode,
        step_u,
        step_v,
        top_uv_bounds,
        None,
        adjacent_uv_strip,
    )
}
