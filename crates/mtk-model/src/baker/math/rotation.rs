use glam::Vec3;
use mtk_core::Direction;

use crate::parser::model_json::RotationJson;

/// Applies local element rotation (around element origin [0..16], axis, angle, and rescale).
pub(crate) fn rotate_element_point(p: Vec3, rot: &RotationJson) -> Vec3 {
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
pub(crate) fn rotate_point(p: Vec3, rot_x: f32, rot_y: f32) -> Vec3 {
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
pub(crate) fn rotate_point_3d_with_pivot(
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
pub(crate) fn rotate_direction(dir: Direction, rot_x: f32, rot_y: f32) -> Direction {
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
pub(crate) fn calculate_facing(positions: &[Vec3; 4]) -> Direction {
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
