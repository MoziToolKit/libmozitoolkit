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

use std::collections::HashMap;

use crate::builtin::props::cube::{visible_faces_except, CubeSpec};
use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{
    BlockModelJson, BuiltinTransform, ElementJson, FaceJson, TextureValue,
};
use mtk_core::Direction;

/// Texture sheet the chest models are unwrapped against (vanilla `Sheets.CHEST_SHEET`).
const CHEST_TEXTURE_SIZE: f32 = 64.0;

/// One textured box of a chest layer.
///
/// `hidden` mirrors the client's `visibleFaces` argument. It is not an optimisation: the shared face
/// of a double chest has no opaque pixels in the `_left`/`_right` sheets (verified for
/// `entity/chest/normal_left.png`), so the face is blank in vanilla as well.
struct ChestBox {
    tex_offs: [f32; 2],
    origin: [f32; 3],
    dimensions: [f32; 3],
    hidden: &'static [Direction],
}

/// `ChestModel.createSingleBodyLayer()`.
const SINGLE_BOXES: [ChestBox; 3] = [
    // bottom: texOffs(0, 19).addBox(1, 0, 1, 14, 10, 14)
    ChestBox {
        tex_offs: [0.0, 19.0],
        origin: [1.0, 0.0, 1.0],
        dimensions: [14.0, 10.0, 14.0],
        hidden: &[],
    },
    // lid: texOffs(0, 0).addBox(1, 0, 0, 14, 5, 14) at PartPose.offset(0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        origin: [1.0, 9.0, 1.0],
        dimensions: [14.0, 5.0, 14.0],
        hidden: &[],
    },
    // lock: texOffs(0, 0).addBox(7, -2, 14, 2, 4, 1) at PartPose.offset(0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        origin: [7.0, 7.0, 15.0],
        dimensions: [2.0, 4.0, 1.0],
        hidden: &[],
    },
];

/// `ChestModel.createDoubleBodyLeftLayer()`: the left half, west face suppressed.
const LEFT_BOXES: [ChestBox; 3] = [
    // bottom: addBox(0, 0, 1, 15, 10, 14) with visibleFaces = allOfEnumExcept(WEST)
    ChestBox {
        tex_offs: [0.0, 19.0],
        origin: [0.0, 0.0, 1.0],
        dimensions: [15.0, 10.0, 14.0],
        hidden: &[Direction::West],
    },
    // lid: addBox(0, 0, 0, 15, 5, 14) at (0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        origin: [0.0, 9.0, 1.0],
        dimensions: [15.0, 5.0, 14.0],
        hidden: &[Direction::West],
    },
    // lock: addBox(0, -2, 14, 1, 4, 1) at (0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        origin: [0.0, 7.0, 15.0],
        dimensions: [1.0, 4.0, 1.0],
        hidden: &[Direction::West],
    },
];

/// `ChestModel.createDoubleBodyRightLayer()`: the right half, east face suppressed.
const RIGHT_BOXES: [ChestBox; 3] = [
    // bottom: addBox(1, 0, 1, 15, 10, 14) with visibleFaces = allOfEnumExcept(EAST)
    ChestBox {
        tex_offs: [0.0, 19.0],
        origin: [1.0, 0.0, 1.0],
        dimensions: [15.0, 10.0, 14.0],
        hidden: &[Direction::East],
    },
    // lid: addBox(1, 0, 0, 15, 5, 14) at (0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        origin: [1.0, 9.0, 1.0],
        dimensions: [15.0, 5.0, 14.0],
        hidden: &[Direction::East],
    },
    // lock: addBox(15, -2, 14, 1, 4, 1) at (0, 9, 1)
    ChestBox {
        tex_offs: [0.0, 0.0],
        origin: [15.0, 7.0, 15.0],
        dimensions: [1.0, 4.0, 1.0],
        hidden: &[Direction::East],
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

/// Chest material, i.e. `Sheets.chooseSprite` families.
fn chest_material(short_name: &str) -> &'static str {
    // `ChestRenderer.getChestMaterial`: copper chests read their oxidation from the block, trapped
    // chests use the trapped sheet, everything else falls back to the normal chest sheet.
    if short_name.contains("copper") {
        "copper"
    } else if short_name.starts_with("trapped_chest") || short_name == "trapped_chest" {
        "trapped"
    } else if short_name.starts_with("ender_chest") || short_name == "ender_chest" {
        "ender"
    } else {
        "normal"
    }
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

    let material = chest_material(short);
    let chest_type = ChestType::parse(blockstate.properties.get("type").map(String::as_str));
    let texture = format!(
        "minecraft:entity/chest/{material}{}",
        chest_type.texture_suffix()
    );

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
            let cube = CubeSpec::new(spec.tex_offs, spec.origin, spec.dimensions);
            let mut faces = HashMap::new();
            for face in visible_faces_except(spec.hidden) {
                let uv = cube.face_uvs(face);
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
            let (min, max) = cube.bounds();
            ElementJson {
                from: min,
                to: max,
                rotation: None,
                transform: Some(transform.clone()),
                shade: Some(true),
                light_emission: None,
                faces,
            }
        })
        .collect();

    let mut textures = HashMap::new();
    // Vanilla's chest sheet is not an atlas sprite, so `particle` points at the same texture.
    textures.insert("texture".to_string(), TextureValue::Path(texture.clone()));
    textures.insert("particle".to_string(), TextureValue::Path(texture));

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
