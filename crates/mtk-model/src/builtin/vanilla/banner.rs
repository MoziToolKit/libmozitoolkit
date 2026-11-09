use super::helpers::make_box_faces;
use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{BlockModelJson, BuiltinTransform, ElementJson, TextureValue};
use std::collections::HashMap;

/// Procedurally synthesizes a banner model (standing or wall) with pole, crossbar, and cloth.
pub fn create_builtin_banner_model(
    blockstate: &BlockState,
    entity: Option<&crate::entity::BlockEntityData>,
) -> Option<BlockModelJson> {
    let name = blockstate.name.as_str();
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);
    let is_wall = clean.ends_with("_wall_banner");

    let rot_y = if is_wall {
        let facing = blockstate
            .properties
            .get("facing")
            .map(|s| s.as_str())
            .unwrap_or("north");
        match facing {
            "north" => 180.0,
            "east" => 270.0,
            "south" => 0.0,
            "west" => 90.0,
            _ => 180.0,
        }
    } else {
        let rotation: f32 = blockstate
            .properties
            .get("rotation")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        (rotation / 16.0) * 360.0
    };

    let cloth_texture = if let Some(crate::entity::BlockEntityData::Banner {
        base_color,
        patterns,
    }) = entity
    {
        crate::banner::banner_texture_identifier(base_color, patterns)
    } else {
        let stem = clean.strip_suffix("_wall_banner").unwrap_or(clean);
        let color = stem.strip_suffix("_banner").unwrap_or(stem);
        format!("minecraft:banner/{}", color)
    };

    let mut textures = HashMap::new();
    textures.insert(
        "particle".to_string(),
        TextureValue::Path("minecraft:entity/banner/banner_base".to_string()),
    );
    textures.insert(
        "base".to_string(),
        TextureValue::Path("minecraft:entity/banner/banner_base".to_string()),
    );
    textures.insert("cloth".to_string(), TextureValue::Path(cloth_texture));

    let scale = 2.0 / 3.0;
    let offset_x = 8.0;
    let offset_y = if is_wall { 13.0 } else { 28.666666 };
    let offset_z = if is_wall { 1.0 } else { 8.0 };

    let mut elements = Vec::new();

    // 1. Crossbar element
    elements.push(ElementJson {
        from: [
            -10.0 * scale + offset_x,
            -scale + offset_y,
            -scale + offset_z,
        ],
        to: [
            10.0 * scale + offset_x,
            1.0 * scale + offset_y,
            1.0 * scale + offset_z,
        ],
        rotation: None,
        transform: Some(BuiltinTransform {
            rotate: [0.0, rot_y, 0.0],
            pivot: [8.0, 8.0, 8.0],
        }),
        shade: Some(true),
        faces: make_box_faces(
            "#base",
            [0.5, 10.5, 5.5, 11.0],
            [5.5, 10.5, 10.5, 11.0],
            [6.0, 11.0, 11.0, 11.5],
            [0.5, 11.0, 5.5, 11.5],
            [5.5, 11.0, 6.0, 11.5],
            [0.0, 11.0, 0.5, 11.5],
        ),
    });

    // 2. Post element (standing banner only)
    if !is_wall {
        elements.push(ElementJson {
            from: [
                -scale + offset_x,
                -43.0 * scale + offset_y,
                -scale + offset_z,
            ],
            to: [
                1.0 * scale + offset_x,
                -scale + offset_y,
                1.0 * scale + offset_z,
            ],
            rotation: None,
            transform: Some(BuiltinTransform {
                rotate: [0.0, rot_y, 0.0],
                pivot: [8.0, 8.0, 8.0],
            }),
            shade: Some(true),
            faces: make_box_faces(
                "#base",
                [11.5, 0.0, 12.0, 0.5],
                [12.0, 0.0, 12.5, 0.5],
                [12.5, 0.5, 13.0, 11.0],
                [11.5, 0.5, 12.0, 11.0],
                [12.0, 0.5, 12.5, 11.0],
                [11.0, 0.5, 11.5, 11.0],
            ),
        });
    }

    // 3. Banner cloth element
    elements.push(ElementJson {
        from: [
            -10.0 * scale + offset_x,
            -39.0 * scale + offset_y,
            1.0 * scale + offset_z,
        ],
        to: [
            10.0 * scale + offset_x,
            1.0 * scale + offset_y,
            2.0 * scale + offset_z,
        ],
        rotation: None,
        transform: Some(BuiltinTransform {
            rotate: [0.0, rot_y, 0.0],
            pivot: [8.0, 8.0, 8.0],
        }),
        shade: Some(true),
        faces: make_box_faces(
            "#cloth",
            [0.25, 0.0, 5.25, 0.25],
            [5.25, 0.0, 10.25, 0.25],
            [5.5, 0.25, 10.5, 10.25],
            [0.25, 0.25, 5.25, 10.25],
            [5.25, 0.25, 5.5, 10.25],
            [0.0, 0.25, 0.25, 10.25],
        ),
    });

    Some(BlockModelJson {
        parent: None,
        ambientocclusion: Some(true),
        textures: Some(textures),
        elements: Some(elements),
    })
}
