use std::collections::HashMap;

use glam::Vec3;
use mtk_core::Direction;
use mtk_model::parser::obj::{ModObjLoader, WavefrontObjParser};
use mtk_model::{BlockModelJson, BlockStateDefinition, ModelBaker};
use mtk_testkit::{blockstate_json_path, model_json_path, require_assets};
use std::path::Path;

/// Reads a vanilla blockstate definition through the shared `mtk-testkit` asset resolver.
fn read_vanilla_blockstate(root: &Path, block_id: &str) -> Option<BlockStateDefinition> {
    std::fs::read_to_string(blockstate_json_path(root, block_id))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

/// Builds a model loader closure resolving parent models through `mtk-testkit`.
fn vanilla_model_loader(root: &Path) -> impl Fn(&str) -> Option<BlockModelJson> + '_ {
    move |id: &str| {
        std::fs::read_to_string(model_json_path(root, id))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
    }
}

#[test]
fn test_stairs_blockstate_baking_and_culling() {
    let mut baker = ModelBaker::new();

    let bs_json = r##"{
        "variants": {
            "facing=east,half=bottom,shape=straight": {
                "model": "minecraft:block/oak_stairs",
                "y": 90,
                "uvlock": true
            }
        }
    }"##;

    let model_json = r##"{
        "textures": {
            "bottom": "minecraft:block/oak_planks",
            "top": "minecraft:block/oak_planks",
            "side": "minecraft:block/oak_planks"
        },
        "elements": [
            {
                "from": [0, 0, 0],
                "to": [16, 8, 16],
                "faces": {
                    "down":  { "texture": "#bottom", "cullface": "down" },
                    "up":    { "texture": "#top" },
                    "north": { "texture": "#side", "cullface": "north" },
                    "south": { "texture": "#side", "cullface": "south" },
                    "west":  { "texture": "#side", "cullface": "west" },
                    "east":  { "texture": "#side", "cullface": "east" }
                }
            },
            {
                "from": [8, 8, 0],
                "to": [16, 16, 16],
                "faces": {
                    "down":  { "texture": "#bottom" },
                    "up":    { "texture": "#top", "cullface": "up" },
                    "north": { "texture": "#side", "cullface": "north" },
                    "south": { "texture": "#side", "cullface": "south" },
                    "west":  { "texture": "#side" },
                    "east":  { "texture": "#side", "cullface": "east" }
                }
            }
        ]
    }"##;

    let def: BlockStateDefinition = serde_json::from_str(bs_json).unwrap();
    let model: BlockModelJson = serde_json::from_str(model_json).unwrap();

    let baked = baker
        .bake_blockstate(
            "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]",
            Some(&def),
            |_id| Some(model.clone()),
        )
        .unwrap();

    assert!(!baked.is_cube);
    assert_eq!(baked.elements.len(), 2);

    // Unclipped mesh includes internal contact faces
    let unclipped_mesh = baked.to_mesh(false);
    assert_eq!(unclipped_mesh.face_count(), 12);

    // Clipped mesh: the contact faces (upper bottom and lower top overlap) are eliminated
    let clipped_mesh = baked.to_mesh(true);
    // Lower element top face [8..16, 8, 0..16] is fully covered by upper element,
    // and upper element bottom face [8..16, 8, 0..16] is fully covered by lower element.
    // Both contact regions should be clipped away!
    assert!(
        clipped_mesh.face_count() < unclipped_mesh.face_count(),
        "Clipped mesh faces ({}) should be fewer than unclipped ({})",
        clipped_mesh.face_count(),
        unclipped_mesh.face_count()
    );
}

#[test]
fn test_obj_polygon_fan_and_negative_indices() {
    let obj_text = r#"
# 5-vertex polygon and negative indices
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.5 1.5 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.5 1.0
vt 0.0 1.0
usemtl [1]minecraft:block/gold_block
f -5/-5 -4/-4 -3/-3 -2/-2 -1/-1
"#;

    let faces = WavefrontObjParser::parse_str(obj_text, None);
    // 5-vertex polygon fanned into 3 triangles: (0,1,2), (0,2,3), (0,3,4)
    assert_eq!(faces.len(), 3);
    for f in &faces {
        assert_eq!(f.verts.len(), 3);
        assert_eq!(f.material, "minecraft:block/gold_block");
        assert_eq!(f.tint_index, 1);
    }

    let baked = ModObjLoader::process_raw_faces(
        &faces,
        &HashMap::new(),
        "minecraft:block/dirt",
        0.0,
        0.0,
        0.0,
        Vec3::ZERO,
    );
    assert_eq!(baked.len(), 3);
}

#[test]
fn test_builtin_chest_and_bell_fallback() {
    let mut baker = mtk_model::ModelBaker::new();

    // Chest with no model elements in loader -> should fallback to builtin chest model
    let empty_loader = |_: &str| None;
    let chest_model = baker
        .bake_blockstate(
            "minecraft:chest[facing=north,type=single]",
            None,
            empty_loader,
        )
        .expect("Should bake chest using builtin model");

    assert!(
        !chest_model.elements.is_empty(),
        "Chest should have builtin elements"
    );
    assert!(
        chest_model.elements.len() >= 3,
        "Single chest should have base, lid, and latch"
    );

    // Left double chest baked with same baker must resolve to normal_left, not cached single chest
    let chest_left = baker
        .bake_blockstate(
            "minecraft:chest[facing=north,type=left]",
            None,
            empty_loader,
        )
        .expect("Should bake left chest using builtin model");
    assert!(
        chest_left
            .elements
            .iter()
            .any(|e| { e.faces.values().any(|f| f.texture.contains("normal_left")) }),
        "Left chest must resolve to normal_left texture"
    );

    // Bell with vanilla-like frame-only elements (missing bell_body texture/elements)
    // should trigger fallback to builtin bell model and include the golden bell body
    let vanilla_bell_loader = |id: &str| {
        if id.contains("bell") {
            let mut textures = HashMap::new();
            textures.insert(
                "particle".to_string(),
                mtk_model::TextureValue::Path("minecraft:block/bell_bottom".to_string()),
            );
            textures.insert(
                "bar".to_string(),
                mtk_model::TextureValue::Path("minecraft:block/dark_oak_planks".to_string()),
            );
            textures.insert(
                "post".to_string(),
                mtk_model::TextureValue::Path("minecraft:block/stone".to_string()),
            );
            // Vanilla has 3 frame elements, 0 bell body
            Some(mtk_model::BlockModelJson {
                parent: None,
                ambientocclusion: Some(true),
                textures: Some(textures),
                elements: Some(vec![mtk_model::ElementJson {
                    from: [2.0, 13.0, 7.0],
                    to: [14.0, 15.0, 9.0],
                    faces: HashMap::new(),
                    rotation: None,
                    transform: None,
                    shade: None,
                }]),
            })
        } else {
            None
        }
    };
    let bell_model = baker
        .bake_blockstate(
            "minecraft:bell[attachment=floor,facing=north]",
            None,
            vanilla_bell_loader,
        )
        .expect("Should bake bell with body fallback");

    assert_eq!(
        bell_model.elements.len(),
        5,
        "Bell should have all 5 elements (3 frame + 2 body)"
    );
    let bell_mesh = bell_model.to_mesh(false);
    assert!(
        bell_mesh.face_count() >= 28,
        "Bell should have at least 28 faces (16+ frame + 12 body)"
    );

    // Test Chest UV correctness (top of lid and base floor)
    let chest_lid = &chest_model.elements[1];
    let lid_up = chest_lid
        .faces
        .get(&Direction::Up)
        .expect("Lid should have Up face");
    assert!(
        lid_up.uv_bounds[0] >= 0.2 && lid_up.uv_bounds[2] <= 0.5,
        "Lid up face should map to outside top"
    );

    // Double chest left and right
    let left_chest = baker
        .bake_blockstate(
            "minecraft:chest[facing=north,type=left]",
            None,
            empty_loader,
        )
        .expect("Should bake left chest");
    let left_bottom = &left_chest.elements[0];
    let left_west = left_bottom
        .faces
        .get(&Direction::West)
        .expect("Left chest should have West face");
    assert!(
        left_west.uv_bounds[0] <= 0.05,
        "Left chest West face should be solid outer texture"
    );

    let right_chest = baker
        .bake_blockstate(
            "minecraft:chest[facing=north,type=right]",
            None,
            empty_loader,
        )
        .expect("Should bake right chest");
    let right_bottom = &right_chest.elements[0];
    let right_east = right_bottom
        .faces
        .get(&Direction::East)
        .expect("Right chest should have East face");
    assert!(
        right_east.uv_bounds[0] >= 0.4,
        "Right chest East face should be solid outer texture"
    );
}

#[test]
fn test_builtin_models_coverage() {
    let mut baker = mtk_model::ModelBaker::new();
    let empty_loader = |_: &str| None;

    // Chest
    let chest = baker
        .bake_blockstate(
            "minecraft:chest[facing=north,type=single]",
            None,
            empty_loader,
        )
        .expect("Should bake chest");
    assert!(!chest.elements.is_empty(), "Chest should have elements");

    // Shulker box
    let shulker = baker
        .bake_blockstate("minecraft:shulker_box[facing=up]", None, empty_loader)
        .expect("Should bake shulker box");
    assert!(
        !shulker.elements.is_empty(),
        "Shulker box should have elements"
    );

    // Decorated pot
    let pot = baker
        .bake_blockstate("minecraft:decorated_pot[facing=north]", None, empty_loader)
        .expect("Should bake decorated pot");
    assert!(
        !pot.elements.is_empty(),
        "Decorated pot should have elements"
    );

    // Dragon head
    let dragon_head = baker
        .bake_blockstate("minecraft:dragon_head[rotation=0]", None, empty_loader)
        .expect("Should bake dragon head");
    assert!(
        !dragon_head.elements.is_empty(),
        "Dragon head should have elements"
    );

    // Skull
    let skull = baker
        .bake_blockstate("minecraft:skeleton_skull[rotation=0]", None, empty_loader)
        .expect("Should bake skull");
    assert!(!skull.elements.is_empty(), "Skull should have elements");

    // End portal
    let end_portal = baker
        .bake_blockstate("minecraft:end_portal", None, empty_loader)
        .expect("Should bake end portal");
    assert!(
        !end_portal.elements.is_empty(),
        "End portal should have elements"
    );
}

#[test]
fn test_baked_model_cull_meta_serialization() {
    let mut baker = ModelBaker::new();
    let cube_json = r##"{
        "textures": { "all": "minecraft:block/stone" },
        "elements": [{
            "from": [0, 0, 0],
            "to": [16, 16, 16],
            "faces": {
                "down":  { "texture": "#all", "cullface": "down" },
                "up":    { "texture": "#all", "cullface": "up" },
                "north": { "texture": "#all", "cullface": "north" },
                "south": { "texture": "#all", "cullface": "south" },
                "west":  { "texture": "#all", "cullface": "west" },
                "east":  { "texture": "#all", "cullface": "east" }
            }
        }]
    }"##;
    let model: BlockModelJson = serde_json::from_str(cube_json).unwrap();
    let baked = baker
        .bake_blockstate("minecraft:stone", None, |_| Some(model.clone()))
        .unwrap();
    assert!(baked.cull_meta.is_some());
    let meta = baked.cull_meta.as_ref().unwrap();
    assert_eq!(meta.category, mtk_cull::CullCategory::SolidOpaque);
    assert_eq!(meta.full_face_mask, mtk_core::direction::DirMask::ALL);

    // Test postcard roundtrip
    let bin = postcard::to_allocvec(&baked).unwrap();
    let deserialized: mtk_model::baker::baked_model::BakedModel =
        postcard::from_bytes(&bin).unwrap();
    assert!(deserialized.cull_meta.is_some());
    let des_meta = deserialized.cull_meta.unwrap();
    assert_eq!(des_meta.category, mtk_cull::CullCategory::SolidOpaque);
    assert_eq!(des_meta.full_face_mask, mtk_core::direction::DirMask::ALL);
}

#[test]
fn test_baked_model_atlas_uv_remapping() {
    let mut baker = ModelBaker::new();
    let cube_json = r##"{
        "textures": { "all": "minecraft:block/stone" },
        "elements": [{
            "from": [0, 0, 0],
            "to": [16, 16, 16],
            "faces": {
                "up": { "texture": "#all", "cullface": "up" }
            }
        }]
    }"##;
    let model: BlockModelJson = serde_json::from_str(cube_json).unwrap();
    let mut baked = baker
        .bake_blockstate("minecraft:stone", None, |_| Some(model.clone()))
        .unwrap();

    // Before remapping: atlas_uvs is None
    let up_face = baked.elements[0]
        .faces
        .get(&mtk_core::direction::Direction::Up)
        .unwrap();
    assert!(up_face.atlas_uvs.is_none());

    // Remap to atlas: bounds [0.25, 0.5, 0.5, 0.75], chunk_id 1, texture_id 42
    baked.remap_to_atlas_with(|tex| {
        if tex == "minecraft:block/stone" {
            Some(([0.25, 0.5, 0.5, 0.75], 1, 42))
        } else {
            None
        }
    });

    let remapped_face = baked.elements[0]
        .faces
        .get(&mtk_core::direction::Direction::Up)
        .unwrap();
    assert!(remapped_face.atlas_uvs.is_some());
    assert_eq!(remapped_face.atlas_chunk_id, Some(1));
    assert_eq!(remapped_face.atlas_texture_id, Some(42));

    let uvs = remapped_face.atlas_uvs.unwrap();
    // Verify UV values are strictly in [0.25..0.5] and [0.5..0.75]
    for uv in &uvs {
        assert!(uv.x >= 0.25 && uv.x <= 0.5);
        assert!(uv.y >= 0.5 && uv.y <= 0.75);
    }

    // Test postcard roundtrip serialization
    let bin = postcard::to_allocvec(&baked).unwrap();
    let deserialized: mtk_model::baker::baked_model::BakedModel =
        postcard::from_bytes(&bin).unwrap();
    let des_face = deserialized.elements[0]
        .faces
        .get(&mtk_core::direction::Direction::Up)
        .unwrap();
    assert_eq!(des_face.atlas_uvs, Some(uvs));
    assert_eq!(des_face.atlas_chunk_id, Some(1));
    assert_eq!(des_face.atlas_texture_id, Some(42));
}

#[test]
fn test_baked_model_six_direction_bucketing() {
    use mtk_core::direction::Direction;
    use mtk_model::baker::ModelBaker;
    use mtk_model::parser::BlockModelJson;

    let mut baker = ModelBaker::new();

    let complex_json = r##"{
        "textures": {
            "top": "minecraft:block/stone",
            "bottom": "minecraft:block/dirt",
            "rod": "minecraft:block/iron_block"
        },
        "elements": [
            {
                "from": [0, 0, 0],
                "to": [16, 8, 16],
                "faces": {
                    "down": { "texture": "#bottom", "cullface": "down" },
                    "up": { "texture": "#top", "cullface": "up" },
                    "north": { "texture": "#rod" }
                }
            },
            {
                "from": [0, 8, 0],
                "to": [8, 16, 16],
                "faces": {
                    "up": { "texture": "#top", "cullface": "up" }
                }
            }
        ]
    }"##;
    let model: BlockModelJson = serde_json::from_str(complex_json).unwrap();
    let baked = baker
        .bake_blockstate("minecraft:custom_stairs", None, |_| Some(model.clone()))
        .unwrap();

    // Verify bucketing
    // Up has 2 faces (one from elem 0, one from elem 1)
    let up_bucket = &baked.culled_faces[Direction::Up.to_index()];
    assert_eq!(up_bucket.len(), 2, "Up bucket must contain 2 culled faces");

    // Down has 1 face
    let down_bucket = &baked.culled_faces[Direction::Down.to_index()];
    assert_eq!(
        down_bucket.len(),
        1,
        "Down bucket must contain 1 culled face"
    );

    // South, West, East have 0 faces
    assert_eq!(baked.culled_faces[Direction::South.to_index()].len(), 0);
    assert_eq!(baked.culled_faces[Direction::West.to_index()].len(), 0);
    assert_eq!(baked.culled_faces[Direction::East.to_index()].len(), 0);

    // North has cullface == None, so it must be in unculled_faces
    assert_eq!(
        baked.unculled_faces.len(),
        1,
        "North face without cullface must be in unculled_faces"
    );
    assert_eq!(baked.unculled_faces[0].direction, Direction::North);

    // Test postcard roundtrip serialization preserving face buckets
    let bin = postcard::to_allocvec(&baked).unwrap();
    let deserialized: mtk_model::baker::baked_model::BakedModel =
        postcard::from_bytes(&bin).unwrap();
    assert_eq!(deserialized.culled_faces[Direction::Up.to_index()].len(), 2);
    assert_eq!(
        deserialized.culled_faces[Direction::Down.to_index()].len(),
        1
    );
    assert_eq!(deserialized.unculled_faces.len(), 1);
}

#[test]
fn test_bedrock_culling_metadata_distinct_from_bed() {
    let mut baker = mtk_model::ModelBaker::new();

    // Bedrock should bake as a standard 16x16x16 cube
    let cube_loader = |_: &str| {
        let json = r##"{
            "textures": { "all": "minecraft:block/bedrock" },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "down":  { "texture": "#all", "cullface": "down" },
                        "up":    { "texture": "#all", "cullface": "up" },
                        "north": { "texture": "#all", "cullface": "north" },
                        "south": { "texture": "#all", "cullface": "south" },
                        "west":  { "texture": "#all", "cullface": "west" },
                        "east":  { "texture": "#all", "cullface": "east" }
                    }
                }
            ]
        }"##;
        serde_json::from_str::<BlockModelJson>(json).ok()
    };

    let bedrock = baker
        .bake_blockstate("minecraft:bedrock", None, cube_loader)
        .expect("Should bake bedrock");

    assert!(bedrock.is_cube, "Bedrock must be recognized as a cube");
    assert!(bedrock.is_opaque, "Bedrock must be recognized as opaque");

    let cull_meta = bedrock
        .cull_meta
        .as_ref()
        .expect("Bedrock must have cull_meta");
    assert_eq!(
        cull_meta.category,
        mtk_cull::CullCategory::SolidOpaque,
        "Bedrock category must be SolidOpaque, NOT GlassTranslucent or PartialShape"
    );
    assert!(
        cull_meta.is_full_cube,
        "Bedrock must be full cube in cull meta"
    );
    assert!(cull_meta.is_opaque, "Bedrock must be opaque in cull meta");
    assert_eq!(
        cull_meta.full_face_mask,
        mtk_core::direction::DirMask::ALL,
        "Bedrock full_face_mask must cover all 6 faces"
    );

    // Conversely, verify red bed is NOT SolidOpaque
    let empty_loader = |_: &str| None;
    let bed = baker
        .bake_blockstate(
            "minecraft:red_bed[facing=north,part=foot]",
            None,
            empty_loader,
        )
        .expect("Should bake red bed");

    assert!(!bed.is_cube, "Bed must NOT be a full cube");
    assert!(!bed.is_opaque, "Bed must NOT be opaque");
    let bed_cull_meta = bed.cull_meta.as_ref().expect("Bed must have cull_meta");
    assert_ne!(
        bed_cull_meta.category,
        mtk_cull::CullCategory::SolidOpaque,
        "Bed category must NOT be SolidOpaque"
    );
}

#[test]
fn test_baker_deoverlapping_duplicate_faces() {
    let mut baker = mtk_model::ModelBaker::new();

    // Model with 2 identical overlapping north faces (causes Z-fighting in DCC if unhandled)
    let loader = |_: &str| {
        let json = r##"{
            "textures": { "tex": "minecraft:block/stone" },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "north": { "texture": "#tex", "cullface": "north" }
                    }
                },
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "north": { "texture": "#tex", "cullface": "north" }
                    }
                }
            ]
        }"##;
        serde_json::from_str::<mtk_model::BlockModelJson>(json).ok()
    };

    let mut baked = baker
        .bake_blockstate("minecraft:stone", None, loader)
        .expect("Should bake test model");

    // When exclude_hidden_volume (de-overlapping) is false, both identical faces exist
    let raw_mesh = baked.to_mesh(false);
    assert_eq!(
        raw_mesh.triangle_count(),
        4,
        "Raw unclipped mesh should have 2 quads (4 triangles)"
    );

    // When exclude_hidden_volume is true, duplicate face must be eliminated to prevent DCC Z-fighting
    let clean_mesh = baked.to_mesh(true);
    assert_eq!(
        clean_mesh.triangle_count(),
        2,
        "Clean mesh must have exactly 1 quad (2 triangles) with duplicate eliminated"
    );

    // Also verify deduplicate_faces on BakedModel structure directly
    let removed = baked.deduplicate_faces();
    assert_eq!(
        removed, 1,
        "deduplicate_faces must report 1 removed duplicate face"
    );
    let after_mesh = baked.to_mesh(false);
    assert_eq!(
        after_mesh.triangle_count(),
        2,
        "After deduplicate_faces, raw mesh must also have 1 quad"
    );
}

#[test]
fn test_baker_coplanar_opposite_contact_faces() {
    let mut baker = mtk_model::ModelBaker::new();

    // Two stacked elements touching at y=8
    let loader = |_: &str| {
        let json = r##"{
            "textures": { "tex": "minecraft:block/oak_planks" },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 8, 16],
                    "faces": {
                        "down": { "texture": "#tex", "cullface": "down" },
                        "up":   { "texture": "#tex" }
                    }
                },
                {
                    "from": [0, 8, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "down": { "texture": "#tex" },
                        "up":   { "texture": "#tex", "cullface": "up" }
                    }
                }
            ]
        }"##;
        serde_json::from_str::<mtk_model::BlockModelJson>(json).ok()
    };

    let mut baked = baker
        .bake_blockstate("minecraft:oak_slab", None, loader)
        .expect("Should bake slab");

    // Raw mesh has 4 faces (8 triangles)
    let raw_mesh = baked.to_mesh(false);
    assert_eq!(raw_mesh.triangle_count(), 8);

    // Clean mesh should eliminate the interior touching faces at y=8, leaving only external down and up
    let clean_mesh = baked.to_mesh(true);
    assert_eq!(
        clean_mesh.triangle_count(),
        4,
        "Interior contacting faces at y=8 must be culled (leaving 2 outer quads)"
    );

    // Also verify BakedModel deduplicate_faces
    let removed = baked.deduplicate_faces();
    assert_eq!(removed, 2, "Both contacting interior faces must be removed");
    let after_mesh = baked.to_mesh(false);
    assert_eq!(after_mesh.triangle_count(), 4);
}

#[test]
fn test_builtin_blockbench_json_models() {
    use mtk_model::baker::ModelBaker;
    use mtk_model::builtin::BuiltinModelRegistry;

    let test_model_ids = [
        "minecraft:block/chest",
        "minecraft:block/chest_left",
        "minecraft:block/chest_right",
        "minecraft:block/trapped_chest",
        "minecraft:block/ender_chest",
        "minecraft:block/shulker_box",
        "minecraft:block/skull",
        "minecraft:block/skull_wall",
        "minecraft:block/dragon_head",
        "minecraft:block/piglin_head",
        "minecraft:block/bell_floor",
        "minecraft:block/bell_ceiling",
        "minecraft:block/bell_wall",
        "minecraft:block/bell_between_walls",
        "minecraft:block/end_portal",
        "minecraft:block/decorated_pot",
    ];

    for model_id in test_model_ids {
        let model = BuiltinModelRegistry::get_builtin_model_by_id(model_id)
            .unwrap_or_else(|| panic!("Builtin model {} must exist", model_id));
        assert!(
            model.elements.is_some() && !model.elements.as_ref().unwrap().is_empty(),
            "Model {} must have elements",
            model_id
        );
    }

    let mut baker = ModelBaker::new();
    let empty_loader = |_: &str| None;

    // Test baking double chests (left and right)
    let chest_left = baker
        .bake_blockstate(
            "minecraft:chest[facing=south,type=left]",
            None,
            empty_loader,
        )
        .expect("Left chest should bake");
    assert_eq!(chest_left.elements.len(), 3);

    let chest_right = baker
        .bake_blockstate(
            "minecraft:chest[facing=south,type=right]",
            None,
            empty_loader,
        )
        .expect("Right chest should bake");
    assert_eq!(chest_right.elements.len(), 3);

    // Test baking bell in all 4 attachments
    for att in ["floor", "ceiling", "single_wall", "double_wall"] {
        let state = format!("minecraft:bell[attachment={},facing=north]", att);
        let bell = baker
            .bake_blockstate(&state, None, empty_loader)
            .unwrap_or_else(|_| panic!("Bell {} must bake", att));
        assert!(!bell.elements.is_empty(), "Bell {} must have elements", att);
        // Has both support frame elements and 2 bell body elements
        assert!(
            bell.elements.len() >= 3,
            "Bell {} must contain frame and body",
            att
        );
    }

    // Verify single_wall bell touches the wall in all 4 directions
    for (facing, expected_dir) in [
        ("east", mtk_core::direction::Direction::East),
        ("west", mtk_core::direction::Direction::West),
        ("north", mtk_core::direction::Direction::North),
        ("south", mtk_core::direction::Direction::South),
    ] {
        let state = format!("minecraft:bell[attachment=single_wall,facing={}]", facing);
        let bell = baker
            .bake_blockstate(&state, None, empty_loader)
            .unwrap_or_else(|_| panic!("Bell single_wall facing {} must bake", facing));
        let mesh = bell.to_mesh(false);
        // Find extreme coordinate in the wall direction (should be 1.0 or 0.0)
        let mut touches_wall = false;
        for v in &mesh.positions {
            match expected_dir {
                mtk_core::direction::Direction::East => {
                    if (v[0] - 1.0).abs() < 1e-4 {
                        touches_wall = true;
                    }
                }
                mtk_core::direction::Direction::West => {
                    if v[0].abs() < 1e-4 {
                        touches_wall = true;
                    }
                }
                mtk_core::direction::Direction::South => {
                    if (v[2] - 1.0).abs() < 1e-4 {
                        touches_wall = true;
                    }
                }
                mtk_core::direction::Direction::North if v[2].abs() < 1e-4 => {
                    touches_wall = true;
                }
                _ => {}
            }
        }
        assert!(
            touches_wall,
            "Bell single_wall facing {} must touch wall at {:?}",
            facing, expected_dir
        );
    }
}

#[test]
fn test_cross_plant_dcc_single_sided_baking() {
    let mut baker = mtk_model::ModelBaker::new();
    let loader = |model_id: &str| {
        if model_id == "minecraft:block/dandelion" || model_id == "minecraft:block/cross" {
            let json = r##"{
                "ambientocclusion": false,
                "textures": {
                    "cross": "minecraft:block/dandelion"
                },
                "elements": [
                    {   "from": [ 0.8, 0, 8 ],
                        "to": [ 15.2, 16, 8 ],
                        "rotation": { "origin": [ 8, 8, 8 ], "axis": "y", "angle": 45, "rescale": true },
                        "shade": false,
                        "faces": {
                            "north": { "uv": [ 0, 0, 16, 16 ], "texture": "#cross" },
                            "south": { "uv": [ 0, 0, 16, 16 ], "texture": "#cross" }
                        }
                    },
                    {   "from": [ 8, 0, 0.8 ],
                        "to": [ 8, 16, 15.2 ],
                        "rotation": { "origin": [ 8, 8, 8 ], "axis": "y", "angle": 45, "rescale": true },
                        "shade": false,
                        "faces": {
                            "west": { "uv": [ 0, 0, 16, 16 ], "texture": "#cross" },
                            "east": { "uv": [ 0, 0, 16, 16 ], "texture": "#cross" }
                        }
                    }
                ]
            }"##;
            serde_json::from_str::<mtk_model::BlockModelJson>(json).ok()
        } else {
            None
        }
    };

    let mut baked = baker
        .bake_blockstate("minecraft:dandelion", None, loader)
        .expect("Should bake dandelion model");

    assert_eq!(baked.elements.len(), 2, "Cross model must have 2 elements");
    // In DCC-First baking, each zero-thickness planar element collapses its redundant back-to-back faces into 1 canonical face
    assert_eq!(
        baked.elements[0].faces.len(),
        1,
        "Element 0 must collapse to 1 face"
    );
    assert_eq!(
        baked.elements[1].faces.len(),
        1,
        "Element 1 must collapse to 1 face"
    );
    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::North));
    assert!(baked.elements[1]
        .faces
        .contains_key(&mtk_core::direction::Direction::West));

    // Raw mesh has exactly 2 quads = 4 triangles (no duplicate overlapping faces!)
    let raw_mesh = baked.to_mesh(false);
    assert_eq!(
        raw_mesh.triangle_count(),
        4,
        "Raw mesh must have exactly 2 quads (4 triangles)"
    );

    // Clean mesh (exclude_hidden_volume = true) must NOT cull the plant!
    let clean_mesh = baked.to_mesh(true);
    assert_eq!(
        clean_mesh.triangle_count(),
        4,
        "Clean mesh must preserve both cross quads (4 triangles)"
    );

    // deduplicate_faces also must not mistakenly remove the cross quads
    let removed = baked.deduplicate_faces();
    assert_eq!(
        removed, 0,
        "No duplicate faces to remove in already clean DCC model"
    );
    let after_mesh = baked.to_mesh(false);
    assert_eq!(after_mesh.triangle_count(), 4);
}

#[test]
fn test_sunflower_top_dual_texture_preserved() {
    let mut baker = mtk_model::ModelBaker::new();
    let loader = |model_id: &str| {
        if model_id == "minecraft:block/sunflower_top" {
            let json = r##"{
                "textures": {
                    "back": "minecraft:block/sunflower_back",
                    "front": "minecraft:block/sunflower_front"
                },
                "elements": [
                    {   "from": [ 9.6, -1, 1 ],
                        "to": [ 9.6, 15, 15 ],
                        "rotation": { "origin": [ 8, 8, 8 ], "axis": "z", "angle": 22.5, "rescale": true },
                        "faces": {
                            "west": { "uv": [ 0, 0, 16, 16 ], "texture": "#back" },
                            "east": { "uv": [ 0, 0, 16, 16 ], "texture": "#front" }
                        }
                    }
                ]
            }"##;
            serde_json::from_str::<mtk_model::BlockModelJson>(json).ok()
        } else {
            None
        }
    };

    let baked = baker
        .bake_blockstate("minecraft:sunflower_top", None, loader)
        .expect("Should bake sunflower_top model");

    // Since west (#back) and east (#front) have different textures, both faces are preserved!
    assert_eq!(
        baked.elements[0].faces.len(),
        2,
        "Sunflower top dual-textured face must preserve both sides"
    );
    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::West));
    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::East));
}

#[test]
fn test_scaffolding_baking_and_deduplicate() {
    let root = require_assets!();
    let mut baker = ModelBaker::new();
    let bs_def = read_vanilla_blockstate(&root, "minecraft:scaffolding")
        .expect("scaffolding blockstate fixture must be available via mtk-testkit");

    let mut baked = baker
        .bake_blockstate(
            "minecraft:scaffolding[bottom=false]",
            Some(&bs_def),
            vanilla_model_loader(&root),
        )
        .expect("Should bake scaffolding");

    assert_eq!(
        baked.elements.len(),
        9,
        "Scaffolding should have 9 elements"
    );
    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::Up));
    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::Down));

    let removed = baked.deduplicate_faces();
    assert_eq!(
        removed, 0,
        "No faces should be removed within isolated scaffolding model"
    );

    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::Up));
    assert!(baked.elements[0]
        .faces
        .contains_key(&mtk_core::direction::Direction::Down));

    let (mesh, textures) = baked.to_mesh_with_textures(false);
    assert_eq!(mesh.face_count(), 34);
    assert!(
        textures.iter().any(|t| t.contains("scaffolding_top")),
        "Scaffolding top texture must be present!"
    );
}

#[test]
fn test_vault_and_spawner_inverted_elements() {
    let root = require_assets!();
    let mut baker = ModelBaker::new();
    let loader = vanilla_model_loader(&root);

    // 1. Spawner: 2 elements (outer + inner cage), total 12 faces
    if let Some(bs_def) = read_vanilla_blockstate(&root, "minecraft:spawner") {
        let mut spawner = baker
            .bake_blockstate("minecraft:spawner", Some(&bs_def), &loader)
            .expect("Should bake spawner");

        assert_eq!(spawner.elements.len(), 2, "Spawner should have 2 elements");
        assert!(
            spawner.elements[1].is_inverted(),
            "Inner cage element must be detected as inverted"
        );

        let removed = spawner.deduplicate_faces();
        assert_eq!(
            removed, 0,
            "Deduplicate faces must not remove inverted cage faces"
        );

        let (mesh, textures) = spawner.to_mesh_with_textures(true);
        assert_eq!(
            mesh.face_count(),
            12,
            "Spawner mesh must have all 12 faces (6 outer + 6 inner)"
        );
        assert!(textures.iter().any(|t| t.contains("spawner")));
    }

    // 2. Vault: Element 0 (outer box) + Element 1 (inner cage), top face must be preserved
    if let Some(bs_def) = read_vanilla_blockstate(&root, "minecraft:vault") {
        let mut vault = baker
            .bake_blockstate(
                "minecraft:vault[facing=north,vault_state=inactive,ominous=false]",
                Some(&bs_def),
                &loader,
            )
            .expect("Should bake vault");

        assert_eq!(vault.elements.len(), 2, "Vault should have 2 elements");
        assert!(
            vault.elements[1].is_inverted(),
            "Cage inverted faces must be detected as inverted"
        );

        vault.deduplicate_faces();

        let (mesh, textures) = vault.to_mesh_with_textures(true);
        assert!(
            mesh.face_count() >= 7,
            "Vault mesh must retain both outer and inner faces (got {})",
            mesh.face_count()
        );

        // Check that at least one face has normal pointing UP (0, 1, 0)
        let has_up_normal = mesh.normals.iter().any(|n| n[1] > 0.99);
        assert!(
            has_up_normal,
            "Vault top face with normal (0, 1, 0) must be preserved!"
        );
        assert!(
            textures.iter().any(|t| t.contains("vault_top")),
            "vault_top texture must be present!"
        );
    }
}

#[test]
fn test_heavy_core_texture_resolution() {
    let root = require_assets!();
    let mut baker = ModelBaker::new();
    let loader = vanilla_model_loader(&root);

    if let Some(bs_def) = read_vanilla_blockstate(&root, "minecraft:heavy_core") {
        let hc = baker
            .bake_blockstate("minecraft:heavy_core", Some(&bs_def), &loader)
            .expect("Should bake heavy_core");

        let (mesh, textures) = hc.to_mesh_with_textures(true);
        assert_eq!(mesh.face_count(), 6, "Heavy core should have 6 faces");
        assert_eq!(
            textures,
            vec!["minecraft:block/heavy_core".to_string()],
            "Heavy core must resolve 'all' to 'minecraft:block/heavy_core'"
        );
    }
}

#[test]
fn test_end_portal_gateway_conduit_and_banner_builtins() {
    let mut baker = ModelBaker::new();
    let empty_loader = |_: &str| -> Option<mtk_model::BlockModelJson> { None };

    // 1. End Portal
    let ep = baker
        .bake_blockstate("minecraft:end_portal", None, empty_loader)
        .expect("Should bake end_portal builtin");
    assert!(ep.is_emissive, "End portal must be emissive");
    let (ep_mesh, ep_tex) = ep.to_mesh_with_textures(false);
    assert_eq!(
        ep_mesh.face_count(),
        1,
        "End portal should have 1 upward face"
    );
    assert_eq!(
        ep_tex,
        vec!["minecraft:entity/end_portal/end_portal".to_string()]
    );
    assert!(
        ep_mesh.normals.iter().all(|n| n[1] > 0.99),
        "End portal normal must face Up (0, 1, 0)"
    );

    // 2. End Gateway
    let eg = baker
        .bake_blockstate("minecraft:end_gateway", None, empty_loader)
        .expect("Should bake end_gateway builtin");
    assert!(eg.is_emissive, "End gateway must be emissive");
    let (eg_mesh, eg_tex) = eg.to_mesh_with_textures(false);
    assert_eq!(
        eg_mesh.face_count(),
        6,
        "End gateway should have 6 cube faces"
    );
    assert_eq!(
        eg_tex,
        vec!["minecraft:entity/end_portal/end_portal".to_string()]
    );

    // 3. Conduit
    let conduit = baker
        .bake_blockstate("minecraft:conduit", None, empty_loader)
        .expect("Should bake conduit builtin");
    assert!(conduit.is_emissive, "Conduit must be emissive");
    let (cd_mesh, cd_tex) = conduit.to_mesh_with_textures(false);
    assert_eq!(cd_mesh.face_count(), 6, "Conduit should have 6 faces");
    assert_eq!(cd_tex, vec!["minecraft:entity/conduit/base".to_string()]);

    // 4. Banner (standing)
    let standing_banner = baker
        .bake_blockstate("minecraft:black_banner[rotation=4]", None, empty_loader)
        .expect("Should bake standing banner builtin");
    let (sb_mesh, sb_tex) = standing_banner.to_mesh_with_textures(false);
    assert!(
        sb_mesh.face_count() >= 12,
        "Standing banner must have pole, crossbar, and cloth faces (got {})",
        sb_mesh.face_count()
    );
    assert!(sb_tex.contains(&"minecraft:entity/banner/banner_base".to_string()));
    // Verify all banner UV coordinates are properly bounded in [0.0, 1.0]
    for uv in &sb_mesh.uvs {
        assert!(
            uv[0] >= -1e-4 && uv[0] <= 1.0 + 1e-4,
            "Standing banner U coordinate out of bounds: {}",
            uv[0]
        );
        assert!(
            uv[1] >= -1e-4 && uv[1] <= 1.0 + 1e-4,
            "Standing banner V coordinate out of bounds: {}",
            uv[1]
        );
    }

    // 5. Wall Banner
    let wall_banner = baker
        .bake_blockstate(
            "minecraft:red_wall_banner[facing=north]",
            None,
            empty_loader,
        )
        .expect("Should bake wall banner builtin");
    let (wb_mesh, wb_tex) = wall_banner.to_mesh_with_textures(false);
    assert!(
        wb_mesh.face_count() >= 10,
        "Wall banner must have crossbar and cloth faces (got {})",
        wb_mesh.face_count()
    );
    assert!(wb_tex.contains(&"minecraft:entity/banner/banner_base".to_string()));
    for uv in &wb_mesh.uvs {
        assert!(
            uv[0] >= -1e-4 && uv[0] <= 1.0 + 1e-4,
            "Wall banner U coordinate out of bounds: {}",
            uv[0]
        );
        assert!(
            uv[1] >= -1e-4 && uv[1] <= 1.0 + 1e-4,
            "Wall banner V coordinate out of bounds: {}",
            uv[1]
        );
    }
    assert!(wb_tex.contains(&"minecraft:banner/red".to_string()));

    // 5.1 Standing banner with BlockEntityData (patterned)
    let banner_entity = mtk_model::BlockEntityData::new_banner(
        "white",
        vec![("stripe_top", "blue"), ("flower", "red")],
    );
    let patterned_banner = baker
        .bake_blockstate_with_entity(
            "minecraft:white_banner[rotation=0]",
            Some(&banner_entity),
            None,
            empty_loader,
        )
        .expect("Should bake patterned banner");
    let (_, pat_tex) = patterned_banner.to_mesh_with_textures(false);
    assert!(pat_tex.contains(&"minecraft:banner/white+stripe_top#blue+flower#red".to_string()));

    // 6. Decorated Pot
    let pot_model = baker
        .bake_blockstate("minecraft:decorated_pot[facing=north]", None, empty_loader)
        .expect("Should bake decorated pot builtin");
    let (pot_mesh, pot_tex) = pot_model.to_mesh_with_textures(false);
    assert!(
        pot_mesh.face_count() >= 16,
        "Decorated pot must have body and neck faces (got {})",
        pot_mesh.face_count()
    );
    assert!(pot_tex.contains(&"minecraft:entity/decorated_pot/decorated_pot_side".to_string()));
    assert!(pot_tex.contains(&"minecraft:entity/decorated_pot/decorated_pot_base".to_string()));

    // 6.1 Decorated Pot with RFC v3.2 Sherds BlockEntityData
    let pot_entity = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [
            Some("minecraft:angler_pottery_sherd".to_string()), // back
            None,                                               // left (blank brick)
            Some("miner".to_string()),                          // right
            Some("blade_pottery_sherd".to_string()),            // front
        ],
    };
    let decorated_model = baker
        .bake_blockstate_with_entity(
            "minecraft:decorated_pot[facing=north]",
            Some(&pot_entity),
            None,
            empty_loader,
        )
        .expect("Should bake decorated pot with BlockEntityData");
    let (dec_mesh, dec_tex) = decorated_model.to_mesh_with_textures(false);
    assert_eq!(dec_mesh.face_count(), pot_mesh.face_count());
    assert!(dec_tex.contains(&"minecraft:entity/decorated_pot/angler_pottery_pattern".to_string()));
    assert!(dec_tex.contains(&"minecraft:entity/decorated_pot/miner_pottery_pattern".to_string()));
    assert!(dec_tex.contains(&"minecraft:entity/decorated_pot/blade_pottery_pattern".to_string()));
    assert!(dec_tex.contains(&"minecraft:entity/decorated_pot/decorated_pot_side".to_string()));
    assert!(dec_tex.contains(&"minecraft:entity/decorated_pot/decorated_pot_base".to_string()));
    // 6.2 Verify directional textures on sides match vanilla DecoratedPotRenderer
    // facing=north: South (+Z) is front (blade), North (-Z) is back (angler),
    // West (-X) is left (side), East (+X) is right (miner).
    let pot_body = decorated_model
        .elements
        .iter()
        .find(|el| {
            (el.from_pos[0] - 1.0).abs() < 1e-3 || (el.from_pos[0] - 1.0 / 16.0).abs() < 1e-3
        })
        .expect("Pot body element must exist");

    let north_face = pot_body
        .faces
        .get(&mtk_core::direction::Direction::North)
        .expect("North face exists");
    assert!(
        north_face.texture.contains("angler_pottery_pattern"),
        "North face must be back pattern (angler), got {}",
        north_face.texture
    );

    let south_face = pot_body
        .faces
        .get(&mtk_core::direction::Direction::South)
        .expect("South face exists");
    assert!(
        south_face.texture.contains("blade_pottery_pattern"),
        "South face must be front pattern (blade), got {}",
        south_face.texture
    );

    let west_face = pot_body
        .faces
        .get(&mtk_core::direction::Direction::West)
        .expect("West face exists");
    assert!(
        west_face.texture.contains("decorated_pot_side"),
        "West face must be left pattern (default side), got {}",
        west_face.texture
    );

    let east_face = pot_body
        .faces
        .get(&mtk_core::direction::Direction::East)
        .expect("East face exists");
    assert!(
        east_face.texture.contains("miner_pottery_pattern"),
        "East face must be right pattern (miner), got {}",
        east_face.texture
    );

    // Verify top and bottom UV assignment
    let top_face = pot_body
        .faces
        .get(&mtk_core::direction::Direction::Up)
        .expect("Up face exists");
    // uvs: [North-West, South-West, South-East, North-East]
    assert!((top_face.uvs[0].x - 7.0 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[0].y - 13.5 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[1].x - 7.0 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[1].y - 6.5 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[2].x - 14.0 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[2].y - 6.5 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[3].x - 14.0 / 16.0).abs() < 1e-4);
    assert!((top_face.uvs[3].y - 13.5 / 16.0).abs() < 1e-4);

    let down_face = pot_body
        .faces
        .get(&mtk_core::direction::Direction::Down)
        .expect("Down face exists");
    assert!((down_face.uvs[0].x - 0.0 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[0].y - 6.5 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[1].x - 0.0 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[1].y - 13.5 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[2].x - 7.0 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[2].y - 13.5 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[3].x - 7.0 / 16.0).abs() < 1e-4);
    assert!((down_face.uvs[3].y - 6.5 / 16.0).abs() < 1e-4);

    // Verify decorated pot neck (Y in [16/16, 17/16]) and rim (Y in [17/16, 20/16])
    let max_y = pot_mesh
        .positions
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (max_y - 20.0 / 16.0).abs() < 1e-3,
        "Decorated pot rim top must be at Y=20/16 = 1.25 (got {})",
        max_y
    );
}

#[test]
fn test_dragon_and_piglin_head_geometry() {
    let mut baker = mtk_model::ModelBaker::new();
    let empty_loader = |_: &str| None;

    // 1. Dragon Head
    let dragon = baker
        .bake_blockstate("minecraft:dragon_head[rotation=0]", None, empty_loader)
        .expect("Should bake dragon head");
    let (dragon_mesh, dragon_tex) = dragon.to_mesh_with_textures(false);
    assert!(
        !dragon_mesh.positions.is_empty(),
        "Dragon mesh should have vertices"
    );
    assert!(dragon_tex.contains(&"minecraft:entity/enderdragon/dragon".to_string()));

    // 2. Piglin Head
    let piglin = baker
        .bake_blockstate("minecraft:piglin_head[rotation=0]", None, empty_loader)
        .expect("Should bake piglin head");
    let (piglin_mesh, piglin_tex) = piglin.to_mesh_with_textures(false);
    assert!(
        !piglin_mesh.positions.is_empty(),
        "Piglin mesh should have vertices"
    );
    assert!(piglin_tex.contains(&"minecraft:entity/piglin/piglin".to_string()));

    // Verify ears tilt outward beyond the head's lateral boundary:
    // Head width is [3..13] / 16 = [0.1875..0.8125].
    // Rotated ears tilt outward: max_x > 13.0/16 and min_x < 3.0/16.
    let min_x = piglin_mesh
        .positions
        .iter()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = piglin_mesh
        .positions
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        min_x < 3.0 / 16.0 - 0.05,
        "Piglin ear must tilt outward past West boundary (min_x = {})",
        min_x
    );
    assert!(
        max_x > 13.0 / 16.0 + 0.05,
        "Piglin ear must tilt outward past East boundary (max_x = {})",
        max_x
    );
}

#[test]
fn test_cross_emissive_coplanar_deduplication() {
    use glam::{Vec2, Vec3};
    use mtk_core::direction::Direction;
    use mtk_model::baker::baked_model::{BakedElement, BakedFace, BakedModel};
    use std::collections::HashMap;

    // Simulate cross_emissive model with 2 elements on the exact same quad geometry:
    // Element 0: base bush, North face
    // Element 1: firefly_bush_emissive, North face
    let verts = [
        Vec3::new(0.05, 0.0, 0.5),
        Vec3::new(0.95, 0.0, 0.5),
        Vec3::new(0.95, 1.0, 0.5),
        Vec3::new(0.05, 1.0, 0.5),
    ];
    let norm = Vec3::new(0.0, 0.0, -1.0);
    let uvs = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y];

    let mut el0_faces = HashMap::new();
    el0_faces.insert(
        Direction::North,
        BakedFace {
            direction: Direction::North,
            texture: "minecraft:block/firefly_bush".to_string(),
            vertices: verts,
            normal: norm,
            uvs,
            tint_index: -1,
            ..Default::default()
        },
    );

    let mut el1_faces = HashMap::new();
    el1_faces.insert(
        Direction::North,
        BakedFace {
            direction: Direction::North,
            texture: "minecraft:block/firefly_bush_emissive".to_string(),
            vertices: verts,
            normal: norm,
            uvs,
            tint_index: -1,
            ..Default::default()
        },
    );

    let mut model = BakedModel {
        block_state: "minecraft:firefly_bush".to_string(),
        elements: vec![
            BakedElement {
                from_pos: [0.8, 0.0, 8.0],
                to_pos: [15.2, 16.0, 8.0],
                faces: el0_faces,
            },
            BakedElement {
                from_pos: [0.8, 0.0, 8.0],
                to_pos: [15.2, 16.0, 8.0],
                faces: el1_faces,
            },
        ],
        obj_faces: Vec::new(),
        ..Default::default()
    };

    let removed = model.deduplicate_faces();
    assert_eq!(
        removed, 1,
        "Must deduplicate the redundant emissive companion face"
    );

    // Verify remaining face is the base texture
    let remaining_faces: Vec<_> = model
        .elements
        .iter()
        .flat_map(|e| e.faces.values())
        .collect();
    assert_eq!(remaining_faces.len(), 1);
    assert_eq!(remaining_faces[0].texture, "minecraft:block/firefly_bush");
}
