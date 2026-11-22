//! Extensions for Forge / NeoForge declarative model loaders (`*:obj`, `*:composite`, `ie_obj`).

use std::collections::HashMap;

use glam::{EulerRot, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Returns `true` when `loader` names a declarative Wavefront OBJ model loader.
#[inline]
pub(crate) fn is_obj_loader(loader: &str) -> bool {
    matches!(
        loader,
        "forge:obj" | "neoforge:obj" | "porting_lib:obj" | "immersiveengineering:ie_obj"
    )
}

/// Returns `true` when `loader` names a declarative composite model loader.
#[inline]
pub(crate) fn is_composite_loader(loader: &str) -> bool {
    matches!(
        loader,
        "forge:composite" | "neoforge:composite" | "porting_lib:composite"
    )
}

/// Origin anchor specification for NeoForge / Forge `TransformationHelper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransformOriginJson {
    Named(String),
    Coords([f32; 3]),
}

impl TransformOriginJson {
    fn to_vec3(&self) -> Vec3 {
        match self {
            Self::Named(s) => match s.to_ascii_lowercase().as_str() {
                "center" => Vec3::splat(0.5),
                "opposing-corner" => Vec3::ONE,
                _ => Vec3::ZERO,
            },
            Self::Coords(c) => Vec3::from_array(*c),
        }
    }
}

/// Rotation specification accepted by NeoForge / Forge `TransformationHelper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransformRotationJson {
    Quat([f32; 4]),
    Euler([f32; 3]),
    EulerObject {
        #[serde(default)]
        x: f32,
        #[serde(default)]
        y: f32,
        #[serde(default)]
        z: f32,
    },
}

impl TransformRotationJson {
    fn to_quat(&self) -> Quat {
        match self {
            Self::Quat([x, y, z, w]) => {
                let q = Quat::from_xyzw(*x, *y, *z, *w);
                if q.length_squared() > 1e-12 {
                    q.normalize()
                } else {
                    Quat::IDENTITY
                }
            }
            Self::Euler([rx, ry, rz]) => Quat::from_euler(
                EulerRot::XYZ,
                rx.to_radians(),
                ry.to_radians(),
                rz.to_radians(),
            ),
            Self::EulerObject { x, y, z } => Quat::from_euler(
                EulerRot::XYZ,
                x.to_radians(),
                y.to_radians(),
                z.to_radians(),
            ),
        }
    }
}

/// Scale specification accepted by NeoForge / Forge `TransformationHelper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransformScaleJson {
    Vec([f32; 3]),
    Uniform(f32),
}

impl TransformScaleJson {
    fn to_vec3(&self) -> Vec3 {
        match self {
            Self::Vec(v) => Vec3::from_array(*v),
            Self::Uniform(s) => Vec3::splat(*s),
        }
    }
}

/// Object-form NeoForge / Forge root transformation block (`transform` / `root_transforms`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelTransformObjectJson {
    #[serde(default)]
    pub origin: Option<TransformOriginJson>,
    #[serde(default)]
    pub translation: Option<[f32; 3]>,
    #[serde(default, alias = "rotation")]
    pub left_rotation: Option<TransformRotationJson>,
    #[serde(default)]
    pub scale: Option<TransformScaleJson>,
    #[serde(default)]
    pub right_rotation: Option<TransformRotationJson>,
}

/// Model-level transform block in Forge/NeoForge JSON models (object, 4x4 matrix, or flat matrix).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelRootTransformJson {
    Identity(String),
    Matrix4x4([[f32; 4]; 4]),
    Matrix16([f32; 16]),
    Object(ModelTransformObjectJson),
}

impl ModelRootTransformJson {
    /// Converts this JSON transform into a column-major `Mat4` in `[0, 1]` block space.
    pub fn to_mat4(&self) -> Mat4 {
        match self {
            Self::Identity(_) => Mat4::IDENTITY,
            Self::Matrix4x4(rows) => Mat4::from_cols_array_2d(rows).transpose(),
            Self::Matrix16(row_major) => Mat4::from_cols_array(row_major).transpose(),
            Self::Object(obj) => {
                let origin = obj
                    .origin
                    .as_ref()
                    .map(TransformOriginJson::to_vec3)
                    .unwrap_or(Vec3::ZERO);
                let translation = obj.translation.map(Vec3::from_array).unwrap_or(Vec3::ZERO);
                let left_rot = obj
                    .left_rotation
                    .as_ref()
                    .map(TransformRotationJson::to_quat)
                    .unwrap_or(Quat::IDENTITY);
                let scale = obj
                    .scale
                    .as_ref()
                    .map(TransformScaleJson::to_vec3)
                    .unwrap_or(Vec3::ONE);
                let right_rot = obj
                    .right_rotation
                    .as_ref()
                    .map(TransformRotationJson::to_quat)
                    .unwrap_or(Quat::IDENTITY);

                Mat4::from_translation(origin)
                    * Mat4::from_translation(translation)
                    * Mat4::from_quat(left_rot)
                    * Mat4::from_scale(scale)
                    * Mat4::from_quat(right_rot)
                    * Mat4::from_translation(-origin)
            }
        }
    }
}

/// Composes an optional outer transform with an optional inner transform (`outer * inner`),
/// returning `None` when the combined transform is effectively identity.
pub(crate) fn compose_optional_mat4(
    outer: Option<[f32; 16]>,
    inner: Option<[f32; 16]>,
) -> Option<[f32; 16]> {
    let mat = match (outer, inner) {
        (None, None) => return None,
        (Some(a), None) => Mat4::from_cols_array(&a),
        (None, Some(b)) => Mat4::from_cols_array(&b),
        (Some(a), Some(b)) => Mat4::from_cols_array(&a) * Mat4::from_cols_array(&b),
    };
    if mat.abs_diff_eq(Mat4::IDENTITY, 1e-6) {
        None
    } else {
        Some(mat.to_cols_array())
    }
}

/// Resolved Wavefront OBJ model specification inside a `ResolvedBlockModel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedObjSpec {
    pub model_path: String,
    pub fallback_namespace: String,
    pub mtl_override: Option<String>,
    pub flip_v: bool,
    pub visibility: HashMap<String, bool>,
    pub textures: HashMap<String, String>,
    pub root_transform: Option<[f32; 16]>,
}

/// Extracts the model namespace prefix (`"create"` from `"create:block/wheel"`), defaulting to `"minecraft"`.
#[inline]
pub(crate) fn model_namespace(model_id: &str) -> String {
    model_id
        .split_once(':')
        .map(|(ns, _)| ns.to_string())
        .unwrap_or_else(|| "minecraft".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mekanism_center_transform_mat4() {
        let json = r#"{"origin": "center", "rotation": [45, 0, -45]}"#;
        let tf: ModelRootTransformJson = serde_json::from_str(json).unwrap();
        let mat = tf.to_mat4();
        // Center point [0.5, 0.5, 0.5] remains fixed
        let center = mat.transform_point3(Vec3::splat(0.5));
        assert!(center.abs_diff_eq(Vec3::splat(0.5), 1e-5));
    }
}
