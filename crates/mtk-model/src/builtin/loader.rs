// BSD 3-Clause License
//
// Copyright (c) 2024, Bram Stout Productions
// Upstream: https://github.com/BramStoutProductions/MiEx
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this
//    list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its
//    contributors may be used to endorse or promote products derived from
//    this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::evaluator::{
    entity_uv_to_faces, eval_miex_arithmetic, eval_miex_condition, eval_miex_num, eval_miex_string,
};
use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{
    BlockModelJson, BuiltinTransform, ElementJson, FaceJson, RotationJson, TextureValue,
};
use serde_json::Value;
use std::collections::HashMap;

/// Embedded static raw JSON strings from upstream MiEx `base_resource_pack`.
pub const MIEX_BED_JSON: &str = include_str!("../../assets/builtins/blockstates/bed.json");
pub const MIEX_CHEST_JSON: &str = include_str!("../../assets/builtins/blockstates/chest.json");
pub const MIEX_SHULKER_BOX_JSON: &str =
    include_str!("../../assets/builtins/blockstates/shulker_box.json");
pub const MIEX_SIGN_JSON: &str = include_str!("../../assets/builtins/blockstates/sign.json");
pub const MIEX_HANGING_SIGN_JSON: &str =
    include_str!("../../assets/builtins/blockstates/hanging_sign.json");
pub const MIEX_SKULL_JSON: &str = include_str!("../../assets/builtins/blockstates/skull.json");
pub const MIEX_END_PORTAL_JSON: &str =
    include_str!("../../assets/builtins/blockstates/end_portal.json");

/// Loader that parses upstream MiEx JSON definitions dynamically at runtime.
pub struct MiExModelLoader;

impl MiExModelLoader {
    /// Loads a builtin model for a blockstate using the official upstream MiEx JSON definition.
    pub fn load_for_blockstate(blockstate: &BlockState) -> Option<BlockModelJson> {
        Self::load_for_blockstate_with_entity(blockstate, None)
    }

    /// Loads a builtin model for a blockstate using the official upstream MiEx JSON definition,
    /// enriched with optional dynamic entity properties (e.g. skull rotation or custom profile textures).
    pub fn load_for_blockstate_with_entity(
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
    ) -> Option<BlockModelJson> {
        let name = blockstate.name.as_str();
        let short_name = name.strip_prefix("minecraft:").unwrap_or(name);

        let raw_json = if short_name == "chest" || short_name.ends_with("_chest") {
            MIEX_CHEST_JSON
        } else if short_name == "bed" || short_name.ends_with("_bed") {
            MIEX_BED_JSON
        } else if short_name == "shulker_box" || short_name.ends_with("_shulker_box") {
            MIEX_SHULKER_BOX_JSON
        } else if short_name == "end_portal" {
            MIEX_END_PORTAL_JSON
        } else if short_name.contains("hanging_sign") {
            MIEX_HANGING_SIGN_JSON
        } else if short_name.ends_with("_sign")
            || short_name == "sign"
            || short_name.contains("_wall_sign")
        {
            MIEX_SIGN_JSON
        } else if (short_name.contains("head") || short_name.contains("skull"))
            && !short_name.contains("piston")
        {
            MIEX_SKULL_JSON
        } else {
            return None;
        };

        Self::eval_builtin_json(raw_json, blockstate, entity)
    }

    /// Evaluates a MiEx builtin JSON AST into a concrete `BlockModelJson`.
    fn eval_builtin_json(
        raw_json: &str,
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
    ) -> Option<BlockModelJson> {
        let root: Value = serde_json::from_str(raw_json).ok()?;
        let mut vars_str: HashMap<String, String> = HashMap::new();
        let mut vars_num: HashMap<String, f32> = HashMap::new();

        let mut textures: HashMap<String, TextureValue> = HashMap::new();
        if let Some(default_tex) = root.get("defaultTexture").and_then(|v| v.as_str()) {
            textures.insert(
                "particle".to_string(),
                TextureValue::Path(default_tex.to_string()),
            );
            textures.insert(
                "texture".to_string(),
                TextureValue::Path(default_tex.to_string()),
            );
        }

        // Inject dynamic entity context variables before AST handler execution
        if let Some(crate::entity::BlockEntityData::Skull {
            kind,
            rotation,
            profile,
        }) = entity
        {
            if let Some(rot) = rotation {
                let rot_deg = (22.5 * (*rot as f32) + 180.0) % 360.0;
                vars_num.insert("rotY".to_string(), rot_deg);
                vars_str.insert("rotY".to_string(), rot_deg.to_string());
            } else {
                vars_num.entry("rotY".to_string()).or_insert(0.0);
                vars_str
                    .entry("rotY".to_string())
                    .or_insert("0.0".to_string());
            }

            if let Some(prof) = profile {
                let tex = prof.texture_identifier();
                vars_str.insert("entity_texture".to_string(), tex.clone());
                vars_str.insert("texture".to_string(), tex.clone());
                textures.insert("texture".to_string(), TextureValue::Path(tex.clone()));
                textures.insert("particle".to_string(), TextureValue::Path(tex));
            } else {
                let effective_kind =
                    crate::entity::skull::SkullKind::from_block_name(&blockstate.name)
                        .unwrap_or(*kind);
                let tex = effective_kind.default_texture_path().to_string();
                vars_str.insert("entity_texture".to_string(), tex.clone());
                vars_str.insert("texture".to_string(), tex.clone());
                textures.insert("texture".to_string(), TextureValue::Path(tex.clone()));
                textures.insert("particle".to_string(), TextureValue::Path(tex));
            }
        }

        let mut elements: Vec<ElementJson> = Vec::new();

        if let Some(handler) = root.get("handler") {
            Self::process_handler_node(
                handler,
                blockstate,
                &mut vars_str,
                &mut vars_num,
                &mut textures,
                &mut elements,
                None,
            );
        }

        // Post-handler enforcement: ensure skull textures override AST templates
        if let Some(crate::entity::BlockEntityData::Skull { kind, profile, .. }) = entity {
            let tex = if let Some(prof) = profile {
                prof.texture_identifier()
            } else {
                let effective_kind =
                    crate::entity::skull::SkullKind::from_block_name(&blockstate.name)
                        .unwrap_or(*kind);
                effective_kind.default_texture_path().to_string()
            };
            textures.insert("texture".to_string(), TextureValue::Path(tex.clone()));
            textures.insert("particle".to_string(), TextureValue::Path(tex));
        }

        Some(BlockModelJson {
            parent: None,
            ambientocclusion: Some(true),
            textures: Some(textures),
            elements: Some(elements),
        })
    }

    fn process_handler_node(
        node: &Value,
        blockstate: &BlockState,
        vars_str: &mut HashMap<String, String>,
        vars_num: &mut HashMap<String, f32>,
        textures: &mut HashMap<String, TextureValue>,
        elements: &mut Vec<ElementJson>,
        curr_transform: Option<&BuiltinTransform>,
    ) {
        if let Some(arr) = node.as_array() {
            for item in arr {
                Self::process_handler_node(
                    item,
                    blockstate,
                    vars_str,
                    vars_num,
                    textures,
                    elements,
                    curr_transform,
                );
            }
            return;
        }

        // 1. Check condition
        if let Some(cond_val) = node.get("condition").and_then(|v| v.as_str()) {
            if !eval_miex_condition(cond_val, vars_str, blockstate) {
                return;
            }
        }

        // 2. Evaluate variables
        if let Some(vars_val) = node.get("variables").and_then(|v| v.as_array()) {
            for v_item in vars_val {
                if let Some(stmt) = v_item.as_str() {
                    Self::execute_variable_stmt(stmt, blockstate, vars_str, vars_num);
                }
            }
        }

        // 2.5 Compute active transform
        let active_transform = if let Some(t_val) = node.get("transform") {
            let mut rot = [0.0f32; 3];
            let mut pivot = [8.0f32; 3];
            if let Some(arr) = t_val.get("rotate").and_then(|v| v.as_array()) {
                for (i, v) in arr.iter().take(3).enumerate() {
                    rot[i] = if let Some(n) = v.as_f64() {
                        n as f32
                    } else if let Some(s) = v.as_str() {
                        eval_miex_num(s, vars_num)
                    } else {
                        0.0
                    };
                }
            }
            if let Some(arr) = t_val.get("pivot").and_then(|v| v.as_array()) {
                for (i, v) in arr.iter().take(3).enumerate() {
                    pivot[i] = if let Some(n) = v.as_f64() {
                        n as f32
                    } else if let Some(s) = v.as_str() {
                        eval_miex_num(s, vars_num)
                    } else {
                        8.0
                    };
                }
            }
            if let Some(parent_t) = curr_transform {
                Some(BuiltinTransform {
                    rotate: [
                        parent_t.rotate[0] + rot[0],
                        parent_t.rotate[1] + rot[1],
                        parent_t.rotate[2] + rot[2],
                    ],
                    pivot,
                })
            } else {
                Some(BuiltinTransform { rotate: rot, pivot })
            }
        } else {
            curr_transform.cloned()
        };

        // 3. Extract textures
        if let Some(tex_obj) = node.get("textures").and_then(|v| v.as_object()) {
            for (k, v) in tex_obj {
                if let Some(tex_expr) = v.as_str() {
                    let evaluated = eval_miex_string(tex_expr, vars_str, blockstate);
                    textures.insert(k.clone(), TextureValue::Path(evaluated.clone()));
                    if k == "texture" && !textures.contains_key("particle") {
                        textures.insert("particle".to_string(), TextureValue::Path(evaluated));
                    }
                }
            }
        }

        // 4. Extract elements
        if let Some(elems_val) = node.get("elements").and_then(|v| v.as_array()) {
            for elem_val in elems_val {
                if let Some(elem) =
                    Self::parse_element_node(elem_val, vars_num, active_transform.as_ref())
                {
                    elements.push(elem);
                }
            }
        }

        // 5. Recurse children
        if let Some(children) = node.get("children") {
            Self::process_handler_node(
                children,
                blockstate,
                vars_str,
                vars_num,
                textures,
                elements,
                active_transform.as_ref(),
            );
        }
    }

    fn execute_variable_stmt(
        stmt: &str,
        blockstate: &BlockState,
        vars_str: &mut HashMap<String, String>,
        vars_num: &mut HashMap<String, f32>,
    ) {
        if let Some((lhs, rhs)) = stmt.split_once('=') {
            let var_name = lhs.trim().to_string();
            let expr = rhs.trim();

            // Boolean expressions without ternary
            if (expr.contains(".contains(")
                || expr.contains(".endsWith(")
                || expr.contains("==")
                || expr.contains("!=")
                || expr.contains("||")
                || expr.contains("&&"))
                && !expr.contains('?')
            {
                let b = eval_miex_condition(expr, vars_str, blockstate);
                vars_str.insert(
                    var_name.clone(),
                    if b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    },
                );
            } else if expr.contains('?') {
                let evaluated = eval_miex_string(expr, vars_str, blockstate);
                if let Some(num) = eval_miex_arithmetic(&evaluated, vars_num, vars_str, blockstate)
                {
                    vars_num.insert(var_name.clone(), num);
                    vars_str.insert(var_name.clone(), num.to_string());
                } else if let Ok(n) = evaluated.parse::<f32>() {
                    vars_num.insert(var_name.clone(), n);
                    vars_str.insert(var_name.clone(), evaluated);
                } else {
                    vars_str.insert(var_name.clone(), evaluated);
                }
            } else if let Some(num) = eval_miex_arithmetic(expr, vars_num, vars_str, blockstate) {
                vars_num.insert(var_name.clone(), num);
                vars_str.insert(var_name.clone(), num.to_string());
            } else if expr.starts_with('\'')
                || expr.contains("thisBlock.")
                || (expr.contains('+') && expr.contains('\''))
            {
                let evaluated = eval_miex_string(expr, vars_str, blockstate);
                vars_str.insert(var_name.clone(), evaluated);
            } else if let Ok(n) = expr.parse::<f32>() {
                vars_num.insert(var_name.clone(), n);
                vars_str.insert(var_name.clone(), n.to_string());
            } else {
                let s = eval_miex_string(expr, vars_str, blockstate);
                if let Ok(n) = s.parse::<f32>() {
                    vars_num.insert(var_name.clone(), n);
                }
                vars_str.insert(var_name, s);
            }
        }
    }

    fn parse_element_node(
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
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
