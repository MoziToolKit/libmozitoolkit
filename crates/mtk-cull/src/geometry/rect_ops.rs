use alloc::vec::Vec;
use mtk_core::constants::geometry::EPS;
use mtk_core::Aabb2d;
use mtk_core::Direction;
use mtk_core::Vec3;

use crate::types::{is_empty_rect, is_full_rect};

/// Subtracts `occluder` rectangle from `source` rectangle.
/// Returns a list of disjoint rectangles representing `source \ occluder`.
///
/// If `occluder` does not intersect `source`, returns `vec![source]`.
/// If `occluder` completely covers `source`, returns `vec![]`.
pub(crate) fn subtract_rect(source: &Aabb2d, occluder: &Aabb2d) -> Vec<Aabb2d> {
    let mut out = [Aabb2d::ZERO; 4];
    let n = subtract_rect_into(source, occluder, &mut out);
    out[..n].to_vec()
}

/// Zero-allocation rectangle subtraction writing up to 4 disjoint fragments directly into stack buffer.
/// Returns the number of resulting fragments (0..=4).
#[inline]
pub(crate) fn subtract_rect_into(
    source: &Aabb2d,
    occluder: &Aabb2d,
    out: &mut [Aabb2d; 4],
) -> usize {
    if !source.intersects(occluder) {
        out[0] = *source;
        return 1;
    }

    // If occluder completely covers source, nothing remains
    if occluder.contains_rect(source, EPS) {
        return 0;
    }

    let mut count = 0;

    // Bounding bounds of the intersection
    let inter_min_x = source.min.x.max(occluder.min.x);
    let inter_max_x = source.max.x.min(occluder.max.x);
    let inter_min_y = source.min.y.max(occluder.min.y);
    let inter_max_y = source.max.y.min(occluder.max.y);

    // 1. Bottom slice: [source.min.x .. source.max.x] x [source.min.y .. inter_min_y]
    if inter_min_y - source.min.y > EPS {
        out[count] = Aabb2d::from_min_max(source.min.x, source.min.y, source.max.x, inter_min_y);
        count += 1;
    }

    // 2. Top slice: [source.min.x .. source.max.x] x [inter_max_y .. source.max.y]
    if source.max.y - inter_max_y > EPS {
        out[count] = Aabb2d::from_min_max(source.min.x, inter_max_y, source.max.x, source.max.y);
        count += 1;
    }

    // 3. Left slice: [source.min.x .. inter_min_x] x [inter_min_y .. inter_max_y]
    if inter_min_x - source.min.x > EPS {
        out[count] = Aabb2d::from_min_max(source.min.x, inter_min_y, inter_min_x, inter_max_y);
        count += 1;
    }

    // 4. Right slice: [inter_max_x .. source.max.x] x [inter_min_y .. inter_max_y]
    if source.max.x - inter_max_x > EPS {
        out[count] = Aabb2d::from_min_max(inter_max_x, inter_min_y, source.max.x, inter_max_y);
        count += 1;
    }

    count
}

/// Checks if all rectangles in `target_rects` are completely covered by `neighbor_occluders`.
///
/// Returns `true` if target is 100% occluded (should be culled), `false` if any part remains visible.
/// Equivalent to Minecraft `Shapes.joinIsNotEmpty(targetShape, occluderShape, BooleanOp.ONLY_FIRST) == false`.
pub(crate) fn is_face_completely_occluded(
    target_rects: &[Aabb2d],
    neighbor_occluders: &[Aabb2d],
) -> bool {
    if target_rects.is_empty() {
        return true;
    }
    if neighbor_occluders.is_empty() {
        return false;
    }

    // Fast path: check if any neighbor occluder is full
    for occ in neighbor_occluders {
        if is_full_rect(occ, EPS) {
            return true;
        }
    }

    let mut pieces = Vec::with_capacity(8);
    let mut next_pieces = Vec::with_capacity(8);
    let mut buf = [Aabb2d::ZERO; 4];

    // Detailed 2D polygon subtraction for each target piece
    for target in target_rects {
        if is_empty_rect(target, EPS) {
            continue;
        }
        pieces.clear();
        pieces.push(*target);

        for occ in neighbor_occluders {
            if is_empty_rect(occ, EPS) {
                continue;
            }
            next_pieces.clear();
            for p in &pieces {
                let n = subtract_rect_into(p, occ, &mut buf);
                next_pieces.extend_from_slice(&buf[..n]);
            }
            core::mem::swap(&mut pieces, &mut next_pieces);
            if pieces.is_empty() {
                break;
            }
        }
        // If any fragment of this target rect remains unoccluded, the face is visible!
        if !pieces.is_empty() {
            return false;
        }
    }

    true
}

/// Extract 2D `Aabb2d` on the outer plane for a single quad face's vertices in [0..1] space.
/// If the quad lies on the boundary plane corresponding to `direction`, returns `Some(Aabb2d)`;
/// otherwise returns `None`.
pub(crate) fn extract_quad_face_occlusion_rect(
    vertices: &[Vec3; 4],
    direction: Direction,
) -> Option<Aabb2d> {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;

    for v in vertices {
        min_x = min_x.min(v.x);
        max_x = max_x.max(v.x);
        min_y = min_y.min(v.y);
        max_y = max_y.max(v.y);
        min_z = min_z.min(v.z);
        max_z = max_z.max(v.z);
    }

    match direction {
        Direction::East => {
            // Outer plane at X=1
            if (max_x - 1.0).abs() <= EPS && (min_x - 1.0).abs() <= EPS {
                let rect = Aabb2d::from_min_max(min_z, min_y, max_z, max_y);
                if !is_empty_rect(&rect, EPS) {
                    return Some(rect);
                }
            }
        }
        Direction::West => {
            // Outer plane at X=0
            if min_x.abs() <= EPS && max_x.abs() <= EPS {
                let rect = Aabb2d::from_min_max(min_z, min_y, max_z, max_y);
                if !is_empty_rect(&rect, EPS) {
                    return Some(rect);
                }
            }
        }
        Direction::Up => {
            // Outer plane at Y=1
            if (max_y - 1.0).abs() <= EPS && (min_y - 1.0).abs() <= EPS {
                let rect = Aabb2d::from_min_max(min_x, min_z, max_x, max_z);
                if !is_empty_rect(&rect, EPS) {
                    return Some(rect);
                }
            }
        }
        Direction::Down => {
            // Outer plane at Y=0
            if min_y.abs() <= EPS && max_y.abs() <= EPS {
                let rect = Aabb2d::from_min_max(min_x, min_z, max_x, max_z);
                if !is_empty_rect(&rect, EPS) {
                    return Some(rect);
                }
            }
        }
        Direction::South => {
            // Outer plane at Z=1
            if (max_z - 1.0).abs() <= EPS && (min_z - 1.0).abs() <= EPS {
                let rect = Aabb2d::from_min_max(min_x, min_y, max_x, max_y);
                if !is_empty_rect(&rect, EPS) {
                    return Some(rect);
                }
            }
        }
        Direction::North => {
            // Outer plane at Z=0
            if min_z.abs() <= EPS && max_z.abs() <= EPS {
                let rect = Aabb2d::from_min_max(min_x, min_y, max_x, max_y);
                if !is_empty_rect(&rect, EPS) {
                    return Some(rect);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subtract_no_intersection() {
        let s = Aabb2d::from_min_max(0.0, 0.0, 0.5, 0.5);
        let o = Aabb2d::from_min_max(0.6, 0.6, 1.0, 1.0);
        let res = subtract_rect(&s, &o);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0], s);
    }

    #[test]
    fn test_subtract_full_cover() {
        let s = Aabb2d::from_min_max(0.2, 0.2, 0.8, 0.8);
        let o = Aabb2d::from_min_max(0.0, 0.0, 1.0, 1.0);
        let res = subtract_rect(&s, &o);
        assert!(res.is_empty());
    }

    #[test]
    fn test_subtract_half_slab() {
        // Bottom slab: [0..1] x [0..0.5] occluded by a bottom slab
        let s = Aabb2d::from_min_max(0.0, 0.0, 1.0, 1.0);
        let o = Aabb2d::from_min_max(0.0, 0.0, 1.0, 0.5);
        let res = subtract_rect(&s, &o);
        assert_eq!(res.len(), 1);
        // Remaining should be top half [0..1] x [0.5..1.0]
        assert!((res[0].min.y - 0.5).abs() < 1e-4);
        assert!((res[0].max.y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_subtract_two_halves_fully_occluded() {
        let s = Aabb2d::from_min_max(0.0, 0.0, 1.0, 1.0);
        let o1 = Aabb2d::from_min_max(0.0, 0.0, 1.0, 0.5);
        let o2 = Aabb2d::from_min_max(0.0, 0.5, 1.0, 1.0);
        assert!(is_face_completely_occluded(&[s], &[o1, o2]));
    }

    #[test]
    fn test_extract_quad_face_occlusion_rect() {
        // Top face of a bottom half slab (Y=0.5 plane) -> Not on Y=1 outer boundary -> None
        let slab_top_inner = [
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.0, 0.5, 1.0),
            Vec3::new(1.0, 0.5, 1.0),
            Vec3::new(1.0, 0.5, 0.0),
        ];
        assert!(extract_quad_face_occlusion_rect(&slab_top_inner, Direction::Up).is_none());

        // Bottom face of a bottom half slab (Y=0.0 plane) -> Full 2D face on boundary
        let slab_bottom_outer = [
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 1.0),
        ];
        let rect = extract_quad_face_occlusion_rect(&slab_bottom_outer, Direction::Down);
        assert!(rect.is_some());
        assert!(is_full_rect(&rect.unwrap(), 1e-4));
    }
}
