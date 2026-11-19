//! Structural checks for the native chest generator.
//!
//! These assert the vanilla contract (`ChestModel` / `ChestRenderer`) directly, so they stay valid
//! after the MiEx definitions are deleted.

use mtk_model::{BlockState, BuiltinModelRegistry};

fn model(state: &str) -> mtk_model::BlockModelJson {
    let parsed = BlockState::parse(state).expect("state parses");
    BuiltinModelRegistry::get_block_entity_model_json(&parsed, None)
        .unwrap_or_else(|| panic!("{state} must produce a builtin model"))
}

#[test]
fn single_chest_has_three_boxes_and_four_facings() {
    for (facing, expected_y) in [
        ("north", 0.0),
        ("east", 90.0),
        ("south", 180.0),
        ("west", 270.0),
    ] {
        let state = format!("minecraft:chest[facing={facing},type=single,waterlogged=false]");
        let model = model(&state);
        let elements = model.elements.as_ref().expect("elements");
        assert_eq!(elements.len(), 3, "{state}: bottom + lid + lock");
        for element in elements {
            let transform = element.transform.as_ref().expect("transform");
            assert_eq!(
                transform.rotate,
                [0.0, expected_y, 0.0],
                "{state}: ChestRenderer rotates by -facing.toYRot()"
            );
            assert_eq!(
                element.faces.len(),
                6,
                "{state}: a single chest hides nothing"
            );
        }
    }
}

/// The shared face of a double chest is suppressed because its texture panel is blank: the west
/// rect `(0..14, 33..43)` of `entity/chest/normal_left.png` is fully transparent.
#[test]
fn double_chest_halves_suppress_the_shared_face() {
    let left = model("minecraft:chest[facing=north,type=left,waterlogged=false]");
    for element in left.elements.as_ref().expect("elements") {
        assert!(
            !element.faces.contains_key("west"),
            "createDoubleBodyLeftLayer passes allOfEnumExcept(WEST)"
        );
        assert_eq!(element.faces.len(), 5);
    }

    let right = model("minecraft:chest[facing=north,type=right,waterlogged=false]");
    for element in right.elements.as_ref().expect("elements") {
        assert!(
            !element.faces.contains_key("east"),
            "createDoubleBodyRightLayer passes allOfEnumExcept(EAST)"
        );
        assert_eq!(element.faces.len(), 5);
    }
}

/// The halves must not be identical: `createDoubleBodyLeftLayer` extends to x=0, the right layer
/// starts at x=1 and carries the latch on the far side.
#[test]
fn double_chest_halves_use_the_matching_layer_geometry() {
    let left = model("minecraft:chest[facing=north,type=left,waterlogged=false]");
    let left_body = &left.elements.as_ref().expect("elements")[0];
    assert_eq!(left_body.from, [0.0, 0.0, 1.0]);
    assert_eq!(left_body.to, [15.0, 10.0, 15.0]);
    let left_lock = &left.elements.as_ref().expect("elements")[2];
    assert_eq!(left_lock.from, [0.0, 7.0, 15.0]);

    let right = model("minecraft:chest[facing=north,type=right,waterlogged=false]");
    let right_body = &right.elements.as_ref().expect("elements")[0];
    assert_eq!(right_body.from, [1.0, 0.0, 1.0]);
    assert_eq!(right_body.to, [16.0, 10.0, 15.0]);
    let right_lock = &right.elements.as_ref().expect("elements")[2];
    assert_eq!(right_lock.from, [15.0, 7.0, 15.0]);
}

#[test]
fn chest_material_selects_the_vanilla_sheet() {
    for (state, expected) in [
        (
            "minecraft:chest[facing=north,type=single,waterlogged=false]",
            "minecraft:entity/chest/normal",
        ),
        (
            "minecraft:trapped_chest[facing=north,type=single,waterlogged=false]",
            "minecraft:entity/chest/trapped",
        ),
        (
            "minecraft:ender_chest[facing=north,type=single,waterlogged=false]",
            "minecraft:entity/chest/ender",
        ),
        (
            "minecraft:chest[facing=north,type=left,waterlogged=false]",
            "minecraft:entity/chest/normal_left",
        ),
        (
            "minecraft:chest[facing=north,type=right,waterlogged=false]",
            "minecraft:entity/chest/normal_right",
        ),
    ] {
        let model = model(state);
        let textures = model.textures.as_ref().expect("textures");
        let texture = match textures.get("texture").expect("texture key") {
            mtk_model::TextureValue::Path(path) => path.clone(),
            other => panic!("unexpected texture value {other:?}"),
        };
        assert_eq!(texture, expected, "{state}");
    }
}

/// The body's top face must sample the lid-area rectangle of the chest sheet, not the bottom one.
///
/// MiEx emitted these two rectangles swapped (its model space is y/z mirrored); the conversion
/// follows `ModelPart.Cube`, which is the authority.
#[test]
fn body_face_uvs_follow_the_vanilla_cube_unwrap() {
    let model = model("minecraft:chest[facing=north,type=single,waterlogged=false]");
    let elements = model.elements.as_ref().expect("elements");
    let body = &elements[0];

    let uv = |face: &str| -> [f32; 4] {
        body.faces
            .get(face)
            .unwrap_or_else(|| panic!("body has a {face} face"))
            .uv
            .expect("uv present")
    };

    // CubeSpec::face_uvs on a 64px sheet, scaled by 16/64.
    // up: (u2, v1) - (u22, v0) = (28, 33) - (42, 19)
    assert_eq!(uv("up"), [7.0, 8.25, 10.5, 4.75]);
    // down: (u1, v0) - (u2, v1) = (14, 19) - (28, 33)
    assert_eq!(uv("down"), [3.5, 4.75, 7.0, 8.25]);
    // north: (u1, v1) - (u2, v2) = (14, 33) - (28, 43)
    assert_eq!(uv("north"), [3.5, 8.25, 7.0, 10.75]);
    // south: (u3, v1) - (u4, v2) = (42, 33) - (56, 43)
    assert_eq!(uv("south"), [10.5, 8.25, 14.0, 10.75]);
}
