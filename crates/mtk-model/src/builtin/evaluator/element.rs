use super::expr::eval_miex_num;
use super::uv::entity_uv_to_faces;
use crate::parser::model_json::{BuiltinTransform, ElementJson, FaceJson, RotationJson};
use serde_json::Value;
use std::collections::HashMap;

/// Parses a MiEx element JSON node into a concrete `ElementJson`.
pub fn parse_element_node(
    val: &Value,
    vars_num: &HashMap<String, f32>,
    active_transform: Option<&BuiltinTransform>,
) -> Option<ElementJson> {
    let from_val = val.get("from")?.as_array()?;
    let to_val = val.get("to")?.as_array()?;

    let parse_coord = |v: &Value| -> f32 {
        if let Some(n) = v.as_f64() {
            n as f32
        } else if let Some(s) = v.as_str() {
            eval_miex_num(s, vars_num)
        } else {
            0.0
        }
    };

    let from = [
        parse_coord(from_val.first()?),
        parse_coord(from_val.get(1)?),
        parse_coord(from_val.get(2)?),
    ];

    let to = [
        parse_coord(to_val.first()?),
        parse_coord(to_val.get(1)?),
        parse_coord(to_val.get(2)?),
    ];

    let tex_ref = val
        .get("texture")
        .and_then(|v| v.as_str())
        .unwrap_or("#texture")
        .trim_matches('\'');

    let faces = if let Some(entity_uvs) = val.get("entityUVs").and_then(|v| v.as_array()) {
        let u0 = entity_uvs.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let v0 = entity_uvs.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let u1 = entity_uvs.get(2).and_then(|v| v.as_f64()).unwrap_or(16.0) as f32;
        let v1 = entity_uvs.get(3).and_then(|v| v.as_f64()).unwrap_or(16.0) as f32;

        let bounds = [
            from[0].min(to[0]),
            from[1].min(to[1]),
            from[2].min(to[2]),
            from[0].max(to[0]),
            from[1].max(to[1]),
            from[2].max(to[2]),
        ];

        entity_uv_to_faces(bounds, [u0, v0, u1, v1], tex_ref)
    } else if let Some(faces_val) = val.get("faces").and_then(|v| v.as_object()) {
        let mut f_map = HashMap::new();
        for (dir_name, face_obj) in faces_val {
            let uv = face_obj.get("uv").and_then(|v| v.as_array()).map(|uv_arr| {
                [
                    uv_arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    uv_arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    uv_arr.get(2).and_then(|v| v.as_f64()).unwrap_or(16.0) as f32,
                    uv_arr.get(3).and_then(|v| v.as_f64()).unwrap_or(16.0) as f32,
                ]
            });
            let rotation = face_obj
                .get("rotation")
                .and_then(|v| v.as_i64())
                .map(|n| n as u32);

            let texture = face_obj
                .get("texture")
                .and_then(|v| v.as_str())
                .unwrap_or(tex_ref)
                .trim_matches('\'')
                .to_string();

            f_map.insert(
                dir_name.clone(),
                FaceJson {
                    uv,
                    texture,
                    cullface: None,
                    rotation,
                    tintindex: None,
                },
            );
        }
        f_map
    } else {
        HashMap::new()
    };

    let elem_rotation = if let Some(rot_obj) = val.get("rotation") {
        let origin_arr = rot_obj.get("origin").and_then(|v| v.as_array());
        let origin = if let Some(arr) = origin_arr {
            [
                arr.first().and_then(|v| v.as_f64()).unwrap_or(8.0) as f32,
                arr.get(1).and_then(|v| v.as_f64()).unwrap_or(8.0) as f32,
                arr.get(2).and_then(|v| v.as_f64()).unwrap_or(8.0) as f32,
            ]
        } else {
            [8.0, 8.0, 8.0]
        };
        let axis = rot_obj
            .get("axis")
            .and_then(|v| v.as_str())
            .unwrap_or("y")
            .to_string();
        let angle = rot_obj.get("angle").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        Some(RotationJson {
            origin,
            axis,
            angle,
            rescale: None,
        })
    } else {
        None
    };

    Some(ElementJson {
        from,
        to,
        rotation: elem_rotation,
        transform: active_transform.cloned(),
        shade: Some(true),
        faces,
    })
}
