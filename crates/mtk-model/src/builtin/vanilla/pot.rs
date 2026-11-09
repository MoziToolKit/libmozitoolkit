use crate::parser::model_json::{BlockModelJson, TextureValue};
use crate::{BakedModel, ModelBaker};

pub const MODEL_DECORATED_POT: &str =
    include_str!("../../../assets/builtins/models/decorated_pot.json");

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
