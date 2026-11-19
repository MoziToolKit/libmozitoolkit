//! Chest geometry, transcribed from the client.
//!
//! Source of truth:
//! - `net/minecraft/client/model/object/chest/ChestModel.java` — `createSingleBodyLayer`,
//!   `createDoubleBodyLeftLayer`, `createDoubleBodyRightLayer`
//! - `net/minecraft/client/renderer/blockentity/ChestRenderer.java` — layer choice by `type`, the
//!   `facing` rotation (`rotationAround(YP, -facing.toYRot(), 0.5, 0, 0.5)`) and the sprite choice
//!   (`Sheets.chooseSprite(material, type)` → `entity/chest/<material>[_left|_right]`)
//!
//! Chests ship no client model JSON — the jar only carries a `particle` placeholder — so this
//! generator is the production geometry source, not an optimization.
//!
//! The numbers below are the client's boxes expressed in the block-model space (y up, +z south).
//! `ModelPart` is y-down, so every box is flipped on y and the face UVs are permuted; the two
//! double-chest layers also swap names, which is why the `_left` sheet pairs with the geometry the
//! client uses for its right half. `tests/test_chest_model.rs` pins this against the client.

use std::collections::HashMap;

use crate::builtin::props::cube::{visible_faces_except, CubeSpec};
use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{
    BlockModelJson, BuiltinTransform, ElementJson, FaceJson, TextureValue,
};

/// Texture sheet the chest models are unwrapped against (vanilla `Sheets.CHEST_SHEET`).
const CHEST_TEXTURE_SIZE: f32 = 64.0;

/// One textured box of a chest layer, in y-up block-model space.
struct ChestBox {
    tex_offs: [f32; 2],
    /// Minimum corner of the vanilla box, after the z mirror.
    min: [f32; 3],
    /// Maximum corner of the vanilla box, after the z mirror.
    max: [f32; 3],
}

/// Single chest: `ChestModel.createSingleBodyLayer()` on a y-up, +z-south grid.
///
/// `min`/`max` are the vanilla box after the z mirror; the emitted corner order comes from
/// [`model_corners`].
const SINGLE_BOXES: [ChestBox; 3] = [
    // bottom: texOffs(0, 19).addBox(1, 0, 1, 14, 10, 14)
    ChestBox {
        tex_offs: [0.0, 19.0],
        min: [1.0, 0.0, 1.0],
        max: [15.0, 10.0, 15.0],
    },
    // lid: texOffs(0, 0).addBox(1, 0, 0, 14, 5, 14) at PartPose.offset(0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        min: [1.0, 9.0, 1.0],
        max: [15.0, 14.0, 15.0],
    },
    // lock: texOffs(0, 0).addBox(7, -2, 14, 2, 4, 1) at PartPose.offset(0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        min: [7.0, 7.0, 0.0],
        max: [9.0, 11.0, 1.0],
    },
];

/// `_left` sheet: `createDoubleBodyRightLayer()` after the z mirror.
const LEFT_BOXES: [ChestBox; 3] = [
    // bottom/lid: addBox(1, 0, 1, 15, 10|5, 14)
    ChestBox {
        tex_offs: [0.0, 19.0],
        min: [1.0, 0.0, 1.0],
        max: [16.0, 10.0, 15.0],
    },
    ChestBox {
        tex_offs: [0.0, 0.0],
        min: [1.0, 9.0, 1.0],
        max: [16.0, 14.0, 15.0],
    },
    // lock: addBox(15, -2, 14, 1, 4, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        min: [15.0, 7.0, 0.0],
        max: [16.0, 11.0, 1.0],
    },
];

/// `_right` sheet: `createDoubleBodyLeftLayer()` after the z mirror.
const RIGHT_BOXES: [ChestBox; 3] = [
    // bottom/lid: addBox(0, 0, 1, 15, 10|5, 14)
    ChestBox {
        tex_offs: [0.0, 19.0],
        min: [0.0, 0.0, 1.0],
        max: [15.0, 10.0, 15.0],
    },
    ChestBox {
        tex_offs: [0.0, 0.0],
        min: [0.0, 9.0, 1.0],
        max: [15.0, 14.0, 15.0],
    },
    // lock: addBox(0, -2, 14, 1, 4, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        min: [0.0, 7.0, 0.0],
        max: [1.0, 11.0, 1.0],
    },
];

/// `ChestType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChestType {
    Single,
    Left,
    Right,
}

impl ChestType {
    fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("left") => Self::Left,
            Some("right") => Self::Right,
            _ => Self::Single,
        }
    }

    fn boxes(self) -> &'static [ChestBox] {
        match self {
            Self::Single => &SINGLE_BOXES,
            Self::Left => &LEFT_BOXES,
            Self::Right => &RIGHT_BOXES,
        }
    }

    fn texture_suffix(self) -> &'static str {
        match self {
            Self::Single => "",
            Self::Left => "_left",
            Self::Right => "_right",
        }
    }
}

/// Sheet for `short_name`, i.e. `Sheets.chooseSprite(ChestRenderer.getChestMaterial(...), type)`.
///
/// Two details a naive port gets wrong:
/// - Ender chests have a single sheet: `Sheets.chooseSprite` returns `ENDER_CHEST_LOCATION` for
///   every `ChestType`, and `entity/chest/ender_left.png` does not exist. Appending the shape
///   suffix makes a double ender chest fall back to an untextured model.
/// - Copper chests pick a sheet per oxidation state (`copper`, `_exposed`, `_weathered`,
///   `_oxidized`); waxed variants share the sheet of their oxidation state.
fn chest_sheet(short_name: &str, chest_type: ChestType) -> String {
    if short_name == "ender_chest" {
        return "minecraft:entity/chest/ender".to_string();
    }

    let base = if short_name.contains("oxidized") {
        "copper_oxidized"
    } else if short_name.contains("weathered") {
        "copper_weathered"
    } else if short_name.contains("exposed") {
        "copper_exposed"
    } else if short_name.contains("copper") {
        // Plain and waxed-unaffected copper share `copper.png`.
        "copper"
    } else if short_name.contains("trapped") {
        "trapped"
    } else {
        "normal"
    };

    format!(
        "minecraft:entity/chest/{base}{}",
        chest_type.texture_suffix()
    )
}

/// `-facing.toYRot()` for the four horizontal facings.
fn facing_y_rotation(facing: &str) -> f32 {
    match facing {
        "east" => 90.0,
        "south" => 180.0,
        "west" => 270.0,
        // north (and any unexpected value) leaves the model unrotated.
        _ => 0.0,
    }
}

/// Builds the chest model for `blockstate`, or `None` when it is not a chest.
pub(crate) fn build_chest_model(blockstate: &BlockState) -> Option<BlockModelJson> {
    let short = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);
    let is_chest = short == "chest" || short.ends_with("_chest");
    if !is_chest {
        return None;
    }

    let chest_type = ChestType::parse(blockstate.properties.get("type").map(String::as_str));
    let texture = chest_sheet(short, chest_type);

    let facing = blockstate
        .properties
        .get("facing")
        .map(String::as_str)
        .unwrap_or("north");
    let transform = BuiltinTransform {
        rotate: [0.0, facing_y_rotation(facing), 0.0],
        // A Y rotation only needs the axis, so the vanilla pivot (0.5, 0, 0.5) and the block
        // centre are equivalent here.
        pivot: [8.0, 8.0, 8.0],
    };

    let elements: Vec<ElementJson> = chest_type
        .boxes()
        .iter()
        .map(|spec| {
            let dimensions = [
                spec.max[0] - spec.min[0],
                spec.max[1] - spec.min[1],
                spec.max[2] - spec.min[2],
            ];
            let cube = CubeSpec::new(spec.tex_offs, spec.min, dimensions);
            let mut faces = HashMap::new();
            for face in visible_faces_except(&[]) {
                let uv = cube.model_face_uvs(face);
                let scale = 16.0 / CHEST_TEXTURE_SIZE;
                faces.insert(
                    face.as_str().to_string(),
                    FaceJson {
                        uv: Some([uv[0] * scale, uv[1] * scale, uv[2] * scale, uv[3] * scale]),
                        texture: "#texture".to_string(),
                        cullface: None,
                        rotation: None,
                        tintindex: None,
                    },
                );
            }
            let (from, to) = crate::builtin::props::cube::model_corners(spec.min, spec.max);
            ElementJson {
                from,
                to,
                rotation: None,
                transform: Some(transform.clone()),
                shade: Some(true),
                light_emission: None,
                faces,
            }
        })
        .collect();

    let mut textures = HashMap::new();
    textures.insert("texture".to_string(), TextureValue::Path(texture));
    // Particles stay on the generic chest sheet for every material; only the geometry switches to
    // the material sheet.
    textures.insert(
        "particle".to_string(),
        TextureValue::Path("minecraft:entity/chest/normal".to_string()),
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
