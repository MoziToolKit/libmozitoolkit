use std::collections::HashMap;

use mtk_core::Direction;
use mtk_core::Vec2;
use mtk_model::baker::baked_model::{BakedElement, BakedFace, BakedModel};

mod basic;
mod e2e;

pub(crate) fn create_sample_cube(state: &str, tex: &str) -> BakedModel {
    let mut model = BakedModel {
        block_state: state.to_string(),
        is_cube: true,
        is_opaque: true,
        ..Default::default()
    };
    let mut faces = HashMap::new();
    for dir in Direction::ALL {
        let face = BakedFace {
            direction: dir,
            texture: tex.to_string(),
            cullface: Some(dir),
            uvs: [
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
            ..Default::default()
        };
        faces.insert(dir, face);
    }
    model.elements.push(BakedElement {
        from_pos: [0.0, 0.0, 0.0],
        to_pos: [16.0, 16.0, 16.0],
        faces,
    });
    model.rebuild_face_buckets();
    model
}
