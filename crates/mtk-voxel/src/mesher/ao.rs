use glam::Vec3;
use mtk_core::Direction;

/// Determines if a baked quad face is flush with the outer boundary of the unit cube in its direction.
///
/// Matching Minecraft Java `BlockModelLighter.faceCubic`:
/// - UP: minY == maxY && (maxY > 1.0 - EPSILON || is_current_block_full_cube)
/// - DOWN: minY == maxY && (minY < EPSILON || is_current_block_full_cube)
/// - NORTH: minZ == maxZ && (minZ < EPSILON || is_current_block_full_cube)
/// - SOUTH: minZ == maxZ && (maxZ > 1.0 - EPSILON || is_current_block_full_cube)
/// - WEST: minX == maxX && (minX < EPSILON || is_current_block_full_cube)
/// - EAST: minX == maxX && (maxX > 1.0 - EPSILON || is_current_block_full_cube)
#[inline]
pub(crate) fn is_face_cubic(
    dir: Direction,
    vertices: &[Vec3; 4],
    is_current_block_full_cube: bool,
) -> bool {
    if is_current_block_full_cube {
        return true;
    }
    const EPSILON: f32 = 1e-4;
    match dir {
        Direction::Down => vertices.iter().all(|v| v.y.abs() < EPSILON),
        Direction::Up => vertices.iter().all(|v| (v.y - 1.0).abs() < EPSILON),
        Direction::North => vertices.iter().all(|v| v.z.abs() < EPSILON),
        Direction::South => vertices.iter().all(|v| (v.z - 1.0).abs() < EPSILON),
        Direction::West => vertices.iter().all(|v| v.x.abs() < EPSILON),
        Direction::East => vertices.iter().all(|v| (v.x - 1.0).abs() < EPSILON),
    }
}

#[inline(always)]
fn add3(a: (i32, i32, i32), b: (i32, i32, i32)) -> (i32, i32, i32) {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

/// Computes 4-corner continuous AO brightness multipliers `[f32; 4]` in `[0.2..=1.0]`
/// matching Minecraft Java `BlockModelLighter` smooth lighting.
///
/// Vertex ordering strictly matches `UNIT_CUBE_CORNERS` and `BakedFace.vertices` [v0, v1, v2, v3].
pub(crate) fn calculate_face_ao_float<F>(
    dir: Direction,
    face_cubic: bool,
    mut is_solid_opaque: F,
) -> [f32; 4]
where
    F: FnMut(i32, i32, i32) -> bool,
{
    let (c0, c1, c2, c3, dir_offset) = match dir {
        Direction::Down => (
            (-1, 0, 0), // WEST
            (1, 0, 0),  // EAST
            (0, 0, -1), // NORTH
            (0, 0, 1),  // SOUTH
            (0, -1, 0),
        ),
        Direction::Up => (
            (1, 0, 0),  // EAST
            (-1, 0, 0), // WEST
            (0, 0, -1), // NORTH
            (0, 0, 1),  // SOUTH
            (0, 1, 0),
        ),
        Direction::North => (
            (0, 1, 0),  // UP
            (0, -1, 0), // DOWN
            (1, 0, 0),  // EAST
            (-1, 0, 0), // WEST
            (0, 0, -1),
        ),
        Direction::South => (
            (-1, 0, 0), // WEST
            (1, 0, 0),  // EAST
            (0, -1, 0), // DOWN
            (0, 1, 0),  // UP
            (0, 0, 1),
        ),
        Direction::West => (
            (0, 1, 0),  // UP
            (0, -1, 0), // DOWN
            (0, 0, -1), // NORTH
            (0, 0, 1),  // SOUTH
            (-1, 0, 0),
        ),
        Direction::East => (
            (0, -1, 0), // DOWN
            (0, 1, 0),  // UP
            (0, 0, -1), // NORTH
            (0, 0, 1),  // SOUTH
            (1, 0, 0),
        ),
    };

    let base = if face_cubic { dir_offset } else { (0, 0, 0) };

    let pos0 = add3(base, c0);
    let pos1 = add3(base, c1);
    let pos2 = add3(base, c2);
    let pos3 = add3(base, c3);

    let shade0 = if is_solid_opaque(pos0.0, pos0.1, pos0.2) {
        0.2
    } else {
        1.0
    };
    let shade1 = if is_solid_opaque(pos1.0, pos1.1, pos1.2) {
        0.2
    } else {
        1.0
    };
    let shade2 = if is_solid_opaque(pos2.0, pos2.1, pos2.2) {
        0.2
    } else {
        1.0
    };
    let shade3 = if is_solid_opaque(pos3.0, pos3.1, pos3.2) {
        0.2
    } else {
        1.0
    };

    let corner_pos0 = add3(pos0, dir_offset);
    let corner_pos1 = add3(pos1, dir_offset);
    let corner_pos2 = add3(pos2, dir_offset);
    let corner_pos3 = add3(pos3, dir_offset);

    let translucent0 = !is_solid_opaque(corner_pos0.0, corner_pos0.1, corner_pos0.2);
    let translucent1 = !is_solid_opaque(corner_pos1.0, corner_pos1.1, corner_pos1.2);
    let translucent2 = !is_solid_opaque(corner_pos2.0, corner_pos2.1, corner_pos2.2);
    let translucent3 = !is_solid_opaque(corner_pos3.0, corner_pos3.1, corner_pos3.2);

    let shade_corner02 = if !translucent2 && !translucent0 {
        shade0
    } else {
        let p = add3(pos0, c2);
        if is_solid_opaque(p.0, p.1, p.2) {
            0.2
        } else {
            1.0
        }
    };

    let shade_corner03 = if !translucent3 && !translucent0 {
        shade0
    } else {
        let p = add3(pos0, c3);
        if is_solid_opaque(p.0, p.1, p.2) {
            0.2
        } else {
            1.0
        }
    };

    let shade_corner12 = if !translucent2 && !translucent1 {
        shade0
    } else {
        let p = add3(pos1, c2);
        if is_solid_opaque(p.0, p.1, p.2) {
            0.2
        } else {
            1.0
        }
    };

    let shade_corner13 = if !translucent3 && !translucent1 {
        shade0
    } else {
        let p = add3(pos1, c3);
        if is_solid_opaque(p.0, p.1, p.2) {
            0.2
        } else {
            1.0
        }
    };

    let shade_center = if face_cubic {
        if is_solid_opaque(base.0, base.1, base.2) {
            0.2
        } else {
            1.0
        }
    } else if is_solid_opaque(0, 0, 0) {
        0.2
    } else {
        1.0
    };

    let l1 = (shade3 + shade0 + shade_corner03 + shade_center) * 0.25;
    let l2 = (shade2 + shade0 + shade_corner02 + shade_center) * 0.25;
    let l3 = (shade2 + shade1 + shade_corner12 + shade_center) * 0.25;
    let l4 = (shade3 + shade1 + shade_corner13 + shade_center) * 0.25;

    match dir {
        Direction::Down => [l1, l2, l3, l4],
        Direction::Up => [l3, l4, l1, l2],
        Direction::North => [l2, l3, l4, l1],
        Direction::South => [l1, l2, l3, l4],
        Direction::West => [l2, l3, l4, l1],
        Direction::East => [l4, l1, l2, l3],
    }
}

/// Convenience wrapper for calculating standard flush face AO.
#[inline]
pub(crate) fn calculate_face_ao<F>(dir: Direction, is_solid_opaque: F) -> [f32; 4]
where
    F: FnMut(i32, i32, i32) -> bool,
{
    calculate_face_ao_float(dir, true, is_solid_opaque)
}

/// Determines whether a quad should flip its diagonal triangulation split (anisotropy flip)
/// to eliminate diagonal shading crease artifacts.
///
/// Returns `true` if triangulating as `(0-1-2, 0-2-3)` is optimal, or `false` if `(1-2-3, 0-1-3)` is optimal.
#[inline]
pub(crate) fn should_flip_quad_diagonal(ao: [f32; 4]) -> bool {
    (ao[0] + ao[2]) > (ao[1] + ao[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anisotropy_flip() {
        assert!(should_flip_quad_diagonal([1.0, 0.2, 1.0, 0.2]));
        assert!(!should_flip_quad_diagonal([0.2, 1.0, 0.2, 1.0]));
    }

    #[test]
    fn test_is_face_cubic() {
        let full_cube_up = [
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];
        assert!(is_face_cubic(Direction::Up, &full_cube_up, false));

        let dirt_path_up = [
            Vec3::new(0.0, 15.0 / 16.0, 0.0),
            Vec3::new(0.0, 15.0 / 16.0, 1.0),
            Vec3::new(1.0, 15.0 / 16.0, 1.0),
            Vec3::new(1.0, 15.0 / 16.0, 0.0),
        ];
        assert!(!is_face_cubic(Direction::Up, &dirt_path_up, false));

        let slab_up = [
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.0, 0.5, 1.0),
            Vec3::new(1.0, 0.5, 1.0),
            Vec3::new(1.0, 0.5, 0.0),
        ];
        assert!(!is_face_cubic(Direction::Up, &slab_up, false));
    }

    #[test]
    fn test_dirt_path_inset_ambient_occlusion() {
        // Dirt path top face: when isolated, all vertices have continuous brightness 1.0
        let ao_isolated = calculate_face_ao_float(Direction::Up, false, |_dx, _dy, _dz| false);
        assert_eq!(ao_isolated, [1.0, 1.0, 1.0, 1.0]);

        // When a solid block is placed on the East side (dx=1, dy=0, dz=0):
        // East (+X) is shaded (0.2), other 3 sides are 1.0.
        // v0 (-X, -Z) -> (1.0 + 1.0 + 1.0 + 1.0) / 4 = 1.0
        // v1 (-X, +Z) -> (1.0 + 1.0 + 1.0 + 1.0) / 4 = 1.0
        // v2 (+X, +Z) -> (1.0 + 0.2 + 1.0 + 1.0) / 4 = 0.8
        // v3 (+X, -Z) -> (1.0 + 0.2 + 1.0 + 1.0) / 4 = 0.8
        let ao_east_neighbor = calculate_face_ao_float(Direction::Up, false, |dx, dy, dz| {
            dx == 1 && dy == 0 && dz == 0
        });
        assert_eq!(ao_east_neighbor, [1.0, 1.0, 0.8, 0.8]);

        // When solid blocks are placed on both East (1, 0, 0) and South (0, 0, 1):
        // v0 (-X, -Z) -> 1.0
        // v1 (-X, +Z) -> 0.8 (South neighbor)
        // v2 (+X, +Z) -> 0.6 (East 0.2 + South 0.2 + corner SE 1.0 + center 1.0 = 2.4 / 4 = 0.6)
        // v3 (+X, -Z) -> 0.8 (East neighbor)
        let ao_east_south = calculate_face_ao_float(Direction::Up, false, |dx, dy, dz| {
            (dx == 1 && dy == 0 && dz == 0) || (dx == 0 && dy == 0 && dz == 1)
        });
        assert_eq!(ao_east_south, [1.0, 0.8, 0.6, 0.8]);
    }
}
