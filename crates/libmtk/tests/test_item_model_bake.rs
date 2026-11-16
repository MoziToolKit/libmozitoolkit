//! Item-model baking contract: every item model must either produce geometry with its own
//! display transforms, or be reported as a bake failure. Silent substitution with a block
//! model is forbidden — that is how item frames ended up with guessed scale and orientation.

use mtk_model::ItemTransforms;
use mtk_resource::{DirectoryPack, MemoryPack, ResourcePackStack};

fn png(rgba: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba));
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    bytes
}

fn build_stack() -> ResourcePackStack {
    let mut stack = ResourcePackStack::new();
    let mut pack = MemoryPack::new("ItemModelTest");

    pack.insert(
        "assets/minecraft/textures/item/sprite.png",
        png([255, 0, 0, 255]),
    );
    pack.insert(
        "assets/minecraft/textures/block/gadget.png",
        png([0, 255, 0, 255]),
    );

    // 2D sprite item: geometry from the layer texture, display inherited from `item/generated`.
    pack.insert(
        "assets/minecraft/models/item/generated.json",
        br#"{
            "display": {
                "fixed": { "rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [1, 1, 1] },
                "gui": { "rotation": [0, 0, 0], "translation": [0, 0, 0], "scale": [1, 1, 1] }
            }
        }"#
        .to_vec(),
    );
    pack.insert(
        "assets/minecraft/models/item/sprite.json",
        br#"{
            "parent": "minecraft:item/generated",
            "textures": { "layer0": "minecraft:item/sprite" }
        }"#
        .to_vec(),
    );

    // 3D block-entity item: display lives on the item template, geometry on the block model.
    pack.insert(
        "assets/minecraft/models/item/template_gadget.json",
        br#"{
            "display": {
                "gui": { "rotation": [30, 45, 0], "translation": [0, 0, 0], "scale": [0.625, 0.625, 0.625] },
                "fixed": { "rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [1, 1, 1] }
            }
        }"#
        .to_vec(),
    );
    pack.insert(
        "assets/minecraft/models/item/gadget.json",
        br#"{ "parent": "minecraft:item/template_gadget" }"#.to_vec(),
    );
    pack.insert(
        "assets/minecraft/models/block/gadget.json",
        br##"{
            "textures": { "all": "minecraft:block/gadget" },
            "elements": [
                {
                    "from": [1, 0, 1],
                    "to": [15, 14, 15],
                    "faces": {
                        "up":    { "texture": "#all" },
                        "down":  { "texture": "#all" },
                        "north": { "texture": "#all" },
                        "south": { "texture": "#all" },
                        "west":  { "texture": "#all" },
                        "east":  { "texture": "#all" }
                    }
                }
            ]
        }"##
        .to_vec(),
    );
    pack.insert(
        "assets/minecraft/blockstates/gadget.json",
        br#"{ "variants": { "": { "model": "minecraft:block/gadget" } } }"#.to_vec(),
    );

    // Nothing to bake: no elements, no layer textures, no block counterpart.
    pack.insert(
        "assets/minecraft/models/item/unsupported.json",
        br#"{
            "display": { "fixed": { "rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [1, 1, 1] } }
        }"#
        .to_vec(),
    );

    stack.push_pack(Box::new(pack));
    stack
}

#[test]
fn item_models_get_geometry_and_keep_their_own_display_transforms() {
    let db = libmtk::prebake_all_models(&build_stack(), None).expect("prebake");

    // 1. 2D sprite: extruded geometry + inherited display.
    let sprite = db
        .get_item("minecraft:item/sprite")
        .expect("sprite item must be baked");
    assert!(!sprite.elements.is_empty(), "sprite must have geometry");
    assert_eq!(
        sprite.transforms.fixed.map(|t| t.rotation),
        Some([0.0, 180.0, 0.0]),
        "sprite must keep its inherited display transforms"
    );
    assert_eq!(sprite.gui_light, Some(mtk_model::GuiLight::Front));

    // 2. builtin/entity item: geometry from the block model, display from the item template.
    let gadget = db
        .get_item("minecraft:item/gadget")
        .expect("block-entity item must be baked from block geometry");
    assert_eq!(gadget.elements.len(), 1, "block geometry must be used");
    let gui = gadget
        .transforms
        .gui
        .expect("item template gui transform must survive");
    assert_eq!(gui.rotation, [30.0, 45.0, 0.0]);
    assert_eq!(gui.scale, [0.625, 0.625, 0.625]);
    assert!(
        gadget.transforms != ItemTransforms::default(),
        "block-entity items must never fall back to identity transforms"
    );

    // 3. The bare block entry must stay the block model, not be overwritten by item data.
    let block = db
        .models
        .get("minecraft:gadget")
        .expect("blockstate entry must exist");
    assert_eq!(
        block.transforms,
        ItemTransforms::default(),
        "item insertion must not clobber the blockstate entry with item display data"
    );

    // 4. Explicit `item/...` requests never resolve to a fuzzy blockstate variant.
    assert!(
        db.get_item("minecraft:item/unsupported").is_none(),
        "an unbakeable item model must resolve to None, not to a guessed block model"
    );
    assert!(
        db.get_item("minecraft:gadget").is_some(),
        "bare block ids may still resolve a representative blockstate"
    );

    // 5. The failure is recorded instead of disappearing.
    let failures = db.bake_failures();
    let unsupported = failures
        .iter()
        .find(|failure| failure.key == "minecraft:item/unsupported")
        .expect("unbakeable item model must be reported");
    assert_eq!(unsupported.reason, "no_geometry");
    assert!(
        !failures
            .iter()
            .any(|failure| failure.key == "minecraft:item/gadget"),
        "resolved items must not be reported as failures"
    );
}

/// Real-asset guard: every vanilla item model the renderer can reach must either bake with
/// its own display transforms or be reported as a failure.
#[test]
fn real_vanilla_item_models_keep_display_transforms() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // <repo>/crates/libmtk -> <repo> -> the sibling asset checkout directory
    let workspace_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("crate is nested under the repository root");
    let root = std::env::var("MTK_TEST_ASSETS").unwrap_or_else(|_| {
        workspace_root
            .join("Minecraft_Source")
            .to_string_lossy()
            .into_owned()
    });
    if !std::path::Path::new(&root)
        .join("assets/minecraft/models/item")
        .is_dir()
    {
        eprintln!("[mtk] skipping: no vanilla assets at {root}");
        return;
    }

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("Vanilla", &root)));
    let db = libmtk::prebake_all_models(&stack, None).expect("prebake vanilla");

    for item in [
        "minecraft:item/chest",
        "minecraft:item/shulker_box",
        "minecraft:item/decorated_pot",
        "minecraft:item/conduit",
        "minecraft:item/bow",
    ] {
        let model = db
            .get_item(item)
            .unwrap_or_else(|| panic!("{item} must be baked"));
        assert!(!model.elements.is_empty(), "{item} must carry geometry");
        assert!(
            model.transforms != ItemTransforms::default(),
            "{item} must keep item display transforms"
        );
    }

    // Block-context entries must not be overwritten by item models.
    let chest_block = db
        .models
        .get("minecraft:chest")
        .expect("blockstate entry must survive");
    assert_eq!(
        chest_block.transforms,
        ItemTransforms::default(),
        "blockstate entry must not inherit item display data"
    );

    // Known unbakeable builtin-entity items are reported, never guessed.
    let failures = db.bake_failures();
    assert!(
        failures.iter().any(|f| f.key == "minecraft:item/shield"),
        "shield has no bakeable geometry and must be reported"
    );
    assert!(
        !failures.iter().any(|f| f.key == "minecraft:item/chest"),
        "resolved items must not be reported as failures"
    );
    assert!(db.get_item("minecraft:item/shield").is_none());
}
