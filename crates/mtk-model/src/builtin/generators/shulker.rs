//! Shulker box geometry.
//!
//! Source of truth: `net/minecraft/client/model/monster/shulker/ShulkerModel.createBoxLayer()`
//! (the shell only: `CreateBoxLayer` is what `ShulkerBoxRenderer` bakes) plus
//! `ShulkerBoxRenderer.createModelTransform()` for the facing rotation and
//! `Sheets.getShulkerBoxSprite` for the sheet.
//!
//! Two boxes form the shell: an 8-high base and a 12-high lid inset by the renderer's
//! `scale(0.9995)`. The UV rectangles come from the shulker sheet being authored at 16 units per
//! face (a 64px texture normalised to the block-model's 16-unit space), so they are transcribed
//! from the definition that shipped before this generator rather than recomputed from
//! `ModelPart.Cube`.

use std::collections::HashMap;

use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{
    BlockModelJson, BuiltinTransform, ElementJson, FaceJson, TextureValue,
};

/// Renderer inset: `scale(0.9995)` around the block centre puts the lid at 0.01..15.99.
const LID_INSET: f32 = 0.01;

/// One box plus its six face rectangles, in 0..16 block-model UV space.
struct ShulkerBox {
    from: [f32; 3],
    to: [f32; 3],
    /// `(down, up, north, south, east, west)` rectangles, in that order.
    uvs: [[f32; 4]; 6],
}

/// `ShulkerModel.createBoxLayer()`: `base` at texOffs(0, 28), `lid` at texOffs(0, 0), both with
/// `PartPose.offset(0, 24, 0)`.
const BOXES: [ShulkerBox; 2] = [
    // base: addBox(-8, -8, -8, 16, 8, 16)
    ShulkerBox {
        from: [0.0, 0.0, 0.0],
        to: [16.0, 8.0, 16.0],
        uvs: [
            [8.0, 7.0, 12.0, 11.0],
            [4.0, 7.0, 8.0, 11.0],
            [12.0, 11.0, 16.0, 13.0],
            [4.0, 11.0, 8.0, 13.0],
            [8.0, 11.0, 12.0, 13.0],
            [0.0, 11.0, 4.0, 13.0],
        ],
    },
    // lid: addBox(-8, -16, -8, 16, 12, 16), inset by the renderer scale
    ShulkerBox {
        from: [LID_INSET, 4.0, LID_INSET],
        to: [16.0 - LID_INSET, 16.0, 16.0 - LID_INSET],
        uvs: [
            [8.0, 0.0, 12.0, 3.997_855_4],
            [4.0, 0.0, 8.0, 3.997_855_4],
            [12.0, 3.997_855_4, 16.0, 7.0],
            [4.0, 3.997_855_4, 8.0, 7.0],
            [8.0, 3.997_855_4, 12.0, 7.0],
            [0.0, 3.997_855_4, 4.0, 7.0],
        ],
    },
];

/// Face keys in the same order as [`ShulkerBox::uvs`].
const FACE_KEYS: [&str; 6] = ["down", "up", "north", "south", "east", "west"];

/// `ShulkerBoxRenderer.createModelTransform`: `rotate(direction.getRotation())` on top of the y
/// flip, which in block-model terms is a pair of Euler angles around the block centre.
fn facing_rotation(facing: &str) -> [f32; 3] {
    match facing {
        "down" => [180.0, 0.0, 0.0],
        "north" => [90.0, 0.0, 0.0],
        "east" => [90.0, 90.0, 0.0],
        "south" => [90.0, 180.0, 0.0],
        "west" => [90.0, 270.0, 0.0],
        // up (and anything unexpected) leaves the box upright.
        _ => [0.0, 0.0, 0.0],
    }
}

/// `Sheets.getShulkerBoxSprite`: `entity/shulker/shulker` for the undyed box, otherwise
/// `entity/shulker/shulker_<colour>`.
fn shulker_sheet(short_name: &str) -> String {
    let colour = short_name
        .strip_suffix("_shulker_box")
        .unwrap_or(short_name);
    if colour.is_empty() || colour == "shulker_box" {
        "minecraft:entity/shulker/shulker".to_string()
    } else {
        format!("minecraft:entity/shulker/shulker_{colour}")
    }
}

/// Builds the shulker box model, or `None` when `blockstate` is not a shulker box.
pub(crate) fn build_shulker_model(blockstate: &BlockState) -> Option<BlockModelJson> {
    let short = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);
    if !short.ends_with("shulker_box") {
        return None;
    }

    let sheet = shulker_sheet(short);
    let facing = blockstate
        .properties
        .get("facing")
        .map(String::as_str)
        .unwrap_or("up");
    let transform = BuiltinTransform {
        rotate: facing_rotation(facing),
        pivot: [8.0, 8.0, 8.0],
    };

    let elements = BOXES
        .iter()
        .map(|spec| {
            let mut faces = HashMap::new();
            for (index, key) in FACE_KEYS.iter().enumerate() {
                faces.insert(
                    (*key).to_string(),
                    FaceJson {
                        uv: Some(spec.uvs[index]),
                        texture: "#texture".to_string(),
                        cullface: None,
                        rotation: None,
                        tintindex: None,
                    },
                );
            }
            ElementJson {
                from: spec.from,
                to: spec.to,
                rotation: None,
                transform: Some(transform.clone()),
                shade: Some(true),
                light_emission: None,
                faces,
            }
        })
        .collect();

    let mut textures = HashMap::new();
    textures.insert("texture".to_string(), TextureValue::Path(sheet));
    textures.insert(
        "particle".to_string(),
        TextureValue::Path("minecraft:entity/shulker/shulker".to_string()),
    );

    Some(BlockModelJson {
        parent: None,
        ambientocclusion: Some(true),
        gui_light: None,
        textures: Some(textures),
        elements: Some(elements),
        display: None,
        overrides: None,
    })
}
