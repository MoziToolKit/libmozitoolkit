//! # UV Grid & Bilinear Parameter Mathematics
//!
//! Provides UV parameter resolution, integer grid cut factor calculation,
//! and 2D/3D/4D bilinear interpolations and inverses.

use alloc::vec;
use alloc::vec::Vec;
use core::cmp::max;

use crate::geometry::Aabb2d;

/// Calculates target (cols, rows) subdivisions for a quad face based on texture resolution and UV span.
///
/// Computes exact UV edge vector lengths in texture pixel space, ensuring 1:1 physical pixel alignment
/// for Atlas sub-regions, non-square tiles, rotated UVs, and long vertical animated strips.
pub fn calculate_face_target_grid(
    uvs: &[[f32; 2]],
    tex_w: u32,
    tex_h: u32,
    pixels_per_face: f32,
    max_subdivisions: u32,
) -> (u32, u32) {
    if uvs.is_empty() || tex_w == 0 || tex_h == 0 {
        return (1, 1);
    }

    let ppf = if pixels_per_face <= 0.0 {
        1.0
    } else {
        pixels_per_face
    };
    let max_sub = max(1, max_subdivisions);

    // If we have 4 quad corner UVs [uv0, uv1, uv2, uv3]:
    let (u_pixels, v_pixels) = if uvs.len() >= 4 {
        let uv0 = uvs[0];
        let uv1 = uvs[1];
        let uv2 = uvs[2];
        let uv3 = uvs[3];

        // Bottom edge (0 -> 1) and top edge (3 -> 2)
        let du0 = uv1[0] - uv0[0];
        let dv0 = uv1[1] - uv0[1];
        let du1 = uv2[0] - uv3[0];
        let dv1 = uv2[1] - uv3[1];

        // Left edge (0 -> 3) and right edge (1 -> 2)
        let du2 = uv3[0] - uv0[0];
        let dv2 = uv3[1] - uv0[1];
        let du3 = uv2[0] - uv1[0];
        let dv3 = uv2[1] - uv1[1];

        let px_u0 = ((du0 * tex_w as f32).powi(2) + (dv0 * tex_h as f32).powi(2)).sqrt();
        let px_u1 = ((du1 * tex_w as f32).powi(2) + (dv1 * tex_h as f32).powi(2)).sqrt();
        let px_v0 = ((du2 * tex_w as f32).powi(2) + (dv2 * tex_h as f32).powi(2)).sqrt();
        let px_v1 = ((du3 * tex_w as f32).powi(2) + (dv3 * tex_h as f32).powi(2)).sqrt();

        (0.5 * (px_u0 + px_u1), 0.5 * (px_v0 + px_v1))
    } else {
        // Fallback for non-quad: compute bounding box span
        let aabb = Aabb2d::from_points(uvs);
        let u_span = (aabb.max.x - aabb.min.x).abs();
        let v_span = (aabb.max.y - aabb.min.y).abs();
        (u_span * tex_w as f32, v_span * tex_h as f32)
    };

    // Anti-explosion defense for long vertical animated strips (e.g. 16x512)
    let effective_v_pixels = if is_vertical_animation_strip(tex_w, tex_h, v_pixels) {
        tex_w as f32
    } else {
        v_pixels
    };

    let cols = ((u_pixels / ppf).round() as u32).clamp(1, max_sub);
    let rows = ((effective_v_pixels / ppf).round() as u32).clamp(1, max_sub);

    (cols, rows)
}

/// Detects faces that sample a single frame of a tall vertical animation strip.
///
/// Animated textures store their frames vertically (16x512 = 32 frames of 16x16).
/// A face spanning the whole texture height would otherwise be cut into hundreds
/// of horizontal strips. Animation metadata is not available at this layer, so a
/// V span wider than 1.5 frames is treated as "samples one frame" and clamped.
///
/// This is the single policy shared by target-grid sizing, cut-factor generation
/// and polygon clipping; do not duplicate the condition.
#[inline]
pub(crate) fn is_vertical_animation_strip(tex_w: u32, tex_h: u32, v_pixels: f32) -> bool {
    tex_h > tex_w && tex_h.is_multiple_of(tex_w) && v_pixels > tex_w as f32 * 1.5
}

/// Calculates non-uniform [0, 1] parameter cut factors along U and V axes of a quad face,
/// directly snapping interior cuts to the integer pixel grid lines of the texture image.
///
/// Returns `(u_factors: Vec<f32>, v_factors: Vec<f32>)` where each factor list starts with 0.0 and ends with 1.0.
pub fn calculate_pixel_grid_cut_factors(
    uvs: &[[f32; 2]],
    tex_w: u32,
    tex_h: u32,
    pixels_per_face: f32,
    max_subdivisions: u32,
) -> (Vec<f32>, Vec<f32>) {
    if uvs.len() < 4 || tex_w == 0 || tex_h == 0 {
        return (vec![0.0, 1.0], vec![0.0, 1.0]);
    }

    let step = if pixels_per_face <= 0.0 {
        1.0
    } else {
        pixels_per_face
    };
    let max_sub = max_subdivisions.max(1) as usize;

    // Convert UVs to pixel space
    let p0 = [uvs[0][0] * tex_w as f32, uvs[0][1] * tex_h as f32];
    let p1 = [uvs[1][0] * tex_w as f32, uvs[1][1] * tex_h as f32];
    let p2 = [uvs[2][0] * tex_w as f32, uvs[2][1] * tex_h as f32];
    let p3 = [uvs[3][0] * tex_w as f32, uvs[3][1] * tex_h as f32];

    // Helper to find integer grid line cuts along an edge
    fn get_edge_cuts(pa: [f32; 2], pb: [f32; 2], step: f32, max_cuts: usize) -> Vec<f32> {
        let dx = pb[0] - pa[0];
        let dy = pb[1] - pa[1];

        // Determine dominant pixel axis
        let (start, end) = if dx.abs() >= dy.abs() {
            (pa[0], pb[0])
        } else {
            (pa[1], pb[1])
        };

        let span = end - start;
        if span.abs() < 1e-4 {
            return vec![0.0, 1.0];
        }

        let mut cuts = Vec::new();
        cuts.push(0.0);

        if span > 0.0 {
            // Increasing direction
            let mut k = ((start / step) + 1e-4).ceil() * step;
            while k < end - 1e-4 {
                let factor = (k - start) / span;
                if factor > 1e-4 && factor < (1.0 - 1e-4) {
                    cuts.push(factor);
                }
                k += step;
                if cuts.len() >= max_cuts {
                    break;
                }
            }
        } else {
            // Decreasing direction
            let mut k = ((start / step) - 1e-4).floor() * step;
            while k > end + 1e-4 {
                let factor = (k - start) / span;
                if factor > 1e-4 && factor < (1.0 - 1e-4) {
                    cuts.push(factor);
                }
                k -= step;
                if cuts.len() >= max_cuts {
                    break;
                }
            }
        }

        cuts.push(1.0);
        cuts
    }

    // Compute cuts along U (bottom edge p0->p1 and top edge p3->p2)
    let u_cuts_bot = get_edge_cuts(p0, p1, step, max_sub);
    let u_cuts_top = get_edge_cuts(p3, p2, step, max_sub);
    let u_cuts = if u_cuts_bot.len() >= u_cuts_top.len() {
        u_cuts_bot
    } else {
        u_cuts_top
    };

    // Anti-explosion defense for long vertical animated strips
    let v_pixels = (p3[1] - p0[1]).abs().max((p2[1] - p1[1]).abs());
    let max_sub_v = if is_vertical_animation_strip(tex_w, tex_h, v_pixels) {
        max_sub.min(tex_w as usize)
    } else {
        max_sub
    };

    // Compute cuts along V (left edge p0->p3 and right edge p1->p2)
    let v_cuts_left = get_edge_cuts(p0, p3, step, max_sub_v);
    let v_cuts_right = get_edge_cuts(p1, p2, step, max_sub_v);
    let v_cuts = if v_cuts_left.len() >= v_cuts_right.len() {
        v_cuts_left
    } else {
        v_cuts_right
    };

    (u_cuts, v_cuts)
}

/// Bilinear interpolation helper for scalar / vector types.
#[inline]
pub(crate) fn interpolate_bilinear_2d(
    c0: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    c3: [f32; 2],
    u: f32,
    v: f32,
) -> [f32; 2] {
    let u_inv = 1.0 - u;
    let v_inv = 1.0 - v;
    [
        u_inv * v_inv * c0[0] + u * v_inv * c1[0] + u * v * c2[0] + u_inv * v * c3[0],
        u_inv * v_inv * c0[1] + u * v_inv * c1[1] + u * v * c2[1] + u_inv * v * c3[1],
    ]
}

#[inline]
pub(crate) fn interpolate_bilinear_3d(
    c0: [f32; 3],
    c1: [f32; 3],
    c2: [f32; 3],
    c3: [f32; 3],
    u: f32,
    v: f32,
) -> [f32; 3] {
    let u_inv = 1.0 - u;
    let v_inv = 1.0 - v;
    [
        u_inv * v_inv * c0[0] + u * v_inv * c1[0] + u * v * c2[0] + u_inv * v * c3[0],
        u_inv * v_inv * c0[1] + u * v_inv * c1[1] + u * v * c2[1] + u_inv * v * c3[1],
        u_inv * v_inv * c0[2] + u * v_inv * c1[2] + u * v * c2[2] + u_inv * v * c3[2],
    ]
}

#[inline]
pub(crate) fn interpolate_bilinear_4d(
    c0: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
    u: f32,
    v: f32,
) -> [f32; 4] {
    let u_inv = 1.0 - u;
    let v_inv = 1.0 - v;
    [
        u_inv * v_inv * c0[0] + u * v_inv * c1[0] + u * v * c2[0] + u_inv * v * c3[0],
        u_inv * v_inv * c0[1] + u * v_inv * c1[1] + u * v * c2[1] + u_inv * v * c3[1],
        u_inv * v_inv * c0[2] + u * v_inv * c1[2] + u * v * c2[2] + u_inv * v * c3[2],
        u_inv * v_inv * c0[3] + u * v_inv * c1[3] + u * v * c2[3] + u_inv * v * c3[3],
    ]
}

/// Inverts a 2D bilinear map from UV space to parametric coordinates (s, t) in [0, 1]^2.
pub fn invert_quad_bilinear(
    uv0: [f32; 2],
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    u: f32,
    v: f32,
) -> [f32; 2] {
    let a = uv0;
    let b = [uv1[0] - uv0[0], uv1[1] - uv0[1]];
    let c = [uv3[0] - uv0[0], uv3[1] - uv0[1]];
    let d = [
        uv0[0] - uv1[0] + uv2[0] - uv3[0],
        uv0[1] - uv1[1] + uv2[1] - uv3[1],
    ];

    let p = [u - a[0], v - a[1]];

    // If d is near zero, the quad is a parallelogram (rectangle, diamond, or sheared parallelogram)
    if d[0].abs() < 1e-5 && d[1].abs() < 1e-5 {
        let det = b[0] * c[1] - b[1] * c[0];
        if det.abs() > 1e-6 {
            let s = (p[0] * c[1] - p[1] * c[0]) / det;
            let t = (b[0] * p[1] - b[1] * p[0]) / det;
            return [s.clamp(0.0, 1.0), t.clamp(0.0, 1.0)];
        }
    }

    // General quadratic equation in t:
    let a_quad = c[0] * d[1] - c[1] * d[0];
    let b_quad = c[0] * b[1] - c[1] * b[0] + p[0] * d[1] - p[1] * d[0];
    let c_quad = p[0] * b[1] - p[1] * b[0];

    let t = if a_quad.abs() < 1e-6 {
        if b_quad.abs() > 1e-6 {
            -c_quad / b_quad
        } else {
            0.5
        }
    } else {
        let disc = b_quad * b_quad - 4.0 * a_quad * c_quad;
        if disc >= 0.0 {
            let sqrt_disc = disc.sqrt();
            let t1 = (-b_quad + sqrt_disc) / (2.0 * a_quad);
            let t2 = (-b_quad - sqrt_disc) / (2.0 * a_quad);
            if (0.0..=1.0).contains(&t1) {
                t1
            } else if (0.0..=1.0).contains(&t2) {
                t2
            } else if (t1 - 0.5).abs() < (t2 - 0.5).abs() {
                t1
            } else {
                t2
            }
        } else {
            -b_quad / (2.0 * a_quad)
        }
    };
    let t_clamped = t.clamp(0.0, 1.0);

    let denom_x = b[0] + t_clamped * d[0];
    let denom_y = b[1] + t_clamped * d[1];
    let num_x = p[0] - t_clamped * c[0];
    let num_y = p[1] - t_clamped * c[1];

    let s = if denom_x.abs() >= denom_y.abs() && denom_x.abs() > 1e-6 {
        num_x / denom_x
    } else if denom_y.abs() > 1e-6 {
        num_y / denom_y
    } else {
        0.5
    };

    [s.clamp(0.0, 1.0), t_clamped]
}
