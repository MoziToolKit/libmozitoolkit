use mtk_resource::{
    AnimationFrame, AnimationMetadata, AtlasDefinition, MemoryPack, ResourceLocation,
    ResourcePackStack,
};

#[test]
fn test_resource_location_parsing() {
    let loc = ResourceLocation::parse("minecraft:block/stone").unwrap();
    assert_eq!(loc.namespace, "minecraft");
    assert_eq!(loc.path, "block/stone");
    assert_eq!(loc.as_string(), "minecraft:block/stone");
    assert_eq!(
        loc.to_asset_path("textures", "png"),
        "assets/minecraft/textures/block/stone.png"
    );

    // Missing namespace defaults to minecraft
    let loc2 = ResourceLocation::parse("item/diamond_sword").unwrap();
    assert_eq!(loc2.namespace, "minecraft");
    assert_eq!(loc2.path, "item/diamond_sword");

    // Reversible from asset path
    let loc3 = ResourceLocation::from_asset_path(
        "assets/minecraft/textures/block/oak_planks.png",
        "textures",
        "png",
    )
    .unwrap();
    assert_eq!(loc3.as_string(), "minecraft:block/oak_planks");
}

#[test]
fn test_mcmeta_parsing() {
    let json_data = r#"{
        "animation": {
            "frametime": 2,
            "interpolate": true,
            "frames": [
                0,
                1,
                {"index": 2, "time": 4}
            ]
        }
    }"#;
    let v: serde_json::Value = serde_json::from_str(json_data).unwrap();
    let anim: AnimationMetadata = serde_json::from_value(v["animation"].clone()).unwrap();
    assert_eq!(anim.frametime, 2);
    assert!(anim.interpolate);
    let frames = anim.frames.unwrap();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0], AnimationFrame::Index(0));
    assert_eq!(frames[2], AnimationFrame::Detailed { index: 2, time: 4 });
}

#[test]
fn test_atlas_definition_parsing() {
    let json_data = r#"{
        "sources": [
            {
                "type": "minecraft:directory",
                "source": "block",
                "prefix": "block/"
            },
            {
                "type": "minecraft:single",
                "resource": "minecraft:entity/bell/bell_body"
            }
        ]
    }"#;
    let def = AtlasDefinition::parse_json(json_data).unwrap();
    assert_eq!(def.sources.len(), 2);
}

#[test]
fn test_pack_stack_granular_fallback() {
    let mut top_pack = MemoryPack::new("top_pack");
    let mut bottom_pack = MemoryPack::new("bottom_pack");

    // Top pack only provides diamond_ore_s.png
    top_pack.insert(
        "assets/minecraft/textures/block/diamond_ore_s.png",
        vec![1, 2, 3],
    );

    // Bottom pack provides diamond_ore.png and diamond_ore_n.png
    bottom_pack.insert(
        "assets/minecraft/textures/block/diamond_ore.png",
        vec![10, 20],
    );
    bottom_pack.insert(
        "assets/minecraft/textures/block/diamond_ore_n.png",
        vec![30, 40],
    );

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(bottom_pack));
    stack.push_pack(Box::new(top_pack));

    let loc = ResourceLocation::parse("minecraft:block/diamond_ore").unwrap();
    let companions = stack.resolve_pbr_companions(&loc);

    // Albedo & Normal from bottom pack, Specular from top pack
    assert_eq!(companions.albedo, Some(vec![10, 20]));
    assert_eq!(companions.normal, Some(vec![30, 40]));
    assert_eq!(companions.specular, Some(vec![1, 2, 3]));
}

#[test]
fn test_ctm_properties_parsing() {
    let prop_content = r#"
matchTiles=stone dirt
method=ctm
tiles=0-46
connect=block
faces=sides
weight=10
"#;
    let rule = mtk_resource::CtmRule::parse_properties(
        "optifine/ctm/stone_ctm.properties",
        "minecraft",
        prop_content,
    )
    .unwrap();

    assert_eq!(rule.name, "stone_ctm");
    assert_eq!(rule.priority, 10);
    assert_eq!(
        rule.method,
        mtk_resource::CtmMethod::Full { inner_seams: false }
    );
    assert_eq!(rule.tiles.len(), 47);
    assert_eq!(
        rule.tiles[0],
        Some(ResourceLocation::parse("minecraft:optifine/ctm/0").unwrap())
    );
    assert_eq!(
        rule.tiles[46],
        Some(ResourceLocation::parse("minecraft:optifine/ctm/46").unwrap())
    );
    assert_eq!(rule.match_tiles.len(), 2);
    assert_eq!(rule.connect_logic, mtk_resource::ConnectLogic::SameBlock);
}
