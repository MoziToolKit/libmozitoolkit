//! End portal surface.
//!
//! Source of truth: `EndPortalRenderer`, which draws the starfield over the portal block's inner
//! area. The block model only needs one upward-facing quad at the portal's surface height
//! (y = 12/16), textured with `entity/end_portal/end_portal`.

use std::collections::HashMap;

use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{BlockModelJson, ElementJson, FaceJson, TextureValue};

const SHEET: &str = "minecraft:entity/end_portal/end_portal";

/// Builds the end portal model, or `None` when `blockstate` is not an end portal.
pub(crate) fn build_end_portal_model(blockstate: &BlockState) -> Option<BlockModelJson> {
    let short = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);
    if short != "end_portal" {
        return None;
    }

    let mut faces = HashMap::new();
    faces.insert(
        "up".to_string(),
        FaceJson {
            uv: None,
            texture: "#texture".to_string(),
            cullface: None,
            rotation: None,
            tintindex: None,
        },
    );

    let element = ElementJson {
        from: [0.0, 12.0, 0.0],
        to: [16.0, 12.0, 16.0],
        rotation: None,
        transform: None,
        shade: Some(true),
        light_emission: None,
        faces,
    };

    let mut textures = HashMap::new();
    textures.insert("texture".to_string(), TextureValue::Path(SHEET.to_string()));
    textures.insert(
        "particle".to_string(),
        TextureValue::Path(SHEET.to_string()),
    );

    Some(BlockModelJson {
        parent: None,
        ambientocclusion: Some(true),
        gui_light: None,
        textures: Some(textures),
        elements: Some(vec![element]),
        display: None,
        overrides: None,
        ..Default::default()
    })
}
