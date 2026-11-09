use glam::{Mat3, Vec2, Vec3};
use mtk_core::Direction;

use super::face::get_face_canonical_vertex;

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
pub(crate) fn get_face_uvlock_transform(rot_x: f32, rot_y: f32, original_side: Direction) -> Mat3 {
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
pub(crate) fn apply_uvlock_to_uvs(
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
pub(crate) fn recalculate_winding(
    positions: &mut [Vec3; 4],
    uvs: &mut [Vec2; 4],
    final_dir: Direction,
) {
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
