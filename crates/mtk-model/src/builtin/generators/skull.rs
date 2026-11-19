//! Skull and head geometry.
//!
//! Source of truth: `SkullBlockRenderer` plus the per-kind head models (`HumanoidHeadModel`,
//! `DragonHeadModel`, `PiglinHeadModel`, ...) and `SkullBlockRenderer.getRenderType` for the sheet.
//!
//! The tables are the geometry that shipped before this generator, captured once per (kind, wall)
//! pair: eight kinds collapse to five layouts (plain cube, zombie/player two-row cube, player head
//! with hat overlay, dragon with an articulated jaw, piglin with a snout and ears).

use std::collections::HashMap;

use crate::block_entity::skull::SkullKind;
use crate::block_entity::{BlockEntityData, PlayerProfileData};
use crate::parser::blockstate::BlockState;
use crate::parser::model_json::{
    BlockModelJson, BuiltinTransform, ElementJson, FaceJson, RotationJson, RotationValue,
    TextureValue,
};

/// One box and its six face rectangles in 0..16 UV space, ordered
/// `down, up, north, south, east, west`.
pub(super) struct SkullBox {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<([f32; 3], &'static str, f32)>,
    pub uvs: [[f32; 4]; 6],
}

/// Face keys in [`SkullBox::uvs`] order.
pub(super) const FACE_KEYS: [&str; 6] = ["down", "up", "north", "south", "east", "west"];

const SKELETON_FLOOR: [SkullBox; 1] = [SkullBox {
    from: [4.0, 0.0, 4.0],
    to: [12.0, 8.0, 12.0],
    rotation: None,
    uvs: [
        [4.0, 0.0, 6.0, 4.0],
        [2.0, 0.0, 4.0, 4.0],
        [6.0, 4.0, 8.0, 8.0],
        [2.0, 4.0, 4.0, 8.0],
        [4.0, 4.0, 6.0, 8.0],
        [0.0, 4.0, 2.0, 8.0],
    ],
}];

const SKELETON_WALL: [SkullBox; 1] = [SkullBox {
    from: [4.0, 4.0, 0.0],
    to: [12.0, 12.0, 8.0],
    rotation: None,
    uvs: [
        [4.0, 0.0, 6.0, 4.0],
        [2.0, 0.0, 4.0, 4.0],
        [6.0, 4.0, 8.0, 8.0],
        [2.0, 4.0, 4.0, 8.0],
        [4.0, 4.0, 6.0, 8.0],
        [0.0, 4.0, 2.0, 8.0],
    ],
}];

const ZOMBIE_FLOOR: [SkullBox; 1] = [SkullBox {
    from: [4.0, 0.0, 4.0],
    to: [12.0, 8.0, 12.0],
    rotation: None,
    uvs: [
        [4.0, 0.0, 6.0, 2.0],
        [2.0, 0.0, 4.0, 2.0],
        [6.0, 2.0, 8.0, 4.0],
        [2.0, 2.0, 4.0, 4.0],
        [4.0, 2.0, 6.0, 4.0],
        [0.0, 2.0, 2.0, 4.0],
    ],
}];

const ZOMBIE_WALL: [SkullBox; 1] = [SkullBox {
    from: [4.0, 4.0, 0.0],
    to: [12.0, 12.0, 8.0],
    rotation: None,
    uvs: [
        [4.0, 0.0, 6.0, 2.0],
        [2.0, 0.0, 4.0, 2.0],
        [6.0, 2.0, 8.0, 4.0],
        [2.0, 2.0, 4.0, 4.0],
        [4.0, 2.0, 6.0, 4.0],
        [0.0, 2.0, 2.0, 4.0],
    ],
}];

const DRAGON_FLOOR: [SkullBox; 7] = [
    SkullBox {
        from: [2.0, 0.0, 3.5],
        to: [14.0, 12.0, 15.5],
        rotation: None,
        uvs: [
            [9.0, 1.875, 10.0, 2.875],
            [8.0, 1.875, 9.0, 2.875],
            [10.0, 2.875, 11.0, 3.875],
            [8.0, 2.875, 9.0, 3.875],
            [9.0, 2.875, 10.0, 3.875],
            [7.0, 2.875, 8.0, 3.875],
        ],
    },
    SkullBox {
        from: [3.5, 3.0, 14.0],
        to: [12.5, 6.75, 26.0],
        rotation: None,
        uvs: [
            [12.75, 2.75, 13.5, 3.75],
            [12.0, 2.75, 12.75, 3.75],
            [13.75, 3.75, 14.5, 4.0625],
            [12.0, 3.75, 12.75, 4.0625],
            [12.75, 3.75, 13.75, 4.0625],
            [11.0, 3.75, 12.0, 4.0625],
        ],
    },
    SkullBox {
        from: [3.5, 0.0, 14.0],
        to: [12.5, 3.0, 26.0],
        rotation: Some(([8.0, 3.0, 14.0], "x", 11.5)),
        uvs: [
            [12.75, 4.0625, 13.5, 5.0625],
            [12.0, 4.0625, 12.75, 5.0625],
            [13.75, 5.0625, 14.5, 5.3125],
            [12.0, 5.0625, 12.75, 5.3125],
            [12.75, 5.0625, 13.75, 5.3125],
            [11.0, 5.0625, 12.0, 5.3125],
        ],
    },
    SkullBox {
        from: [10.25, 6.75, 23.0],
        to: [11.75, 8.25, 26.0],
        rotation: None,
        uvs: [
            [7.375, 0.0, 7.5, 0.25],
            [7.25, 0.0, 7.375, 0.25],
            [7.625, 0.25, 7.75, 0.375],
            [7.25, 0.25, 7.375, 0.375],
            [7.375, 0.25, 7.625, 0.375],
            [7.0, 0.25, 7.25, 0.375],
        ],
    },
    SkullBox {
        from: [4.25, 6.75, 23.0],
        to: [5.75, 8.25, 26.0],
        rotation: None,
        uvs: [
            [7.375, 0.0, 7.5, 0.25],
            [7.25, 0.0, 7.375, 0.25],
            [7.625, 0.25, 7.75, 0.375],
            [7.25, 0.25, 7.375, 0.375],
            [7.375, 0.25, 7.625, 0.375],
            [7.0, 0.25, 7.25, 0.375],
        ],
    },
    SkullBox {
        from: [10.25, 12.0, 6.5],
        to: [11.75, 15.0, 11.0],
        rotation: None,
        uvs: [
            [0.5, 0.0, 0.625, 0.375],
            [0.375, 0.0, 0.5, 0.375],
            [0.875, 0.375, 1.0, 0.625],
            [0.375, 0.375, 0.5, 0.625],
            [0.5, 0.375, 0.875, 0.625],
            [0.0, 0.375, 0.375, 0.625],
        ],
    },
    SkullBox {
        from: [4.25, 12.0, 6.5],
        to: [5.75, 15.0, 11.0],
        rotation: None,
        uvs: [
            [3.5, 0.0, 3.625, 0.375],
            [3.375, 0.0, 3.5, 0.375],
            [3.875, 0.375, 4.0, 0.625],
            [3.375, 0.375, 3.5, 0.625],
            [3.5, 0.375, 3.875, 0.625],
            [3.0, 0.375, 3.375, 0.625],
        ],
    },
];

const DRAGON_WALL: [SkullBox; 7] = [
    SkullBox {
        from: [2.0, 4.0, 0.0],
        to: [14.0, 16.0, 12.0],
        rotation: None,
        uvs: [
            [9.0, 1.875, 10.0, 2.875],
            [8.0, 1.875, 9.0, 2.875],
            [10.0, 2.875, 11.0, 3.875],
            [8.0, 2.875, 9.0, 3.875],
            [9.0, 2.875, 10.0, 3.875],
            [7.0, 2.875, 8.0, 3.875],
        ],
    },
    SkullBox {
        from: [3.5, 7.0, 10.5],
        to: [12.5, 10.75, 22.5],
        rotation: None,
        uvs: [
            [12.75, 2.75, 13.5, 3.75],
            [12.0, 2.75, 12.75, 3.75],
            [13.75, 3.75, 14.5, 4.0625],
            [12.0, 3.75, 12.75, 4.0625],
            [12.75, 3.75, 13.75, 4.0625],
            [11.0, 3.75, 12.0, 4.0625],
        ],
    },
    SkullBox {
        from: [3.5, 4.0, 10.5],
        to: [12.5, 7.0, 22.5],
        rotation: Some(([8.0, 7.0, 10.5], "x", 11.5)),
        uvs: [
            [12.75, 4.0625, 13.5, 5.0625],
            [12.0, 4.0625, 12.75, 5.0625],
            [13.75, 5.0625, 14.5, 5.3125],
            [12.0, 5.0625, 12.75, 5.3125],
            [12.75, 5.0625, 13.75, 5.3125],
            [11.0, 5.0625, 12.0, 5.3125],
        ],
    },
    SkullBox {
        from: [10.25, 10.75, 19.5],
        to: [11.75, 12.25, 22.5],
        rotation: None,
        uvs: [
            [7.375, 0.0, 7.5, 0.25],
            [7.25, 0.0, 7.375, 0.25],
            [7.625, 0.25, 7.75, 0.375],
            [7.25, 0.25, 7.375, 0.375],
            [7.375, 0.25, 7.625, 0.375],
            [7.0, 0.25, 7.25, 0.375],
        ],
    },
    SkullBox {
        from: [4.25, 10.75, 19.5],
        to: [5.75, 12.25, 22.5],
        rotation: None,
        uvs: [
            [7.375, 0.0, 7.5, 0.25],
            [7.25, 0.0, 7.375, 0.25],
            [7.625, 0.25, 7.75, 0.375],
            [7.25, 0.25, 7.375, 0.375],
            [7.375, 0.25, 7.625, 0.375],
            [7.0, 0.25, 7.25, 0.375],
        ],
    },
    SkullBox {
        from: [10.25, 16.0, 3.0],
        to: [11.75, 19.0, 7.5],
        rotation: None,
        uvs: [
            [0.5, 0.0, 0.625, 0.375],
            [0.375, 0.0, 0.5, 0.375],
            [0.875, 0.375, 1.0, 0.625],
            [0.375, 0.375, 0.5, 0.625],
            [0.5, 0.375, 0.875, 0.625],
            [0.0, 0.375, 0.375, 0.625],
        ],
    },
    SkullBox {
        from: [4.25, 16.0, 3.0],
        to: [5.75, 19.0, 7.5],
        rotation: None,
        uvs: [
            [3.5, 0.0, 3.625, 0.375],
            [3.375, 0.0, 3.5, 0.375],
            [3.875, 0.375, 4.0, 0.625],
            [3.375, 0.375, 3.5, 0.625],
            [3.5, 0.375, 3.875, 0.625],
            [3.0, 0.375, 3.375, 0.625],
        ],
    },
];

const PIGLIN_FLOOR: [SkullBox; 6] = [
    SkullBox {
        from: [3.0, 0.0, 4.0],
        to: [13.0, 8.0, 12.0],
        rotation: None,
        uvs: [
            [4.5, 0.0, 7.0, 2.0],
            [2.0, 0.0, 4.5, 2.0],
            [6.5, 2.0, 9.0, 4.0],
            [2.0, 2.0, 4.5, 4.0],
            [4.5, 2.0, 6.5, 4.0],
            [0.0, 2.0, 2.0, 4.0],
        ],
    },
    SkullBox {
        from: [6.0, 0.0, 12.0],
        to: [10.0, 4.0, 13.0],
        rotation: None,
        uvs: [
            [9.0, 0.25, 10.0, 0.5],
            [8.0, 0.25, 9.0, 0.5],
            [9.25, 0.5, 10.25, 1.5],
            [8.0, 0.5, 9.0, 1.5],
            [9.0, 0.5, 9.25, 1.5],
            [7.75, 0.5, 8.0, 1.5],
        ],
    },
    SkullBox {
        from: [12.5, 1.0, 6.0],
        to: [13.5, 6.0, 10.0],
        rotation: Some(([12.5, 6.0, 8.0], "z", 30.0)),
        uvs: [
            [11.0, 1.5, 11.25, 2.5],
            [10.75, 1.5, 11.0, 2.5],
            [12.0, 2.5, 12.25, 3.75],
            [10.75, 2.5, 11.0, 3.75],
            [11.0, 2.5, 12.0, 3.75],
            [9.75, 2.5, 10.75, 3.75],
        ],
    },
    SkullBox {
        from: [2.5, 1.0, 6.0],
        to: [3.5, 6.0, 10.0],
        rotation: Some(([3.5, 6.0, 8.0], "z", -30.0)),
        uvs: [
            [14.0, 1.5, 14.25, 2.5],
            [13.75, 1.5, 14.0, 2.5],
            [15.0, 2.5, 15.25, 3.75],
            [13.75, 2.5, 14.0, 3.75],
            [14.0, 2.5, 15.0, 3.75],
            [12.75, 2.5, 13.75, 3.75],
        ],
    },
    SkullBox {
        from: [10.0, 0.0, 12.0],
        to: [11.0, 2.0, 13.0],
        rotation: None,
        uvs: [
            [1.0, 0.0, 1.25, 0.25],
            [0.75, 0.0, 1.0, 0.25],
            [1.25, 0.25, 1.5, 0.75],
            [0.75, 0.25, 1.0, 0.75],
            [1.0, 0.25, 1.25, 0.75],
            [0.5, 0.25, 0.75, 0.75],
        ],
    },
    SkullBox {
        from: [5.0, 0.0, 12.0],
        to: [6.0, 2.0, 13.0],
        rotation: None,
        uvs: [
            [1.0, 1.0, 1.25, 1.25],
            [0.75, 1.0, 1.0, 1.25],
            [1.25, 1.25, 1.5, 1.75],
            [0.75, 1.25, 1.0, 1.75],
            [1.0, 1.25, 1.25, 1.75],
            [0.5, 1.25, 0.75, 1.75],
        ],
    },
];

const PIGLIN_WALL: [SkullBox; 6] = [
    SkullBox {
        from: [3.0, 4.0, 0.0],
        to: [13.0, 12.0, 8.0],
        rotation: None,
        uvs: [
            [4.5, 0.0, 7.0, 2.0],
            [2.0, 0.0, 4.5, 2.0],
            [6.5, 2.0, 9.0, 4.0],
            [2.0, 2.0, 4.5, 4.0],
            [4.5, 2.0, 6.5, 4.0],
            [0.0, 2.0, 2.0, 4.0],
        ],
    },
    SkullBox {
        from: [6.0, 4.0, 8.0],
        to: [10.0, 8.0, 9.0],
        rotation: None,
        uvs: [
            [9.0, 0.25, 10.0, 0.5],
            [8.0, 0.25, 9.0, 0.5],
            [9.25, 0.5, 10.25, 1.5],
            [8.0, 0.5, 9.0, 1.5],
            [9.0, 0.5, 9.25, 1.5],
            [7.75, 0.5, 8.0, 1.5],
        ],
    },
    SkullBox {
        from: [12.5, 5.0, 2.0],
        to: [13.5, 10.0, 6.0],
        rotation: Some(([12.5, 10.0, 4.0], "z", 30.0)),
        uvs: [
            [11.0, 1.5, 11.25, 2.5],
            [10.75, 1.5, 11.0, 2.5],
            [12.0, 2.5, 12.25, 3.75],
            [10.75, 2.5, 11.0, 3.75],
            [11.0, 2.5, 12.0, 3.75],
            [9.75, 2.5, 10.75, 3.75],
        ],
    },
    SkullBox {
        from: [2.5, 5.0, 2.0],
        to: [3.5, 10.0, 6.0],
        rotation: Some(([3.5, 10.0, 4.0], "z", -30.0)),
        uvs: [
            [14.0, 1.5, 14.25, 2.5],
            [13.75, 1.5, 14.0, 2.5],
            [15.0, 2.5, 15.25, 3.75],
            [13.75, 2.5, 14.0, 3.75],
            [14.0, 2.5, 15.0, 3.75],
            [12.75, 2.5, 13.75, 3.75],
        ],
    },
    SkullBox {
        from: [10.0, 4.0, 8.0],
        to: [11.0, 6.0, 9.0],
        rotation: None,
        uvs: [
            [1.0, 0.0, 1.25, 0.25],
            [0.75, 0.0, 1.0, 0.25],
            [1.25, 0.25, 1.5, 0.75],
            [0.75, 0.25, 1.0, 0.75],
            [1.0, 0.25, 1.25, 0.75],
            [0.5, 0.25, 0.75, 0.75],
        ],
    },
    SkullBox {
        from: [5.0, 4.0, 8.0],
        to: [6.0, 6.0, 9.0],
        rotation: None,
        uvs: [
            [1.0, 1.0, 1.25, 1.25],
            [0.75, 1.0, 1.0, 1.25],
            [1.25, 1.25, 1.5, 1.75],
            [0.75, 1.25, 1.0, 1.75],
            [1.0, 1.25, 1.25, 1.75],
            [0.5, 1.25, 0.75, 1.75],
        ],
    },
];

const PLAYER_FLOOR: [SkullBox; 2] = [
    SkullBox {
        from: [4.0, 0.0, 4.0],
        to: [12.0, 8.0, 12.0],
        rotation: None,
        uvs: [
            [4.0, 0.0, 6.0, 2.0],
            [2.0, 0.0, 4.0, 2.0],
            [6.0, 2.0, 8.0, 4.0],
            [2.0, 2.0, 4.0, 4.0],
            [4.0, 2.0, 6.0, 4.0],
            [0.0, 2.0, 2.0, 4.0],
        ],
    },
    SkullBox {
        from: [3.75, -0.25, 3.75],
        to: [12.25, 8.25, 12.25],
        rotation: None,
        uvs: [
            [12.0, 0.0, 14.0, 2.0],
            [10.0, 0.0, 12.0, 2.0],
            [14.0, 2.0, 16.0, 4.0],
            [10.0, 2.0, 12.0, 4.0],
            [12.0, 2.0, 14.0, 4.0],
            [8.0, 2.0, 10.0, 4.0],
        ],
    },
];

const PLAYER_WALL: [SkullBox; 2] = [
    SkullBox {
        from: [4.0, 4.0, 0.0],
        to: [12.0, 12.0, 8.0],
        rotation: None,
        uvs: [
            [4.0, 0.0, 6.0, 2.0],
            [2.0, 0.0, 4.0, 2.0],
            [6.0, 2.0, 8.0, 4.0],
            [2.0, 2.0, 4.0, 4.0],
            [4.0, 2.0, 6.0, 4.0],
            [0.0, 2.0, 2.0, 4.0],
        ],
    },
    SkullBox {
        from: [3.75, 3.75, -0.25],
        to: [12.25, 12.25, 8.25],
        rotation: None,
        uvs: [
            [12.0, 0.0, 14.0, 2.0],
            [10.0, 0.0, 12.0, 2.0],
            [14.0, 2.0, 16.0, 4.0],
            [10.0, 2.0, 12.0, 4.0],
            [12.0, 2.0, 14.0, 4.0],
            [8.0, 2.0, 10.0, 4.0],
        ],
    },
];

/// Picks the box table for `kind` and orientation.
fn boxes(kind: SkullKind, wall: bool) -> &'static [SkullBox] {
    match (kind, wall) {
        (SkullKind::Skeleton | SkullKind::WitherSkeleton | SkullKind::Creeper, false) => {
            &SKELETON_FLOOR
        }
        (SkullKind::Skeleton | SkullKind::WitherSkeleton | SkullKind::Creeper, true) => {
            &SKELETON_WALL
        }
        (SkullKind::Zombie, false) => &ZOMBIE_FLOOR,
        (SkullKind::Zombie, true) => &ZOMBIE_WALL,
        (SkullKind::Dragon, false) => &DRAGON_FLOOR,
        (SkullKind::Dragon, true) => &DRAGON_WALL,
        (SkullKind::Piglin, false) => &PIGLIN_FLOOR,
        (SkullKind::Piglin, true) => &PIGLIN_WALL,
        (SkullKind::Player, false) => &PLAYER_FLOOR,
        (SkullKind::Player, true) => &PLAYER_WALL,
    }
}

/// `SkullBlockRenderer` rotates the head by the block's `rotation` (22.5 degrees per step, offset
/// by 180) or by the wall facing.
fn rotation_degrees(blockstate: &BlockState) -> f32 {
    let is_wall = blockstate.name.contains("_wall_");
    if is_wall {
        match blockstate
            .properties
            .get("facing")
            .map(String::as_str)
            .unwrap_or("north")
        {
            "east" => 270.0,
            "south" => 0.0,
            "west" => 90.0,
            _ => 180.0,
        }
    } else {
        let step = blockstate
            .properties
            .get("rotation")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let angle = 22.5 * step + 180.0;
        if angle >= 360.0 {
            angle - 360.0
        } else {
            angle
        }
    }
}

/// Sheet for the head: the player's profile texture when one is available, otherwise the sheet the
/// kind defaults to (`SkullBlockRenderer.getRenderType`).
fn skull_sheet(
    kind: SkullKind,
    blockstate: &BlockState,
    entity: Option<&BlockEntityData>,
) -> String {
    if let Some(BlockEntityData::Skull {
        profile: Some(profile),
        ..
    }) = entity
    {
        if let Some(texture) = player_texture(profile) {
            return texture;
        }
    }
    let effective = SkullKind::from_block_name(&blockstate.name).unwrap_or(kind);
    effective.default_texture_path().to_string()
}

fn player_texture(profile: &PlayerProfileData) -> Option<String> {
    let texture = profile.texture_identifier();
    (!texture.is_empty()).then_some(texture)
}

/// Builds the skull/head model, or `None` when `blockstate` is not a skull.
pub(crate) fn build_skull_model(
    blockstate: &BlockState,
    entity: Option<&BlockEntityData>,
) -> Option<BlockModelJson> {
    let short = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);
    let kind = SkullKind::from_block_name(short)?;
    let wall = short.contains("_wall_");
    let transform = BuiltinTransform {
        rotate: [0.0, rotation_degrees(blockstate), 0.0],
        pivot: [8.0, 8.0, 8.0],
    };

    let elements = boxes(kind, wall)
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
                rotation: spec.rotation.map(|(origin, axis, angle)| RotationJson {
                    origin,
                    value: RotationValue::SingleAxis {
                        axis: axis.to_string(),
                        angle,
                    },
                    rescale: None,
                }),
                transform: Some(transform.clone()),
                shade: Some(true),
                light_emission: None,
                faces,
            }
        })
        .collect();

    let sheet = skull_sheet(kind, blockstate, entity);
    let mut textures = HashMap::new();
    textures.insert("texture".to_string(), TextureValue::Path(sheet));
    // The definition's default texture stays as the particle for every head.
    textures.insert(
        "particle".to_string(),
        TextureValue::Path("minecraft:entity/player/wide/steve".to_string()),
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
