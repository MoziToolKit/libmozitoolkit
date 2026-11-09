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

use crate::parser::model_json::FaceJson;
use std::collections::HashMap;

/// Evaluates standard Minecraft entity box UV mappings into standard face UV coordinates.
///
/// Ported from upstream MiEx `BuiltInModel.java`:
/// Given entity UV bounding box `[min_u, min_v, max_u, max_v]` and 3D bounds `[min_x, min_y, min_z, max_x, max_y, max_z]`:
pub fn entity_uv_to_faces(
    bounds: [f32; 6],
    entity_uv: [f32; 4],
    tex_ref: &str,
) -> HashMap<String, FaceJson> {
    let min_u = entity_uv[0];
    let min_v = entity_uv[1];
    let max_u = entity_uv[2];
    let max_v = entity_uv[3];

    let width = (bounds[3] - bounds[0]).abs();
    let height = (bounds[4] - bounds[1]).abs();
    let depth = (bounds[5] - bounds[2]).abs();

    let uv_span_w = max_u - min_u;
    let uv_span_h = max_v - min_v;

    let uv_w_unit = uv_span_w / (depth + width + depth + width).max(1e-5);
    let uv_h_unit = uv_span_h / (depth + height).max(1e-5);

    let mut faces = HashMap::new();

    // Directions: Down, Up, North, South, East, West
    let dir_uvs: [(&str, [f32; 4]); 6] = [
        // down: (depth + width) * uvWidth + minU, 0 * uvHeight + minV
        (
            "down",
            [
                (depth + width) * uv_w_unit + min_u,
                min_v,
                (depth + width + width) * uv_w_unit + min_u,
                depth * uv_h_unit + min_v,
            ],
        ),
        // up: depth * uvWidth + minU, 0 * uvHeight + minV
        (
            "up",
            [
                depth * uv_w_unit + min_u,
                min_v,
                (depth + width) * uv_w_unit + min_u,
                depth * uv_h_unit + min_v,
            ],
        ),
        // north: (depth + width + depth) * uvWidth + minU, depth * uvHeight + minV
        (
            "north",
            [
                (depth + width + depth) * uv_w_unit + min_u,
                depth * uv_h_unit + min_v,
                (depth + width + depth + width) * uv_w_unit + min_u,
                (depth + height) * uv_h_unit + min_v,
            ],
        ),
        // south: depth * uvWidth + minU, depth * uvHeight + minV
        (
            "south",
            [
                depth * uv_w_unit + min_u,
                depth * uv_h_unit + min_v,
                (depth + width) * uv_w_unit + min_u,
                (depth + height) * uv_h_unit + min_v,
            ],
        ),
        // east: (depth + width) * uvWidth + minU, depth * uvHeight + minV
        (
            "east",
            [
                (depth + width) * uv_w_unit + min_u,
                depth * uv_h_unit + min_v,
                (depth + width + depth) * uv_w_unit + min_u,
                (depth + height) * uv_h_unit + min_v,
            ],
        ),
        // west: 0 * uvWidth + minU, depth * uvHeight + minV
        (
            "west",
            [
                min_u,
                depth * uv_h_unit + min_v,
                depth * uv_w_unit + min_u,
                (depth + height) * uv_h_unit + min_v,
            ],
        ),
    ];

    for (dir, uv) in dir_uvs {
        faces.insert(
            dir.to_string(),
            FaceJson {
                uv: Some(uv),
                texture: tex_ref.to_string(),
                cullface: None,
                rotation: None,
                tintindex: None,
            },
        );
    }

    faces
}
