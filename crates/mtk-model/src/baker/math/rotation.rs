use glam::Vec3;
use mtk_core::Direction;

use crate::parser::model_json::RotationJson;

/// Applies local element rotation around the element origin.
///
/// Supports both vanilla rotation shapes:
/// - single axis (`{axis, angle}`), including the `rescale` stretch for 22.5/45/67.5 degrees;
/// - three-axis Euler (`{x, y, z}`), composed as `Rz * Ry * Rx` to match vanilla
///   `Matrix4f.rotationZYX(z, y, x)`. Euler elements are not rescaled: vanilla's models that use
///   this form never set `rescale`.
pub(crate) fn rotate_element_point(p: Vec3, rot: &RotationJson) -> Vec3 {
    let origin = Vec3::new(
        rot.origin[0] / 16.0,
        rot.origin[1] / 16.0,
        rot.origin[2] / 16.0,
    );
    let mut v = p - origin;

    if let Some((axis, angle)) = rot.single_axis() {
        let rad = angle.to_radians();
        let (s, c) = rad.sin_cos();
        let axis = axis.to_ascii_lowercase();

        match axis.as_str() {
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
            && ((angle.abs() - 22.5).abs() < 0.1
                || (angle.abs() - 45.0).abs() < 0.1
                || (angle.abs() - 67.5).abs() < 0.1)
        {
            let scale = 1.0 / rad.cos();
            match axis.as_str() {
                "x" => v = Vec3::new(v.x, v.y * scale, v.z * scale),
                "y" => v = Vec3::new(v.x * scale, v.y, v.z * scale),
                "z" => v = Vec3::new(v.x * scale, v.y * scale, v.z),
                _ => {}
            }
        }
    } else if let Some((rx, ry, rz)) = rot.euler() {
        // Vanilla order: rotate about Z, then Y, then X.
        if rz != 0.0 {
            let rad = rz.to_radians();
            let (s, c) = rad.sin_cos();
            v = Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z);
        }
        if ry != 0.0 {
            let rad = ry.to_radians();
            let (s, c) = rad.sin_cos();
            v = Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
        }
        if rx != 0.0 {
            let rad = rx.to_radians();
            let (s, c) = rad.sin_cos();
            v = Vec3::new(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::model_json::RotationValue;

    fn rotation(origin: [f32; 3], value: RotationValue) -> RotationJson {
        RotationJson {
            origin,
            value,
            rescale: None,
        }
    }

    #[test]
    fn euler_rotation_matches_vanilla_rotation_zyx() {
        // 90 degrees about Y maps +X to -Z.
        let euler = rotation(
            [0.0, 0.0, 0.0],
            RotationValue::Euler {
                x: 0.0,
                y: 90.0,
                z: 0.0,
            },
        );
        let rotated = rotate_element_point(Vec3::new(1.0, 0.0, 0.0), &euler);
        assert!(
            (rotated - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5,
            "{rotated:?}"
        );

        // 90 degrees about Z maps +X to +Y.
        let euler_z = rotation(
            [0.0, 0.0, 0.0],
            RotationValue::Euler {
                x: 0.0,
                y: 0.0,
                z: 90.0,
            },
        );
        let rotated_z = rotate_element_point(Vec3::new(1.0, 0.0, 0.0), &euler_z);
        assert!(
            (rotated_z - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5,
            "{rotated_z:?}"
        );
    }

    #[test]
    fn euler_and_single_axis_agree_for_one_axis() {
        let single = rotation(
            [0.0, 0.0, 0.0],
            RotationValue::SingleAxis {
                axis: "y".to_string(),
                angle: -67.5,
            },
        );
        let euler = rotation(
            [0.0, 0.0, 0.0],
            RotationValue::Euler {
                x: 0.0,
                y: -67.5,
                z: 0.0,
            },
        );
        let point = Vec3::new(0.3, 0.7, -0.2);
        let a = rotate_element_point(point, &single);
        let b = rotate_element_point(point, &euler);
        assert!((a - b).length() < 1e-5, "{a:?} vs {b:?}");
    }

    #[test]
    fn euler_rotation_respects_the_origin() {
        let euler = rotation(
            [8.0, 8.0, 8.0],
            RotationValue::Euler {
                x: 0.0,
                y: 180.0,
                z: 0.0,
            },
        );
        // Rotating the pivot itself must be a no-op.
        let pivot = Vec3::splat(0.5);
        assert!((rotate_element_point(pivot, &euler) - pivot).length() < 1e-6);
    }
}
