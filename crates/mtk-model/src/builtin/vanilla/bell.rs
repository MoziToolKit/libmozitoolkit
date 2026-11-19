use super::helpers::make_simple_faces;
use crate::parser::model_json::{BlockModelJson, ElementJson, FaceJson, TextureValue};
use std::collections::HashMap;

/// Synthesizes the vanilla bell frame (it ships no block model) plus its body.
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
                light_emission: None,
                faces: make_simple_faces("#bar"),
            });
            elements.push(ElementJson {
                from: [14.0, 0.0, 6.0],
                to: [16.0, 16.0, 10.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                light_emission: None,
                faces: make_simple_faces("#post"),
            });
            elements.push(ElementJson {
                from: [0.0, 0.0, 6.0],
                to: [2.0, 16.0, 10.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                light_emission: None,
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
                light_emission: None,
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
                light_emission: None,
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
                light_emission: None,
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
        ..Default::default()
    };

    add_bell_body(&mut model);
    Some(model)
}

fn add_bell_body(model: &mut BlockModelJson) {
    // Insert bell_body texture if missing
    let textures = model.textures.get_or_insert_with(HashMap::new);
    if !textures.contains_key("bell_body") {
        textures.insert(
            "bell_body".to_string(),
            crate::parser::model_json::TextureValue::Path("entity/bell/bell_body".to_string()),
        );
    }

    let elements = model.elements.get_or_insert_with(Vec::new);

    // The bell body itself (the frame above is synthesized from the blockstate).
    let bell_elem_1 = ElementJson {
        from: [5.0, 6.0, 5.0],
        to: [11.0, 13.0, 11.0],
        rotation: None,
        transform: None,
        shade: Some(true),
        light_emission: None,
        faces: {
            let mut f = HashMap::new();
            f.insert(
                "north".to_string(),
                FaceJson {
                    uv: Some([3.0, 3.0, 6.0, 6.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "east".to_string(),
                FaceJson {
                    uv: Some([6.0, 3.0, 9.0, 6.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "south".to_string(),
                FaceJson {
                    uv: Some([9.0, 3.0, 12.0, 6.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "west".to_string(),
                FaceJson {
                    uv: Some([0.0, 3.0, 3.0, 6.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "up".to_string(),
                FaceJson {
                    uv: Some([3.0, 0.0, 6.0, 3.0]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "down".to_string(),
                FaceJson {
                    uv: Some([6.0, 0.0, 9.0, 3.0]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f
        },
    };

    let bell_elem_2 = ElementJson {
        from: [4.0, 4.0, 4.0],
        to: [12.0, 6.0, 12.0],
        rotation: None,
        transform: None,
        shade: Some(true),
        light_emission: None,
        faces: {
            let mut f = HashMap::new();
            f.insert(
                "north".to_string(),
                FaceJson {
                    uv: Some([4.0, 10.5, 8.0, 11.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "east".to_string(),
                FaceJson {
                    uv: Some([8.0, 10.5, 12.0, 11.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "south".to_string(),
                FaceJson {
                    uv: Some([12.0, 10.5, 16.0, 11.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "west".to_string(),
                FaceJson {
                    uv: Some([0.0, 10.5, 4.0, 11.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "up".to_string(),
                FaceJson {
                    uv: Some([4.0, 6.5, 8.0, 10.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f.insert(
                "down".to_string(),
                FaceJson {
                    uv: Some([8.0, 6.5, 12.0, 10.5]),
                    texture: "#bell_body".to_string(),
                    cullface: None,
                    rotation: None,
                    tintindex: None,
                },
            );
            f
        },
    };

    elements.push(bell_elem_1);
    elements.push(bell_elem_2);
}
