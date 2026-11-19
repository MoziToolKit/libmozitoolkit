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

/// All six faces are emitted for every layer: the MiEx definitions (and therefore the client's
/// current output) carry the full box, with the shared-side panel supplied by the `_left`/`_right`
/// sheet rather than suppressed.
#[test]
fn every_chest_layer_emits_all_six_faces() {
    for state in [
        "minecraft:chest[facing=north,type=single,waterlogged=false]",
        "minecraft:chest[facing=north,type=left,waterlogged=false]",
        "minecraft:chest[facing=north,type=right,waterlogged=false]",
    ] {
        let model = model(state);
        for element in model.elements.as_ref().expect("elements") {
            assert_eq!(element.faces.len(), 6, "{state}");
        }
    }
}

/// The halves must not be identical: `createDoubleBodyLeftLayer` extends to x=0, the right layer
/// starts at x=1 and carries the latch on the far side.
/// The halves are mirror images: the `_left` sheet pairs with the geometry the client uses for its
/// right half, so the latch sits at high x on the left sheet and low x on the right sheet, and both
/// halves keep their body inside the block.
#[test]
fn double_chest_halves_are_mirrored() {
    let left = model("minecraft:chest[facing=north,type=left,waterlogged=false]");
    let left_elements = left.elements.as_ref().expect("elements");
    assert_eq!(left_elements[0].from, [16.0, 10.0, 1.0]);
    assert_eq!(left_elements[0].to, [1.0, 0.0, 15.0]);
    assert_eq!(left_elements[2].from, [16.0, 11.0, 0.0]);
    assert_eq!(left_elements[2].to, [15.0, 7.0, 1.0]);

    let right = model("minecraft:chest[facing=north,type=right,waterlogged=false]");
    let right_elements = right.elements.as_ref().expect("elements");
    assert_eq!(right_elements[0].from, [15.0, 10.0, 1.0]);
    assert_eq!(right_elements[0].to, [0.0, 0.0, 15.0]);
    assert_eq!(right_elements[2].from, [1.0, 11.0, 0.0]);
    assert_eq!(right_elements[2].to, [0.0, 7.0, 1.0]);
}

/// The body's faces must sample the panels the client actually shows: the y-up conversion swaps
/// the client's up/down and north/south rectangles, which was verified visually.
#[test]
fn body_face_uvs_use_the_y_up_mapping() {
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

    // `CubeSpec::model_face_uvs` on a 64px sheet, scaled by 16/64.
    // up samples the client's down panel: (u1, v0) - (u2, v1) = (14, 19) - (28, 33)
    assert_eq!(uv("up"), [3.5, 4.75, 7.0, 8.25]);
    // down samples the client's up panel: (u2, v1) - (u22, v0) = (28, 33) - (42, 19)
    assert_eq!(uv("down"), [7.0, 4.75, 10.5, 8.25]);
    // north samples the client's south panel: (u3, v1) - (u4, v2) = (42, 33) - (56, 43)
    assert_eq!(uv("north"), [10.5, 8.25, 14.0, 10.75]);
    // south samples the client's north panel: (u1, v1) - (u2, v2) = (14, 33) - (28, 43)
    assert_eq!(uv("south"), [3.5, 8.25, 7.0, 10.75]);
}

/// Every chest texture must resolve for every shape.
///
/// Two cases the MiEx definition got wrong: ender chests have one sheet for all shapes (there is no
/// `ender_left.png`), and copper chests pick a sheet per oxidation state.
#[test]
fn chest_sheets_cover_every_shape_and_oxidation_state() {
    let cases = [
        // (block id, expected base sheet)
        ("chest", "normal"),
        ("trapped_chest", "trapped"),
        ("copper_chest", "copper"),
        ("waxed_copper_chest", "copper"),
        ("exposed_copper_chest", "copper_exposed"),
        ("waxed_exposed_copper_chest", "copper_exposed"),
        ("weathered_copper_chest", "copper_weathered"),
        ("waxed_weathered_copper_chest", "copper_weathered"),
        ("oxidized_copper_chest", "copper_oxidized"),
        ("waxed_oxidized_copper_chest", "copper_oxidized"),
    ];

    let sheet = |state: &str| -> String {
        let model = model(state);
        let textures = model.textures.as_ref().expect("textures");
        match textures.get("texture").expect("texture key") {
            mtk_model::TextureValue::Path(path) => path.clone(),
            other => panic!("unexpected texture value {other:?}"),
        }
    };

    for (block, base) in cases {
        for (shape, suffix) in [("single", ""), ("left", "_left"), ("right", "_right")] {
            let state = format!("minecraft:{block}[facing=north,type={shape},waterlogged=false]");
            assert_eq!(
                sheet(&state),
                format!("minecraft:entity/chest/{base}{suffix}"),
                "{state}"
            );
        }
    }

    // Ender chests ignore the shape suffix entirely.
    for shape in ["single", "left", "right"] {
        let state = format!("minecraft:ender_chest[facing=north,type={shape},waterlogged=false]");
        assert_eq!(sheet(&state), "minecraft:entity/chest/ender", "{state}");
    }
}
