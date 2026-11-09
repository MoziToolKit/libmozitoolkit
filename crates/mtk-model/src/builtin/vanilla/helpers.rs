use crate::parser::model_json::FaceJson;
use std::collections::HashMap;

/// Helper to create UV-explicit faces for a cuboid element with given texture variable and UVs.
pub fn make_box_faces(
    tex: &str,
    up: [f32; 4],
    down: [f32; 4],
    north: [f32; 4],
    south: [f32; 4],
    east: [f32; 4],
    west: [f32; 4],
) -> HashMap<String, FaceJson> {
    let mut map = HashMap::new();
    let entries = [
        ("up", up),
        ("down", down),
        ("north", north),
        ("south", south),
        ("east", east),
        ("west", west),
    ];
    for (dir, uv) in entries {
        map.insert(
            dir.to_string(),
            FaceJson {
                uv: Some(uv),
                texture: tex.to_string(),
                cullface: None,
                rotation: None,
                tintindex: None,
            },
        );
    }
    map
}

/// Helper to create simple faces for a cuboid element with given texture variable.
pub fn make_simple_faces(tex: &str) -> HashMap<String, FaceJson> {
    let mut map = HashMap::new();
    for dir in &["north", "south", "east", "west", "up", "down"] {
        map.insert(
            dir.to_string(),
            FaceJson {
                uv: None,
                texture: tex.to_string(),
                cullface: None,
                rotation: None,
                tintindex: None,
            },
        );
    }
    map
}
