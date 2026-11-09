use super::*;

#[test]
fn test_bake_cube_blockstate() {
    let mut baker = ModelBaker::new();

    let model_json = r##"{
            "textures": {
                "all": "minecraft:block/stone"
            },
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

    let parsed_model: BlockModelJson = serde_json::from_str(model_json).unwrap();

    let baked = baker
        .bake_blockstate("minecraft:stone", None, |_id| Some(parsed_model.clone()))
        .unwrap();

    assert!(baked.is_cube);
    assert_eq!(baked.elements.len(), 1);
    assert_eq!(
        baked.get_face(Direction::Up).texture,
        "minecraft:block/stone"
    );

    let mesh = baked.to_mesh(false);
    assert_eq!(mesh.face_count(), 6);
    assert_eq!(mesh.triangle_count(), 12);
    assert_eq!(mesh.vertex_count(), 24);
}

#[test]
fn test_bake_blockstate_variants() {
    let mut baker = ModelBaker::new();

    let model_json = r##"{
            "textures": { "all": "minecraft:block/dirt" },
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "up": { "texture": "#all", "cullface": "up" }
                }
            }]
        }"##;
    let parsed_model: BlockModelJson = serde_json::from_str(model_json).unwrap();

    let bs_def_json = r##"{
            "variants": {
                "": [
                    { "model": "minecraft:block/dirt" },
                    { "model": "minecraft:block/dirt", "y": 90 },
                    { "model": "minecraft:block/dirt", "y": 180 },
                    { "model": "minecraft:block/dirt", "y": 270 }
                ]
            }
        }"##;
    let bs_def: BlockStateDefinition = serde_json::from_str(bs_def_json).unwrap();

    let group = baker
        .bake_blockstate_variants("minecraft:dirt", Some(&bs_def), |_id| {
            Some(parsed_model.clone())
        })
        .unwrap();

    assert_eq!(group.len(), 4);
    assert_eq!(group.weights, vec![1, 1, 1, 1]);
    assert_eq!(group.total_weight, 4);

    assert_eq!(group.models[0].get_face(Direction::Up).uv_rot, 0.0);
    assert_eq!(group.models[1].get_face(Direction::Up).uv_rot, 90.0);
    assert_eq!(group.models[2].get_face(Direction::Up).uv_rot, 180.0);
    assert_eq!(group.models[3].get_face(Direction::Up).uv_rot, 270.0);
}

#[test]
fn test_bake_single_then_variants_cache_regression() {
    let mut baker = ModelBaker::new();

    let model_json = r##"{
            "textures": { "all": "minecraft:block/dirt" },
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "up": { "texture": "#all", "cullface": "up" }
                }
            }]
        }"##;
    let parsed_model: BlockModelJson = serde_json::from_str(model_json).unwrap();

    let bs_def_json = r##"{
            "variants": {
                "": [
                    { "model": "minecraft:block/dirt" },
                    { "model": "minecraft:block/dirt", "y": 90 }
                ]
            }
        }"##;
    let bs_def: BlockStateDefinition = serde_json::from_str(bs_def_json).unwrap();

    // 1. First call bake_blockstate (single model baked into bake_cache)
    let single_baked = baker
        .bake_blockstate("minecraft:dirt", Some(&bs_def), |_id| {
            Some(parsed_model.clone())
        })
        .unwrap();
    assert_eq!(single_baked.get_face(Direction::Up).uv_rot, 0.0);

    // 2. Then call bake_blockstate_variants; must not return a 1-element group from bake_cache
    let group = baker
        .bake_blockstate_variants("minecraft:dirt", Some(&bs_def), |_id| {
            Some(parsed_model.clone())
        })
        .unwrap();
    assert_eq!(
        group.len(),
        2,
        "bake_blockstate_variants after bake_blockstate must not lose variants"
    );
    assert_eq!(group.weights, vec![1, 1]);
    assert_eq!(group.total_weight, 2);
    assert_eq!(group.models[0].get_face(Direction::Up).uv_rot, 0.0);
    assert_eq!(group.models[1].get_face(Direction::Up).uv_rot, 90.0);

    // 3. Clear cache and test reverse order: variants first, then single
    baker.clear_cache();
    let group_first = baker
        .bake_blockstate_variants("minecraft:dirt", Some(&bs_def), |_id| {
            Some(parsed_model.clone())
        })
        .unwrap();
    assert_eq!(group_first.len(), 2);

    let single_after = baker
        .bake_blockstate("minecraft:dirt", Some(&bs_def), |_id| {
            Some(parsed_model.clone())
        })
        .unwrap();
    assert_eq!(single_after.get_face(Direction::Up).uv_rot, 0.0);
}
