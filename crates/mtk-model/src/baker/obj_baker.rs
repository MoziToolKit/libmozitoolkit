//! Wavefront OBJ model baking routines.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;

use crate::baker::baked_model::{BakedFace, BakedModel};
use crate::error::ModelError;
use crate::parser::blockstate::BlockState;
use crate::parser::obj::{ModObjLoader, WavefrontObjParser};

use super::emissive::is_block_emissive;

/// Bakes a Wavefront OBJ mesh into a `BakedModel`.
#[allow(clippy::too_many_arguments)]
pub fn bake_obj_model_impl(
    block_state: &str,
    obj_text: &str,
    texture_mappings: &HashMap<String, String>,
    fallback_texture: &str,
    rot_x: f32,
    rot_y: f32,
    rot_z: f32,
    offset: Vec3,
    object_filter: Option<&[&str]>,
) -> Result<BakedModel, ModelError> {
    let raw_faces = WavefrontObjParser::parse_str(obj_text, object_filter);
    let baked_obj_faces = ModObjLoader::process_raw_faces(
        &raw_faces,
        texture_mappings,
        fallback_texture,
        rot_x,
        rot_y,
        rot_z,
        offset,
    );

    let mut six_faces = [
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
    ];

    for dir in Direction::ALL {
        let idx = dir.to_index();
        if let Some(matching) = baked_obj_faces.iter().find(|f| f.direction == dir) {
            let mut v4 = [Vec3::ZERO; 4];
            let mut u4 = [Vec2::ZERO; 4];
            let count = matching.vertices.len().min(4);
            v4[..count].copy_from_slice(&matching.vertices[..count]);
            u4[..count].copy_from_slice(&matching.uvs[..count]);
            six_faces[idx] = BakedFace {
                direction: dir,
                texture: matching.texture.clone(),
                uv_rot: 0.0,
                uv_bounds: [0.0, 0.0, 1.0, 1.0],
                tint_index: matching.tint_index,
                cullface: None,
                vertices: v4,
                uvs: u4,
                normal: matching.normal,
                ..Default::default()
            };
        } else if let Some(first) = baked_obj_faces.first() {
            let mut v4 = [Vec3::ZERO; 4];
            let mut u4 = [Vec2::ZERO; 4];
            let count = first.vertices.len().min(4);
            v4[..count].copy_from_slice(&first.vertices[..count]);
            u4[..count].copy_from_slice(&first.uvs[..count]);
            six_faces[idx] = BakedFace {
                direction: dir,
                texture: first.texture.clone(),
                uv_rot: 0.0,
                uv_bounds: [0.0, 0.0, 1.0, 1.0],
                tint_index: first.tint_index,
                cullface: None,
                vertices: v4,
                uvs: u4,
                normal: dir.normal(),
                ..Default::default()
            };
        } else {
            six_faces[idx] = BakedFace {
                direction: dir,
                texture: fallback_texture.to_string(),
                normal: dir.normal(),
                ..Default::default()
            };
        }
    }

    let blockstate = BlockState::parse(block_state).unwrap_or_else(|_| BlockState {
        namespace: "minecraft".to_string(),
        name: "custom_obj".to_string(),
        properties: std::collections::BTreeMap::new(),
    });
    let emissive = is_block_emissive(&blockstate);
    let cull_meta = mtk_cull::compute_block_cull_meta(block_state, None, Some(false));

    let mut baked_model = BakedModel {
        block_state: block_state.to_string(),
        elements: Vec::new(),
        obj_faces: baked_obj_faces,
        faces: six_faces,
        is_cube: false,
        is_opaque: false,
        is_emissive: emissive,
        cull_meta: Some(cull_meta),
        culled_faces: Default::default(),
        unculled_faces: Default::default(),
    };
    baked_model.rebuild_face_buckets();
    Ok(baked_model)
}
