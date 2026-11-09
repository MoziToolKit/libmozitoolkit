use super::helpers::make_simple_faces;
use crate::builtin::patch::apply_bell_patches;
use crate::parser::model_json::{BlockModelJson, ElementJson, TextureValue};
use std::collections::HashMap;

/// Synthesizes vanilla bell frame models and applies bell body patches.
pub fn create_builtin_bell_model(stem: &str) -> Option<BlockModelJson> {
    let mut textures = HashMap::new();
    textures.insert(
        "particle".to_string(),
        TextureValue::Path("minecraft:block/bell_bottom".to_string()),
    );
    textures.insert(
        "bar".to_string(),
        TextureValue::Path("minecraft:block/dark_oak_planks".to_string()),
    );
    textures.insert(
        "post".to_string(),
        TextureValue::Path("minecraft:block/stone".to_string()),
    );

    let mut elements = Vec::new();

    match stem {
        "bell_floor" => {
            elements.push(ElementJson {
                from: [2.0, 13.0, 7.0],
                to: [14.0, 15.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
            elements.push(ElementJson {
                from: [14.0, 0.0, 6.0],
                to: [16.0, 16.0, 10.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#post"),
            });
            elements.push(ElementJson {
                from: [0.0, 0.0, 6.0],
                to: [2.0, 16.0, 10.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#post"),
            });
        }
        "bell_ceiling" => {
            elements.push(ElementJson {
                from: [7.0, 13.0, 7.0],
                to: [9.0, 16.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
        }
        "bell_wall" => {
            elements.push(ElementJson {
                from: [3.0, 13.0, 7.0],
                to: [16.0, 15.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
        }
        "bell_between_walls" => {
            elements.push(ElementJson {
                from: [0.0, 13.0, 7.0],
                to: [16.0, 15.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
        }
        _ => return None,
    }

    let mut model = BlockModelJson {
        parent: None,
        ambientocclusion: Some(true),
        textures: Some(textures),
        elements: Some(elements),
    };

    apply_bell_patches("block/bell_", &mut model);
    Some(model)
}
