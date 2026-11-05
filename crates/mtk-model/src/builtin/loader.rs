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
mod tests {
    use super::*;

    #[test]
    fn test_load_chest() {
        let bs_north = BlockState::parse("minecraft:chest[facing=north,type=single]").unwrap();
        let model_north =
            MiExModelLoader::load_for_blockstate(&bs_north).expect("Chest north should load");
        let elems_north = model_north.elements.unwrap_or_default();
        assert!(!elems_north.is_empty());
        let t_north = elems_north[0]
            .transform
            .as_ref()
            .expect("Transform should be present");
        assert_eq!(t_north.rotate[1], 0.0);

        let bs_south = BlockState::parse("minecraft:chest[facing=south,type=single]").unwrap();
        let model_south =
            MiExModelLoader::load_for_blockstate(&bs_south).expect("Chest south should load");
        let elems_south = model_south.elements.unwrap_or_default();
        let t_south = elems_south[0]
            .transform
            .as_ref()
            .expect("Transform should be present");
        assert_eq!(
            t_south.rotate[1], 180.0,
            "South chest should rotate 180 deg"
        );

        let bs_east = BlockState::parse("minecraft:chest[facing=east,type=single]").unwrap();
        let model_east =
            MiExModelLoader::load_for_blockstate(&bs_east).expect("Chest east should load");
        let elems_east = model_east.elements.unwrap();
        let t_east = elems_east[0].transform.as_ref().unwrap();
        assert_eq!(t_east.rotate[1], 90.0, "East chest should rotate 90 deg");
    }

    #[test]
    fn test_skull_floor_vs_wall() {
        let bs_floor = BlockState::parse("minecraft:skeleton_skull[rotation=0]").unwrap();
        let model_floor =
            MiExModelLoader::load_for_blockstate(&bs_floor).expect("Floor skull should load");
        let elems_floor = model_floor.elements.unwrap();
        assert_eq!(
            elems_floor[0].from,
            [4.0, 0.0, 4.0],
            "Floor skull must start at Y=0"
        );

        let bs_wall = BlockState::parse("minecraft:skeleton_wall_skull[facing=north]").unwrap();
        let model_wall =
            MiExModelLoader::load_for_blockstate(&bs_wall).expect("Wall skull should load");
        let elems_wall = model_wall.elements.unwrap();
        assert_eq!(
            elems_wall[0].from,
            [4.0, 4.0, 0.0],
            "Wall skull must start at Y=4, Z=0"
        );
    }

    #[test]
    fn test_dragon_and_piglin_heads() {
        let bs_dragon = BlockState::parse("minecraft:dragon_head[rotation=0]").unwrap();
        let model_dragon =
            MiExModelLoader::load_for_blockstate(&bs_dragon).expect("Dragon head should load");
        let elems_dragon = model_dragon.elements.unwrap();
        assert_eq!(elems_dragon.len(), 7, "Dragon head should have 7 elements");
        let tex_dragon = model_dragon.textures.unwrap();
        assert_eq!(
            tex_dragon.get("texture").unwrap().as_str(),
            "minecraft:entity/enderdragon/dragon"
        );

        // Verify dragon jaw rotation (element 2)
        let jaw = &elems_dragon[2];
        let jaw_rot = jaw
            .rotation
            .as_ref()
            .expect("Dragon jaw should have rotation");
        assert_eq!(jaw_rot.axis, "x");
        assert_eq!(jaw_rot.angle, 11.5);
        assert_eq!(jaw_rot.origin, [8.0, 3.0, 14.0]);

        let bs_piglin = BlockState::parse("minecraft:piglin_head[rotation=4]").unwrap();
        let model_piglin =
            MiExModelLoader::load_for_blockstate(&bs_piglin).expect("Piglin head should load");
        let elems_piglin = model_piglin.elements.unwrap();
        assert_eq!(elems_piglin.len(), 6, "Piglin head should have 6 elements");
        let tex_piglin = model_piglin.textures.unwrap();
        assert_eq!(
            tex_piglin.get("texture").unwrap().as_str(),
            "minecraft:entity/piglin/piglin"
        );
        let t_piglin = elems_piglin[0].transform.as_ref().unwrap();
        assert_eq!(
            t_piglin.rotate[1], 270.0,
            "Rotation 4 should be 270 deg (22.5*4 + 180)"
        );

        // Verify piglin ear rotations and distinct UVs
        let right_ear = &elems_piglin[2];
        let right_ear_rot = right_ear
            .rotation
            .as_ref()
            .expect("Right ear should have rotation");
        assert_eq!(right_ear_rot.axis, "z");
        assert_eq!(right_ear_rot.angle, 30.0);
        assert_eq!(right_ear_rot.origin, [12.5, 6.0, 8.0]);

        let left_ear = &elems_piglin[3];
        let left_ear_rot = left_ear
            .rotation
            .as_ref()
            .expect("Left ear should have rotation");
        assert_eq!(left_ear_rot.axis, "z");
        assert_eq!(left_ear_rot.angle, -30.0);
        assert_eq!(left_ear_rot.origin, [3.5, 6.0, 8.0]);

        // Verify left and right ears have different face UVs
        let right_faces = &right_ear.faces;
        let left_faces = &left_ear.faces;
        assert_ne!(
            right_faces.get("west").unwrap().uv,
            left_faces.get("west").unwrap().uv
        );
    }

    #[test]
    fn test_hanging_sign_load() {
        let bs_standing =
            BlockState::parse("minecraft:oak_hanging_sign[attached=false,rotation=0]").unwrap();
        let model_standing = MiExModelLoader::load_for_blockstate(&bs_standing)
            .expect("Standing hanging sign must load");
        let elems_standing = model_standing.elements.unwrap_or_default();
        // Board (1) + 4 angled chains (4) = 5 elements. Absolutely NO vertical sign post!
        assert_eq!(
            elems_standing.len(),
            5,
            "Standing hanging sign without attachment must have board and 4 chains"
        );
        let tex = model_standing.textures.unwrap();
        assert_eq!(
            tex.get("texture").unwrap().as_str(),
            "minecraft:entity/signs/hanging/oak"
        );
        for elem in &elems_standing {
            // Confirm none of the elements are the vertical sign post (which had X in [7, 9], Y in [0, 9.333])
            let is_post =
                elem.from[0] > 6.0 && elem.to[0] < 10.0 && elem.from[1] < 1.0 && elem.to[1] < 10.0;
            assert!(
                !is_post,
                "Hanging sign must NOT contain vertical standing sign post!"
            );
        }

        let bs_wall = BlockState::parse("minecraft:oak_wall_hanging_sign[facing=north]").unwrap();
        let model_wall =
            MiExModelLoader::load_for_blockstate(&bs_wall).expect("Wall hanging sign must load");
        let elems_wall = model_wall.elements.unwrap_or_default();
        for (i, el) in elems_wall.iter().enumerate() {
            println!("Wall elem {}: from={:?}, to={:?}", i, el.from, el.to);
        }
        // Board (1) + wall bracket (1) = 2 elements
        assert_eq!(
            elems_wall.len(),
            2,
            "Wall hanging sign must have board and bracket"
        );

        let bs_attached =
            BlockState::parse("minecraft:oak_hanging_sign[attached=true,rotation=0]").unwrap();
        let model_attached = MiExModelLoader::load_for_blockstate(&bs_attached)
            .expect("Attached hanging sign must load");
        let elems_attached = model_attached.elements.unwrap_or_default();
        // Board (1) + center chain (1) = 2 elements
        assert_eq!(
            elems_attached.len(),
            2,
            "Attached hanging sign must have board and center chain"
        );
    }

    #[test]
    fn test_unprofiled_skull_direct_atlas_textures() {
        use crate::entity::skull::SkullKind;
        use crate::entity::BlockEntityData;

        // 1. Zombie head without profile -> direct internal zombie texture
        let bs_zombie = BlockState::parse("minecraft:zombie_head[rotation=0]").unwrap();
        let entity_zombie = BlockEntityData::Skull {
            kind: SkullKind::Zombie,
            rotation: Some(0),
            profile: None,
        };
        let model_zombie =
            MiExModelLoader::load_for_blockstate_with_entity(&bs_zombie, Some(&entity_zombie))
                .expect("Zombie head should load");
        let tex_zombie = model_zombie.textures.unwrap();
        assert_eq!(
            tex_zombie.get("texture").unwrap().as_str(),
            "minecraft:entity/zombie/zombie"
        );

        // 2. Creeper wall head without profile -> direct internal creeper texture
        let bs_creeper = BlockState::parse("minecraft:creeper_wall_head[facing=north]").unwrap();
        let entity_creeper = BlockEntityData::Skull {
            kind: SkullKind::Creeper,
            rotation: None,
            profile: None,
        };
        let model_creeper =
            MiExModelLoader::load_for_blockstate_with_entity(&bs_creeper, Some(&entity_creeper))
                .expect("Creeper head should load");
        let tex_creeper = model_creeper.textures.unwrap();
        assert_eq!(
            tex_creeper.get("texture").unwrap().as_str(),
            "minecraft:entity/creeper/creeper"
        );

        // 3. Player head without profile -> direct internal default steve texture
        let bs_player = BlockState::parse("minecraft:player_head[rotation=4]").unwrap();
        let entity_player = BlockEntityData::Skull {
            kind: SkullKind::Player,
            rotation: Some(4),
            profile: None,
        };
        let model_player =
            MiExModelLoader::load_for_blockstate_with_entity(&bs_player, Some(&entity_player))
                .expect("Player head without profile should load");
        let tex_player = model_player.textures.unwrap();
        assert_eq!(
            tex_player.get("texture").unwrap().as_str(),
            "minecraft:entity/player/wide/steve"
        );
    }
}
