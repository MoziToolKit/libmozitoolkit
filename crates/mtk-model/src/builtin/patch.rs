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

use crate::parser::model_json::{BlockModelJson, ElementJson, FaceJson};
use std::collections::HashMap;

/// Applies MiEx patches to BlockModelJson (such as bell elements).
/// Based on upstream MiEx `miex_patches.json`.
pub fn apply_bell_patches(model_id: &str, model: &mut BlockModelJson) -> bool {
    let clean = model_id.strip_prefix("minecraft:").unwrap_or(model_id);
    if !clean.starts_with("block/bell_") && clean != "block/bell" {
        return false;
    }

    // Insert bell_body texture if missing
    let textures = model.textures.get_or_insert_with(HashMap::new);
    if !textures.contains_key("bell_body") {
        textures.insert(
            "bell_body".to_string(),
            crate::parser::model_json::TextureValue::Path("entity/bell/bell_body".to_string()),
        );
    }

    let elements = model.elements.get_or_insert_with(Vec::new);

    // Patch bell main bell-body elements from upstream MiEx
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

    true
}
