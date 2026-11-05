//! Axis-aligned quad clipping against solid 3D bounding boxes.
//!
//! Provides geometric clipping of faces embedded inside neighbouring
//! cuboid volumes using 2D boolean difference and bilinear UV interpolation.

use alloc::vec;
use alloc::vec::Vec;
use glam::{Vec2, Vec3};
use mtk_core::constants::geometry::{EPS, STRICT_EPS, TOLERANCE};
use mtk_core::Aabb2d;
use mtk_core::Direction;

use crate::geometry::rect_ops::subtract_rect;

/// Sub-polygon piece result after excluding hidden volume regions.
#[derive(Debug, Clone, PartialEq)]
pub struct ClippedQuadPiece {
    pub vertices: [Vec3; 4],
    pub uvs: [Vec2; 4],
}

/// Splits an axis-aligned quad face around neighbouring solid bounding boxes covering its outside.
///
/// Many Minecraft models (such as stairs and fences) compose shapes from overlapping cuboids.
/// This algorithm discards regions embedded inside neighbouring volumes using 2D boolean difference
/// and bilinearly interpolates the exact sub-UVs for visible remnants.
pub fn clip_face_excluding_hidden_volume(
    vertices: &[Vec3; 4],
    uvs: &[Vec2; 4],
    direction: Direction,
    neighbour_bounds: &[([f32; 3], [f32; 3])],
) -> Vec<ClippedQuadPiece> {
    let mut spans = [0.0f32; 3];
    for (axis, span_slot) in spans.iter_mut().enumerate() {
        let mut min_val = f32::INFINITY;
        let mut max_val = f32::NEG_INFINITY;
        for v in vertices {
            let val = match axis {
                0 => v.x,
                1 => v.y,
                2 => v.z,
                _ => unreachable!(),
            };
            min_val = min_val.min(val);
            max_val = max_val.max(val);
        }
        *span_slot = max_val - min_val;
    }

    let mut fixed_axis = None;
    for (axis, &span) in spans.iter().enumerate() {
        if span <= EPS {
            if fixed_axis.is_some() {
                // More than 1 flat axis: degenerate face
                return vec![ClippedQuadPiece {
                    vertices: *vertices,
                    uvs: *uvs,
                }];
            }
            fixed_axis = Some(axis);
        }
    }

    let fixed = match fixed_axis {
        Some(f) => f,
        None => {
            // Diagonal / locally rotated face: keep original quad
            return vec![ClippedQuadPiece {
                vertices: *vertices,
                uvs: *uvs,
            }];
        }
    };

    let (a_axis, b_axis) = match fixed {
        0 => (1, 2),
        1 => (0, 2),
        2 => (0, 1),
        _ => unreachable!(),
    };

    let get_axis = |v: Vec3, ax: usize| match ax {
        0 => v.x,
        1 => v.y,
        2 => v.z,
        _ => unreachable!(),
    };

    let plane = get_axis(vertices[0], fixed);
    let mut a0 = f32::INFINITY;
    let mut a1 = f32::NEG_INFINITY;
    let mut b0 = f32::INFINITY;
    let mut b1 = f32::NEG_INFINITY;

    for v in vertices {
        let va = get_axis(*v, a_axis);
        let vb = get_axis(*v, b_axis);
        a0 = a0.min(va);
        a1 = a1.max(va);
        b0 = b0.min(vb);
        b1 = b1.max(vb);
    }

    if a1 - a0 <= STRICT_EPS || b1 - b0 <= STRICT_EPS {
        return vec![ClippedQuadPiece {
            vertices: *vertices,
            uvs: *uvs,
        }];
    }

    let norm = direction.normal();
    let norm_fixed = get_axis(norm, fixed);
    let outside = plane + norm_fixed * EPS;

    let mut pieces = vec![Aabb2d::from_min_max(a0, b0, a1, b1)];

    for (mins, maxs) in neighbour_bounds {
        let min_fixed = mins[fixed];
        let max_fixed = maxs[fixed];
        if !(min_fixed + TOLERANCE <= outside && outside <= max_fixed - TOLERANCE) {
            continue;
        }

        let cut = Aabb2d::from_min_max(mins[a_axis], mins[b_axis], maxs[a_axis], maxs[b_axis]);

        let mut next_pieces = Vec::new();
        for p in pieces {
            let remainders = subtract_rect(&p, &cut);
            next_pieces.extend(remainders);
        }
        pieces = next_pieces;
        if pieces.is_empty() {
            return Vec::new();
        }
    }

    // Map source vertices to corner UVs
    let mut corner_uvs = [Vec2::ZERO; 4]; // [0,0], [1,0], [1,1], [0,1]
    let mut found_corners = [false; 4];

    for (&v, &uv) in vertices.iter().zip(uvs.iter()) {
        let va = get_axis(v, a_axis);
        let vb = get_axis(v, b_axis);
        let sa = ((va - a0) / (a1 - a0)).round() as usize;
        let sb = ((vb - b0) / (b1 - b0)).round() as usize;
        let corner_idx = match (sa, sb) {
            (0, 0) => 0,
            (1, 0) => 1,
            (1, 1) => 2,
            (0, 1) => 3,
            _ => continue,
        };
        corner_uvs[corner_idx] = uv;
        found_corners[corner_idx] = true;
    }

    if !found_corners.iter().all(|&f| f) {
        return vec![ClippedQuadPiece {
            vertices: *vertices,
            uvs: *uvs,
        }];
    }

    let lerp_uv = |va: f32, vb: f32| -> Vec2 {
        let ta = (va - a0) / (a1 - a0);
        let tb = (vb - b0) / (b1 - b0);
        let u0 = corner_uvs[0].lerp(corner_uvs[1], ta);
        let u1 = corner_uvs[3].lerp(corner_uvs[2], ta);
        u0.lerp(u1, tb)
    };

    let make_vec3 = |va: f32, vb: f32| -> Vec3 {
        match fixed {
            0 => Vec3::new(plane, va, vb),
            1 => Vec3::new(va, plane, vb),
            2 => Vec3::new(va, vb, plane),
            _ => unreachable!(),
        }
    };

    let mut result = Vec::with_capacity(pieces.len());
    for rect in pieces {
        // Build CCW quad consistent with source vertices
        let ra0 = rect.min.x;
        let ra1 = rect.max.x;
        let rb0 = rect.min.y;
        let rb1 = rect.max.y;

        // Determine orientation order matching source vertices
        let v0 = make_vec3(ra0, rb0);
        let v1 = make_vec3(ra1, rb0);
        let v2 = make_vec3(ra1, rb1);
        let v3 = make_vec3(ra0, rb1);

        let u0 = lerp_uv(ra0, rb0);
        let u1 = lerp_uv(ra1, rb0);
        let u2 = lerp_uv(ra1, rb1);
        let u3 = lerp_uv(ra0, rb1);

        // Check normal consistency
        let test_norm = (v1 - v0).cross(v2 - v0);
        if test_norm.dot(norm) > 0.0 {
            result.push(ClippedQuadPiece {
                vertices: [v0, v1, v2, v3],
                uvs: [u0, u1, u2, u3],
            });
        } else {
            result.push(ClippedQuadPiece {
                vertices: [v0, v3, v2, v1],
                uvs: [u0, u3, u2, u1],
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_covered_face() {
        // Face of a bottom half slab [0..16, 0..8, 0..16] on top face (y=8/16 = 0.5)
        let v0 = Vec3::new(0.0, 0.5, 0.0);
        let v1 = Vec3::new(0.0, 0.5, 1.0);
        let v2 = Vec3::new(1.0, 0.5, 1.0);
        let v3 = Vec3::new(1.0, 0.5, 0.0);
        let vertices = [v0, v1, v2, v3];
        let uvs = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        ];

        // Neighbour upper step covering half [0.0..1.0, 0.5..1.0, 0.5..1.0]
        let bounds = vec![([0.0, 0.5, 0.5], [1.0, 1.0, 1.0])];

        let clipped = clip_face_excluding_hidden_volume(&vertices, &uvs, Direction::Up, &bounds);

        // Should be clipped down to [0.0..1.0] x [0.0..0.5]
        assert_eq!(clipped.len(), 1);
        let p = &clipped[0];
        let max_z = p.vertices.iter().map(|v| v.z).fold(0.0f32, f32::max);
        assert!((max_z - 0.5).abs() < 1e-4);
    }
}
