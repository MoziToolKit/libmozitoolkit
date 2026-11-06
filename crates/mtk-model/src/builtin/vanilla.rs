//! Embedded vanilla fallback blockstates and JSON models strictly for blocks lacking
//! static JSON models in vanilla (bell, decorated pot, end portal) or bridging
//! model ID lookups to dynamic MiEx AST evaluations.

use crate::builtin::loader::MiExModelLoader;
use crate::builtin::patch::apply_bell_patches;
use crate::parser::blockstate::{BlockState, BlockStateDefinition};
use crate::parser::model_json::{
    BlockModelJson, BuiltinTransform, ElementJson, FaceJson, TextureValue,
};
use crate::{BakedModel, ModelBaker};
use std::collections::HashMap;

// Embedded BlockStates
pub const DEF_DECORATED_POT: &str =
    include_str!("../../assets/builtins/blockstates/decorated_pot.json");
pub const DEF_END_GATEWAY: &str =
    include_str!("../../assets/builtins/blockstates/end_gateway.json");
pub const DEF_CONDUIT: &str = include_str!("../../assets/builtins/blockstates/conduit.json");
pub const DEF_BELL: &str = include_str!("../../assets/builtins/blockstates/bell.json");

// Embedded Models
pub const MODEL_DECORATED_POT: &str =
    include_str!("../../assets/builtins/models/decorated_pot.json");
pub const MODEL_END_PORTAL: &str = include_str!("../../assets/builtins/models/end_portal.json");
pub const MODEL_END_GATEWAY: &str = include_str!("../../assets/builtins/models/end_gateway.json");
pub const MODEL_CONDUIT: &str = include_str!("../../assets/builtins/models/conduit.json");

/// Retrieves an embedded builtin `BlockStateDefinition` by block name.
pub fn get_builtin_blockstate_def(name: &str) -> Option<BlockStateDefinition> {
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);

    let raw_json = match clean {
        "bell" => DEF_BELL,
        "decorated_pot" => DEF_DECORATED_POT,
        "end_gateway" => DEF_END_GATEWAY,
        "conduit" => DEF_CONDUIT,
        _ => return None,
    };

    serde_json::from_str(raw_json).ok()
}

/// Helper to create UV-explicit faces for a cuboid element with given texture variable and UVs.
fn make_box_faces(
    tex: &str,
    up: [f32; 4],
    down: [f32; 4],
    north: [f32; 4],
    south: [f32; 4],
    east: [f32; 4],
    west: [f32; 4],
) -> HashMap<String, FaceJson> {
    let mut map = HashMap::new();
    let entries = [
        ("up", up),
        ("down", down),
        ("north", north),
        ("south", south),
        ("east", east),
        ("west", west),
    ];
    for (dir, uv) in entries {
        map.insert(
            dir.to_string(),
            FaceJson {
                uv: Some(uv),
                texture: tex.to_string(),
                cullface: None,
                rotation: None,
                tintindex: None,
            },
        );
    }
    map
}

/// Helper to create simple faces for a cuboid element with given texture variable.
fn make_simple_faces(tex: &str) -> HashMap<String, FaceJson> {
    let mut map = HashMap::new();
    for dir in &["north", "south", "east", "west", "up", "down"] {
        map.insert(
            dir.to_string(),
            FaceJson {
                uv: None,
                texture: tex.to_string(),
                cullface: None,
                rotation: None,
                tintindex: None,
            },
        );
    }
    map
}

/// Synthesizes vanilla bell frame models and applies bell body patches.
fn create_builtin_bell_model(stem: &str) -> Option<BlockModelJson> {
    let mut textures = HashMap::new();
    textures.insert(
        "particle".to_string(),
        TextureValue::Path("minecraft:block/bell_bottom".to_string()),
    );
    textures.insert(
        "bar".to_string(),
        TextureValue::Path("minecraft:block/dark_oak_planks".to_string()),
    );
    textures.insert(
        "post".to_string(),
        TextureValue::Path("minecraft:block/stone".to_string()),
    );

    let mut elements = Vec::new();

    match stem {
        "bell_floor" => {
            elements.push(ElementJson {
                from: [2.0, 13.0, 7.0],
                to: [14.0, 15.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
            elements.push(ElementJson {
                from: [14.0, 0.0, 6.0],
                to: [16.0, 16.0, 10.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#post"),
            });
            elements.push(ElementJson {
                from: [0.0, 0.0, 6.0],
                to: [2.0, 16.0, 10.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#post"),
            });
        }
        "bell_ceiling" => {
            elements.push(ElementJson {
                from: [7.0, 13.0, 7.0],
                to: [9.0, 16.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
        }
        "bell_wall" => {
            elements.push(ElementJson {
                from: [3.0, 13.0, 7.0],
                to: [16.0, 15.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
        }
        "bell_between_walls" => {
            elements.push(ElementJson {
                from: [0.0, 13.0, 7.0],
                to: [16.0, 15.0, 9.0],
                rotation: None,
                transform: None,
                shade: Some(true),
                faces: make_simple_faces("#bar"),
            });
        }
        _ => return None,
    }

    let mut model = BlockModelJson {
        parent: None,
        ambientocclusion: Some(true),
        textures: Some(textures),
        elements: Some(elements),
    };

    apply_bell_patches("block/bell_", &mut model);
    Some(model)
}

/// Retrieves an embedded builtin `BlockModelJson` by model resource path / ID.
pub fn get_builtin_model_by_id(model_id: &str) -> Option<BlockModelJson> {
    let clean = model_id.strip_prefix("minecraft:").unwrap_or(model_id);
    let stem = clean.strip_prefix("block/").unwrap_or(clean);

    if stem.starts_with("bell_") {
        return create_builtin_bell_model(stem);
    }

    if stem == "decorated_pot" {
        return create_builtin_decorated_pot_model(None);
    }
    if stem == "end_portal" {
        return serde_json::from_str(MODEL_END_PORTAL).ok();
    }
    if stem == "end_gateway" {
        return serde_json::from_str(MODEL_END_GATEWAY).ok();
    }
    if stem == "conduit" {
        return serde_json::from_str(MODEL_CONDUIT).ok();
    }

    // Map entity block model IDs to canonical BlockStates evaluated by MiExModelLoader
    let canonical_state = match stem {
        "chest" => "minecraft:chest[facing=north,type=single]",
        "chest_left" => "minecraft:chest[facing=north,type=left]",
        "chest_right" => "minecraft:chest[facing=north,type=right]",
        "trapped_chest" => "minecraft:trapped_chest[facing=north,type=single]",
        "trapped_chest_left" => "minecraft:trapped_chest[facing=north,type=left]",
        "trapped_chest_right" => "minecraft:trapped_chest[facing=north,type=right]",
        "ender_chest" => "minecraft:ender_chest[facing=north]",
        "shulker_box" => "minecraft:shulker_box[facing=up]",
        "skull" => "minecraft:skeleton_skull[rotation=0]",
        "skull_wall" => "minecraft:skeleton_wall_skull[facing=north]",
        "dragon_head" => "minecraft:dragon_head[rotation=0]",
        "piglin_head" => "minecraft:piglin_head[rotation=0]",
        _ => return None,
    };

    let bs = BlockState::parse(canonical_state).ok()?;
    MiExModelLoader::load_for_blockstate(&bs)
}

/// Procedurally synthesizes a banner model (standing or wall) with pole, crossbar, and cloth.
fn create_builtin_banner_model(
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

static DECORATED_POT_TEMPLATE: std::sync::OnceLock<BlockModelJson> = std::sync::OnceLock::new();
static DECORATED_POT_SKELETONS: std::sync::OnceLock<[BakedModel; 4]> = std::sync::OnceLock::new();

#[inline]
fn get_decorated_pot_template() -> &'static BlockModelJson {
    DECORATED_POT_TEMPLATE.get_or_init(|| {
        serde_json::from_str(MODEL_DECORATED_POT)
            .expect("Builtin decorated pot model template must be valid JSON")
    })
}

/// Maps a horizontal face direction to its corresponding Pottery Sherd slot (0..3)
/// given the block's horizontal facing.
///
/// Slots: 0 = Back, 1 = Left, 2 = Right, 3 = Front
#[inline]
pub fn decorated_pot_sherd_slot(
    facing: mtk_core::Direction,
    face_dir: mtk_core::Direction,
) -> Option<usize> {
    use mtk_core::Direction;
    match facing {
        Direction::North => match face_dir {
            Direction::North => Some(0), // back
            Direction::West => Some(1),  // left
            Direction::East => Some(2),  // right
            Direction::South => Some(3), // front
            _ => None,
        },
        Direction::South => match face_dir {
            Direction::South => Some(0), // back
            Direction::East => Some(1),  // left
            Direction::West => Some(2),  // right
            Direction::North => Some(3), // front
            _ => None,
        },
        Direction::East => match face_dir {
            Direction::East => Some(0),  // back
            Direction::North => Some(1), // left
            Direction::South => Some(2), // right
            Direction::West => Some(3),  // front
            _ => None,
        },
        Direction::West => match face_dir {
            Direction::West => Some(0),  // back
            Direction::South => Some(1), // left
            Direction::North => Some(2), // right
            Direction::East => Some(3),  // front
            _ => None,
        },
        _ => None,
    }
}

/// Retrieves the pre-baked base geometric skeleton for a decorated pot under a horizontal facing.
pub fn get_baked_decorated_pot_skeleton(facing: mtk_core::Direction) -> &'static BakedModel {
    use mtk_core::Direction;
    let skeletons = DECORATED_POT_SKELETONS.get_or_init(|| {
        let mut baker = ModelBaker::new();
        let north = baker
            .bake_blockstate("minecraft:decorated_pot[facing=north]", None, |_| None)
            .expect("Decorated pot north must bake");
        let south = baker
            .bake_blockstate("minecraft:decorated_pot[facing=south]", None, |_| None)
            .expect("Decorated pot south must bake");
        let east = baker
            .bake_blockstate("minecraft:decorated_pot[facing=east]", None, |_| None)
            .expect("Decorated pot east must bake");
        let west = baker
            .bake_blockstate("minecraft:decorated_pot[facing=west]", None, |_| None)
            .expect("Decorated pot west must bake");
        [north, south, east, west]
    });

    match facing {
        Direction::North => &skeletons[0],
        Direction::South => &skeletons[1],
        Direction::East => &skeletons[2],
        Direction::West => &skeletons[3],
        _ => &skeletons[0],
    }
}

/// Assembles a complete BakedModel for a decorated pot by cloning the pre-baked geometric skeleton
/// and patching only the 4 side faces with their configured sherd textures.
///
/// Bypasses all runtime JSON parsing and 3D rotation calculations.
pub fn create_decorated_pot_baked_model(
    facing: mtk_core::Direction,
    sherds: &[Option<String>; 4],
) -> BakedModel {
    let skeleton = get_baked_decorated_pot_skeleton(facing);
    let mut model = skeleton.clone();
    for face in &mut model.unculled_faces {
        if face.texture.contains("decorated_pot_side") {
            if let Some(slot) = decorated_pot_sherd_slot(facing, face.direction) {
                let pattern = sherds[slot].as_deref();
                face.texture = crate::entity::BlockEntityData::sherd_texture(pattern);
            }
        }
    }
    for el in &mut model.elements {
        for (dir, face) in &mut el.faces {
            if face.texture.contains("decorated_pot_side") {
                if let Some(slot) = decorated_pot_sherd_slot(facing, *dir) {
                    let pattern = sherds[slot].as_deref();
                    face.texture = crate::entity::BlockEntityData::sherd_texture(pattern);
                }
            }
        }
    }
    model
}

/// Constructs a decorated pot model, optionally overriding the 4 sides with sherd patterns.
pub fn create_builtin_decorated_pot_model(
    entity: Option<&crate::entity::BlockEntityData>,
) -> Option<BlockModelJson> {
    let mut model = get_decorated_pot_template().clone();
    if let Some(crate::entity::BlockEntityData::DecoratedPot { sherds }) = entity {
        if let Some(ref mut textures) = model.textures {
            // [back, left, right, front]
            let back_tex = crate::entity::BlockEntityData::sherd_texture_ref(sherds[0].as_deref());
            let left_tex = crate::entity::BlockEntityData::sherd_texture_ref(sherds[1].as_deref());
            let right_tex = crate::entity::BlockEntityData::sherd_texture_ref(sherds[2].as_deref());
            let front_tex = crate::entity::BlockEntityData::sherd_texture_ref(sherds[3].as_deref());
            textures.insert("back".to_string(), TextureValue::Path(back_tex.to_string()));
            textures.insert("left".to_string(), TextureValue::Path(left_tex.to_string()));
            textures.insert(
                "right".to_string(),
                TextureValue::Path(right_tex.to_string()),
            );
            textures.insert(
                "front".to_string(),
                TextureValue::Path(front_tex.to_string()),
            );
        }
    }
    Some(model)
}

/// Fallback model for a blockstate and optional block entity data.
pub fn get_builtin_model_for_state_and_entity(
    blockstate: &BlockState,
    entity: Option<&crate::entity::BlockEntityData>,
) -> Option<BlockModelJson> {
    let clean = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);

    if clean == "decorated_pot" {
        return create_builtin_decorated_pot_model(entity);
    }
    if clean == "end_gateway" {
        return get_builtin_model_by_id("end_gateway");
    }
    if clean == "conduit" {
        return get_builtin_model_by_id("conduit");
    }
    if clean == "bell" {
        let att = blockstate
            .properties
            .get("attachment")
            .map(|s| s.as_str())
            .unwrap_or("floor");
        let model_id = match att {
            "ceiling" => "bell_ceiling",
            "single_wall" | "wall" => "bell_wall",
            "double_wall" => "bell_between_walls",
            _ => "bell_floor",
        };
        return get_builtin_model_by_id(model_id);
    }
    if clean.ends_with("_banner") {
        return create_builtin_banner_model(blockstate, entity);
    }
    None
}

/// Pre-baked direct model synthesizer for entity blocks (e.g. Decorated Pot).
pub fn get_builtin_baked_model_for_state_and_entity(
    blockstate: &BlockState,
    entity: Option<&crate::entity::BlockEntityData>,
) -> Option<BakedModel> {
    let clean = blockstate
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&blockstate.name);

    if clean.contains("copper_golem_statue") {
        return Some(crate::entity::copper_statue::create_copper_statue_baked_model(blockstate));
    }

    if clean == "decorated_pot" {
        if let Some(crate::entity::BlockEntityData::DecoratedPot { sherds }) = entity {
            let facing = blockstate
                .properties
                .get("facing")
                .and_then(|f| mtk_core::Direction::parse_loose(f))
                .unwrap_or(mtk_core::Direction::North);

            return Some(create_decorated_pot_baked_model(facing, sherds));
        }
    }
    None
}
