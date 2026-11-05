use glam::{Mat3, Vec2, Vec3};
use mtk_core::Direction;

use crate::parser::model_json::{BuiltinTransform, RotationJson};

/// Canonical vertex extents for standard cube faces according to Minecraft FaceInfo.
pub fn get_face_canonical_vertex(
    dir: Direction,
    from_pos: [f32; 3],
    to_pos: [f32; 3],
    index: usize,
) -> Vec3 {
    let fx = from_pos[0] / 16.0;
    let fy = from_pos[1] / 16.0;
    let fz = from_pos[2] / 16.0;

    let tx = to_pos[0] / 16.0;
    let ty = to_pos[1] / 16.0;
    let tz = to_pos[2] / 16.0;

    let extents = match dir {
        Direction::Down => [
            Vec3::new(fx, fy, tz),
            Vec3::new(fx, fy, fz),
            Vec3::new(tx, fy, fz),
            Vec3::new(tx, fy, tz),
        ],
        Direction::Up => [
            Vec3::new(fx, ty, fz),
            Vec3::new(fx, ty, tz),
            Vec3::new(tx, ty, tz),
            Vec3::new(tx, ty, fz),
        ],
        Direction::North => [
            Vec3::new(tx, ty, fz),
            Vec3::new(tx, fy, fz),
            Vec3::new(fx, fy, fz),
            Vec3::new(fx, ty, fz),
        ],
        Direction::South => [
            Vec3::new(fx, ty, tz),
            Vec3::new(fx, fy, tz),
            Vec3::new(tx, fy, tz),
            Vec3::new(tx, ty, tz),
        ],
        Direction::West => [
            Vec3::new(fx, ty, fz),
            Vec3::new(fx, fy, fz),
            Vec3::new(fx, fy, tz),
            Vec3::new(fx, ty, tz),
        ],
        Direction::East => [
            Vec3::new(tx, ty, tz),
            Vec3::new(tx, fy, tz),
            Vec3::new(tx, fy, fz),
            Vec3::new(tx, ty, fz),
        ],
    };

    extents[index % 4]
}

/// Calculate default UV (min_u, min_v, max_u, max_v) in [0..16] matching Minecraft FaceBakery.
pub fn default_face_uv(dir: Direction, from_pos: [f32; 3], to_pos: [f32; 3]) -> [f32; 4] {
    let [fx, fy, fz] = from_pos;
    let [tx, ty, tz] = to_pos;

    match dir {
        Direction::Down => [fx, 16.0 - tz, tx, 16.0 - fz],
        Direction::Up => [fx, fz, tx, tz],
        Direction::North => [16.0 - tx, 16.0 - ty, 16.0 - fx, 16.0 - fy],
        Direction::South => [fx, 16.0 - ty, tx, 16.0 - fy],
        Direction::West => [fz, 16.0 - ty, tz, 16.0 - fy],
        Direction::East => [16.0 - tz, 16.0 - ty, 16.0 - fz, 16.0 - fy],
    }
}

/// Computes vertex U in [0..1] texture space for cuboid face.
#[inline]
pub fn cuboid_face_get_u(uvs: [f32; 4], rot_shift: usize, vertex: usize, uv_base: f32) -> f32 {
    let [min_u, _min_v, max_u, _max_v] = uvs;
    let idx = (vertex + rot_shift) % 4;
    let base = if uv_base > 0.0 { uv_base } else { 16.0 };
    if idx != 0 && idx != 1 {
        max_u / base
    } else {
        min_u / base
    }
}

/// Computes vertex V in [0..1] texture space for cuboid face.
#[inline]
pub fn cuboid_face_get_v(uvs: [f32; 4], rot_shift: usize, vertex: usize, uv_base: f32) -> f32 {
    let [_min_u, min_v, _max_u, max_v] = uvs;
    let idx = (vertex + rot_shift) % 4;
    let base = if uv_base > 0.0 { uv_base } else { 16.0 };
    if idx != 0 && idx != 3 {
        max_v / base
    } else {
        min_v / base
    }
}

/// Applies local element rotation (around element origin [0..16], axis, angle, and rescale).
pub fn rotate_element_point(p: Vec3, rot: &RotationJson) -> Vec3 {
    let origin = Vec3::new(
        rot.origin[0] / 16.0,
        rot.origin[1] / 16.0,
        rot.origin[2] / 16.0,
    );
    let mut v = p - origin;
    let rad = rot.angle.to_radians();
    let (s, c) = rad.sin_cos();

    match rot.axis.to_ascii_lowercase().as_str() {
        "x" => {
            v = Vec3::new(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
        }
        "y" => {
            v = Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
        }
        "z" => {
            v = Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z);
        }
        _ => {}
    }

    if rot.rescale.unwrap_or(false)
        && ((rot.angle.abs() - 22.5).abs() < 0.1
            || (rot.angle.abs() - 45.0).abs() < 0.1
            || (rot.angle.abs() - 67.5).abs() < 0.1)
    {
        let scale = 1.0 / rad.cos();
        match rot.axis.to_ascii_lowercase().as_str() {
            "x" => v = Vec3::new(v.x, v.y * scale, v.z * scale),
            "y" => v = Vec3::new(v.x * scale, v.y, v.z * scale),
            "z" => v = Vec3::new(v.x * scale, v.y * scale, v.z),
            _ => {}
        }
    }

    v + origin
}

/// Rotate point by block variant X and Y angles around block center (0.5, 0.5, 0.5).
pub fn rotate_point(p: Vec3, rot_x: f32, rot_y: f32) -> Vec3 {
    let origin = Vec3::splat(0.5);
    let mut v = p - origin;

    // Minecraft convention: rotate around X first, then Y (with negative radians)
    if rot_x != 0.0 {
        let rad = (-rot_x).to_radians();
        let (s, c) = rad.sin_cos();
        v = Vec3::new(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
    }
    if rot_y != 0.0 {
        let rad = (-rot_y).to_radians();
        let (s, c) = rad.sin_cos();
        v = Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
    }

    v + origin
}

/// Rotates a point in 3D around an arbitrary pivot point in [0..16] space using Euler angles (X then Y then Z in degrees).
pub fn rotate_point_3d_with_pivot(
    p: Vec3,
    rot_x: f32,
    rot_y: f32,
    rot_z: f32,
    pivot: [f32; 3],
) -> Vec3 {
    let origin = Vec3::new(pivot[0] / 16.0, pivot[1] / 16.0, pivot[2] / 16.0);
    let mut v = p - origin;

    // Follow MiEx rotate order: X then Y then Z
    if rot_x != 0.0 {
        let rad = rot_x.to_radians();
        let (s, c) = rad.sin_cos();
        let new_y = v.z * s + v.y * c;
        let new_z = v.z * c - v.y * s;
        v.y = new_y;
        v.z = new_z;
    }
    if rot_y != 0.0 {
        let rad = rot_y.to_radians();
        let (s, c) = rad.sin_cos();
        let new_x = v.x * c - v.z * s;
        let new_z = v.x * s + v.z * c;
        v.x = new_x;
        v.z = new_z;
    }
    if rot_z != 0.0 {
        let rad = rot_z.to_radians();
        let (s, c) = rad.sin_cos();
        let new_x = v.x * c - v.y * s;
        let new_y = v.x * s + v.y * c;
        v.x = new_x;
        v.y = new_y;
    }

    v + origin
}

/// Rotate a face direction by model variant angles rot_x and rot_y.
pub fn rotate_direction(dir: Direction, rot_x: f32, rot_y: f32) -> Direction {
    let n = dir.normal();
    let mut v = n;
    if rot_x != 0.0 {
        let rad = (-rot_x).to_radians();
        let (s, c) = rad.sin_cos();
        v = Vec3::new(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
    }
    if rot_y != 0.0 {
        let rad = (-rot_y).to_radians();
        let (s, c) = rad.sin_cos();
        v = Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
    }

    let mut best_dir = dir;
    let mut best_dot = -999.0f32;
    for candidate in Direction::ALL {
        let dot = v.dot(candidate.normal());
        if dot > best_dot {
            best_dot = dot;
            best_dir = candidate;
        }
    }
    best_dir
}

/// Calculate closest Minecraft direction from 4 quad vertex positions.
pub fn calculate_facing(positions: &[Vec3; 4]) -> Direction {
    let v1 = positions[1] - positions[0];
    let v2 = positions[2] - positions[0];
    let normal = v1.cross(v2).normalize_or_zero();

    let mut best_dir = Direction::Up;
    let mut best_dot = -999.0f32;
    for candidate in Direction::ALL {
        let dot = normal.dot(candidate.normal());
        if dot > best_dot {
            best_dot = dot;
            best_dir = candidate;
        }
    }
    best_dir
}

/// Exact BlockMath local-to-global matrix transforms.
fn vanilla_uv_transform_local_to_global(dir: Direction) -> Mat3 {
    let half_pi = core::f32::consts::FRAC_PI_2;
    let pi = core::f32::consts::PI;
    match dir {
        Direction::South => Mat3::IDENTITY,
        Direction::East => Mat3::from_rotation_y(half_pi),
        Direction::West => Mat3::from_rotation_y(-half_pi),
        Direction::North => Mat3::from_rotation_y(pi),
        Direction::Up => Mat3::from_rotation_x(-half_pi),
        Direction::Down => Mat3::from_rotation_x(half_pi),
    }
}

/// Computes inverse transformation for UVLock matching Minecraft BlockMath.
pub fn get_face_uvlock_transform(rot_x: f32, rot_y: f32, original_side: Direction) -> Mat3 {
    let mx = Mat3::from_rotation_x((-rot_x).to_radians());
    let my = Mat3::from_rotation_y((-rot_y).to_radians());
    let model_rot = my * mx;

    let local_to_global = vanilla_uv_transform_local_to_global(original_side);
    let face_action = model_rot * local_to_global;

    let transformed_normal = face_action * Vec3::new(0.0, 0.0, 1.0);
    let mut best_side = Direction::South;
    let mut best_dot = -999.0f32;
    for candidate in Direction::ALL {
        let dot = transformed_normal.dot(candidate.normal());
        if dot > best_dot {
            best_dot = dot;
            best_side = candidate;
        }
    }

    let global_to_local = vanilla_uv_transform_local_to_global(best_side).inverse();
    let face_transform = global_to_local * face_action;
    face_transform.inverse()
}

/// Apply UVLock counter-rotation to UV coordinates.
pub fn apply_uvlock_to_uvs(
    uvs: &[Vec2; 4],
    orig_direction: Direction,
    rot_x: f32,
    rot_y: f32,
) -> [Vec2; 4] {
    if rot_x == 0.0 && rot_y == 0.0 {
        return *uvs;
    }
    let inv_uv_transform = get_face_uvlock_transform(rot_x, rot_y, orig_direction);
    let mut res = [Vec2::ZERO; 4];
    for (i, uv) in uvs.iter().enumerate() {
        let centered = Vec3::new(uv.x - 0.5, uv.y - 0.5, 0.0);
        let transformed = inv_uv_transform * centered;
        res[i] = Vec2::new(transformed.x + 0.5, transformed.y + 0.5);
    }
    res
}

/// Exact implementation of Minecraft FaceBakery.recalculateWinding:
/// Reorders positions and uvs so vertices match FaceInfo canonical order.
pub fn recalculate_winding(positions: &mut [Vec3; 4], uvs: &mut [Vec2; 4], final_dir: Direction) {
    let min_x = positions.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let min_y = positions.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let min_z = positions.iter().map(|p| p.z).fold(f32::INFINITY, f32::min);
    let max_x = positions
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = positions
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_z = positions
        .iter()
        .map(|p| p.z)
        .fold(f32::NEG_INFINITY, f32::max);

    let from_pos = [min_x * 16.0, min_y * 16.0, min_z * 16.0];
    let to_pos = [max_x * 16.0, max_y * 16.0, max_z * 16.0];

    for vertex in 0..4 {
        let target_pos = get_face_canonical_vertex(final_dir, from_pos, to_pos, vertex);
        let mut match_idx = None;
        for (i, p) in positions.iter().enumerate().take(4).skip(vertex) {
            if (p.x - target_pos.x).abs() < 1e-4
                && (p.y - target_pos.y).abs() < 1e-4
                && (p.z - target_pos.z).abs() < 1e-4
            {
                match_idx = Some(i);
                break;
            }
        }
        if let Some(i) = match_idx {
            if i != vertex {
                positions.swap(vertex, i);
                uvs.swap(vertex, i);
            }
        }
    }
}

/// Result of face baking.
#[derive(Debug, Clone, PartialEq)]
pub struct BakedFaceGeometry {
    pub direction: Direction,
    pub detected_rotation: f32,
    pub positions: [Vec3; 4],
    pub uvs: [Vec2; 4],
    pub uv_bounds: [f32; 4],
}

/// Full FaceBakery quad baking pipeline.
#[allow(clippy::too_many_arguments)]
pub fn bake_face_exact(
    orig_dir: Direction,
    from_pos: [f32; 3],
    to_pos: [f32; 3],
    uv_bounds: Option<[f32; 4]>,
    face_rotation_deg: f32,
    rot_x: f32,
    rot_y: f32,
    elem_rotation: Option<&RotationJson>,
    builtin_transform: Option<&BuiltinTransform>,
    uvlock: bool,
    uv_base: f32,
) -> BakedFaceGeometry {
    let uv_bounds = uv_bounds.unwrap_or_else(|| default_face_uv(orig_dir, from_pos, to_pos));
    let rot_shift = ((face_rotation_deg / 90.0).round() as usize) % 4;

    // 1. Raw canonical vertices and UVs
    let mut raw_positions = [Vec3::ZERO; 4];
    let mut raw_uvs = [Vec2::ZERO; 4];
    for i in 0..4 {
        raw_positions[i] = get_face_canonical_vertex(orig_dir, from_pos, to_pos, i);
        raw_uvs[i] = Vec2::new(
            cuboid_face_get_u(uv_bounds, rot_shift, i, uv_base),
            cuboid_face_get_v(uv_bounds, rot_shift, i, uv_base),
        );
    }

    // 2. Transform 3D vertices
    let mut transformed_positions = [Vec3::ZERO; 4];
    for i in 0..4 {
        let mut p = raw_positions[i];
        if let Some(rot) = elem_rotation {
            p = rotate_element_point(p, rot);
        }
        if let Some(bt) = builtin_transform {
            p = rotate_point_3d_with_pivot(p, bt.rotate[0], bt.rotate[1], bt.rotate[2], bt.pivot);
        }
        if rot_x != 0.0 || rot_y != 0.0 {
            p = rotate_point(p, rot_x, rot_y);
        }
        transformed_positions[i] = p;
    }

    // 3. Apply UVLock if active
    let mut transformed_uvs = if uvlock && (rot_x != 0.0 || rot_y != 0.0) {
        apply_uvlock_to_uvs(&raw_uvs, orig_dir, rot_x, rot_y)
    } else {
        raw_uvs
    };

    // 4. Final direction
    let final_dir = calculate_facing(&transformed_positions);

    // 5. Recalculate winding
    if elem_rotation.is_none() && builtin_transform.is_none() {
        recalculate_winding(&mut transformed_positions, &mut transformed_uvs, final_dir);
    }

    // 6. Extract UV bounds and rotation
    let u0 = transformed_uvs[0].x;
    let v0 = transformed_uvs[0].y;
    let u1 = transformed_uvs[1].x;
    let v1 = transformed_uvs[1].y;
    let u2 = transformed_uvs[2].x;
    let v2 = transformed_uvs[2].y;
    let u3 = transformed_uvs[3].x;
    let v3 = transformed_uvs[3].y;

    let min_u = u0.min(u1).min(u2).min(u3);
    let max_u = u0.max(u1).max(u2).max(u3);
    let min_v = v0.min(v1).min(v2).min(v3);
    let max_v = v0.max(v1).max(v2).max(v3);

    let dv_du = (v3 + v2) - (v0 + v1);
    let dv_dv = (v0 + v3) - (v1 + v2);

    let mut dir_x = -dv_du;
    let mut dir_y = -dv_dv;

    let detected_rot = if dir_x.abs() < 1e-4 && dir_y.abs() < 1e-4 {
        let du_du = (u3 + u2) - (u0 + u1);
        let du_dv = (u0 + u3) - (u1 + u2);
        dir_x = du_du;
        dir_y = du_dv;
        let angle_rad = dir_y.atan2(dir_x);
        let deg = -angle_rad.to_degrees();
        ((deg / 90.0).round() * 90.0).rem_euclid(360.0)
    } else {
        let angle_rad = dir_x.atan2(dir_y);
        let deg = angle_rad.to_degrees();
        ((deg / 90.0).round() * 90.0).rem_euclid(360.0)
    };

    BakedFaceGeometry {
        direction: final_dir,
        detected_rotation: detected_rot,
        positions: transformed_positions,
        uvs: transformed_uvs,
        uv_bounds: [min_u, min_v, max_u, max_v],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_cube_face_normals() {
        for dir in Direction::ALL {
            let baked = bake_face_exact(
                dir,
                [0.0, 0.0, 0.0],
                [16.0, 16.0, 16.0],
                None,
                0.0,
                0.0,
                0.0,
                None,
                None,
                false,
                16.0,
            );
            assert_eq!(baked.direction, dir);
        }
    }

    #[test]
    fn test_rotate_direction() {
        // Rotating North face around Y by 90 deg clockwise -> East
        let new_dir = rotate_direction(Direction::North, 0.0, 90.0);
        assert_eq!(new_dir, Direction::East);

        // Rotating Up face around X by 90 deg -> North
        let new_dir_x = rotate_direction(Direction::Up, 90.0, 0.0);
        assert_eq!(new_dir_x, Direction::North);
    }
}
