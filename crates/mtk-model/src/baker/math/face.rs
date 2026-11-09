use glam::{Vec2, Vec3};
use mtk_core::Direction;

use super::rotation::{
    calculate_facing, rotate_element_point, rotate_point, rotate_point_3d_with_pivot,
};
use super::uvlock::{apply_uvlock_to_uvs, recalculate_winding};
use crate::parser::model_json::{BuiltinTransform, RotationJson};

/// Canonical vertex extents for standard cube faces according to Minecraft FaceInfo.
pub(crate) fn get_face_canonical_vertex(
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
pub(crate) fn default_face_uv(dir: Direction, from_pos: [f32; 3], to_pos: [f32; 3]) -> [f32; 4] {
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

/// Result of face baking.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BakedFaceGeometry {
    pub direction: Direction,
    pub detected_rotation: f32,
    pub positions: [Vec3; 4],
    pub uvs: [Vec2; 4],
    pub uv_bounds: [f32; 4],
}

/// Full FaceBakery quad baking pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bake_face_exact(
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
