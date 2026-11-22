//! Helper routines for transforming model elements and baking Wavefront OBJ specs into `BakedObjFace`s.

use std::collections::HashMap;

use glam::{Mat3, Mat4, Vec2, Vec3};
use mtk_core::Direction;

use super::ModelBaker;
use crate::baker::math::rotate_point;
use crate::parser::loader_ext::ResolvedObjSpec;
use crate::parser::model_json::BlockModelJson;
use crate::parser::obj::{BakedObjFace, ObjAssetStore};

#[inline]
pub(super) fn nearest_direction(normal: Vec3) -> Direction {
    let mut best_dir = Direction::Up;
    let mut best_dot = f32::NEG_INFINITY;
    for dir in Direction::ALL {
        let d = normal.dot(dir.normal());
        if d > best_dot {
            best_dot = d;
            best_dir = dir;
        }
    }
    best_dir
}

#[inline]
pub(super) fn rotate_vector_blockstate(mut v: Vec3, rot_x: f32, rot_y: f32) -> Vec3 {
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
    v
}

#[inline]
fn resolve_fallback_obj_texture(textures: &HashMap<String, String>) -> String {
    for preferred in ["0", "texture", "all"] {
        if let Some(t) = textures.get(preferred) {
            if !t.is_empty() && !t.starts_with('#') {
                return t.clone();
            }
        }
    }
    for (k, v) in textures {
        if k != "particle" && !v.is_empty() && !v.starts_with('#') {
            return v.clone();
        }
    }
    if let Some(p) = textures.get("particle") {
        if !p.is_empty() && !p.starts_with('#') {
            return p.clone();
        }
    }
    String::new()
}

impl ModelBaker {
    /// Bakes all `ResolvedObjSpec` entries from a resolved model into `BakedObjFace`s,
    /// applying root transforms, blockstate variant rotations, visibility filters, and `.mtl` bindings.
    pub(super) fn bake_obj_specs(
        &self,
        obj_specs: &[ResolvedObjSpec],
        rot_x: f32,
        rot_y: f32,
        out: &mut Vec<BakedObjFace>,
    ) {
        if obj_specs.is_empty() {
            return;
        }
        let Some(ref store) = self.obj_assets else {
            return;
        };

        for spec in obj_specs {
            let Some((obj_asset_path, parsed_obj)) =
                store.get_obj(&spec.model_path, &spec.fallback_namespace)
            else {
                continue;
            };

            let mat = spec.root_transform.map(|cols| Mat4::from_cols_array(&cols));
            let neg_det = mat.is_some_and(|m| m.determinant() < 0.0);
            let normal_mat = mat.map(|m| {
                let linear = Mat3::from_mat4(m);
                if linear.determinant().abs() > 1e-8 {
                    linear.inverse().transpose()
                } else {
                    linear
                }
            });
            let fallback_tex = resolve_fallback_obj_texture(&spec.textures);

            for raw_face in &parsed_obj.faces {
                if !spec.visibility.is_empty() {
                    if raw_face
                        .object_name
                        .as_deref()
                        .and_then(|o| spec.visibility.get(o))
                        == Some(&false)
                    {
                        continue;
                    }
                    if raw_face
                        .group_names
                        .iter()
                        .any(|g| spec.visibility.get(g) == Some(&false))
                    {
                        continue;
                    }
                }

                let mtl_mat = raw_face.material.as_deref().and_then(|mat_name| {
                    store.find_material(
                        obj_asset_path,
                        spec.mtl_override.as_deref(),
                        &parsed_obj.mtllibs,
                        mat_name,
                    )
                });
                let tint_index = mtl_mat.map(|m| m.tint_index).unwrap_or(-1);
                let texture = resolve_face_texture(
                    mtl_mat.and_then(|m| m.map_kd.as_deref()),
                    &spec.textures,
                    &fallback_tex,
                );

                let mut vertices: Vec<Vec3> = raw_face
                    .vertices
                    .iter()
                    .map(|&v| {
                        let p = mat.map_or(v, |m| m.transform_point3(v));
                        if rot_x != 0.0 || rot_y != 0.0 {
                            rotate_point(p, rot_x, rot_y)
                        } else {
                            p
                        }
                    })
                    .collect();

                let mut uvs: Vec<Vec2> = raw_face
                    .uvs
                    .iter()
                    .map(|&uv| Vec2::new(uv.x, if spec.flip_v { 1.0 - uv.y } else { uv.y }))
                    .collect();

                if neg_det {
                    vertices.reverse();
                    uvs.reverse();
                }

                let geom_normal = if vertices.len() >= 3 {
                    (vertices[1] - vertices[0])
                        .cross(vertices[2] - vertices[0])
                        .normalize_or_zero()
                } else {
                    Vec3::Y
                };

                let mut vn_sum = Vec3::ZERO;
                let mut vn_count = 0usize;
                for n in raw_face.normals.iter().flatten() {
                    vn_sum += *n;
                    vn_count += 1;
                }

                let normal = if vn_count > 0 && vn_sum.length_squared() > 1e-8 {
                    let transformed_n = normal_mat.map_or(vn_sum, |nm| nm * vn_sum);
                    let rotated_n = if rot_x != 0.0 || rot_y != 0.0 {
                        rotate_vector_blockstate(transformed_n, rot_x, rot_y)
                    } else {
                        transformed_n
                    };
                    let normalized = rotated_n.normalize_or_zero();
                    if normalized.length_squared() > 1e-8 && normalized.dot(geom_normal) >= -0.25 {
                        normalized
                    } else {
                        geom_normal
                    }
                } else {
                    geom_normal
                };

                let direction = nearest_direction(normal);
                out.push(BakedObjFace {
                    direction,
                    texture,
                    tint_index,
                    vertices,
                    uvs,
                    normal,
                    atlas_uvs: None,
                    atlas_chunk_id: None,
                    atlas_texture_id: None,
                });
            }
        }
    }
}

fn resolve_face_texture(
    map_kd: Option<&str>,
    resolved_textures: &HashMap<String, String>,
    fallback_tex: &str,
) -> String {
    if let Some(kd) = map_kd {
        if let Some(var_name) = kd.strip_prefix('#') {
            if let Some(tex) = resolved_textures.get(var_name) {
                if !tex.is_empty() && !tex.starts_with('#') {
                    return tex.clone();
                }
            }
        } else if ObjAssetStore::is_valid_mc_texture_ref(kd) {
            return BlockModelJson::normalize_texture(kd);
        }
    }
    fallback_tex.to_string()
}
