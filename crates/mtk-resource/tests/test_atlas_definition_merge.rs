//! Every pack's `atlases/*.json` contributes sources, in stack order.
//!
//! Reference: `Minecraft_Source/net/minecraft/client/renderer/texture/atlas/SpriteSourceList.java`
//!
//! ```java
//! for (Resource entry : resourceManager.getResourceStack(resourceId)) {
//!    loaders.addAll(SpriteSources.FILE_CODEC.parse(contents).getOrThrow());
//! }
//! ```
//!
//! `getResourceStack` yields the file from each pack, lowest priority first, so a pack that only
//! *adds* sprites does not have to repeat the vanilla source list — and a higher pack's `filter`
//! can remove the sprites a lower pack contributed.

use mtk_resource::{AtlasDefinition, AtlasSource, MemoryPack, ResourceLocation, ResourcePackStack};

const ATLAS: &str = "assets/minecraft/atlases/blocks.json";

fn definition_json(sources: &str) -> Vec<u8> {
    format!(r#"{{"sources":[{sources}]}}"#).into_bytes()
}

/// Low priority pack: the vanilla-style block directory plus one extra sprite.
fn base_pack() -> MemoryPack {
    let mut pack = MemoryPack::new("vanilla");
    pack.insert(
        "assets/minecraft/textures/block/stone.png",
        b"stone".to_vec(),
    );
    pack.insert("assets/minecraft/textures/block/dirt.png", b"dirt".to_vec());
    pack.insert(
        "assets/minecraft/textures/custom/glow.png",
        b"glow".to_vec(),
    );
    pack.insert(
        ATLAS,
        definition_json(r#"{"type":"minecraft:directory","prefix":"block/","source":"block"}"#),
    );
    pack
}

/// High priority pack: only its own additions, no repeated vanilla list.
fn addon_pack() -> MemoryPack {
    let mut pack = MemoryPack::new("addon");
    pack.insert(
        "assets/minecraft/textures/custom/glow.png",
        b"glow".to_vec(),
    );
    pack.insert(
        ATLAS,
        definition_json(r#"{"type":"minecraft:single","resource":"minecraft:custom/glow"}"#),
    );
    pack
}

fn stack(low: MemoryPack, high: MemoryPack) -> ResourcePackStack {
    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(low));
    stack.push_pack(Box::new(high));
    stack
}

fn collected(stack: &ResourcePackStack) -> Vec<String> {
    let def = stack
        .load_atlas_definition(&ResourceLocation::vanilla("blocks"))
        .expect("definition");
    let mut ids: Vec<String> = stack
        .collect_sprites_for_atlas(&def)
        .expect("collect")
        .into_iter()
        .map(|sprite| sprite.sprite_id.as_string())
        .collect();
    ids.sort();
    ids
}

#[test]
fn a_pack_that_only_adds_sprites_keeps_the_lower_pack_sources() {
    let ids = collected(&stack(base_pack(), addon_pack()));
    for expected in [
        "minecraft:block/stone",
        "minecraft:block/dirt",
        "minecraft:custom/glow",
    ] {
        assert!(
            ids.contains(&expected.to_string()),
            "{expected} missing; a pack that only adds sources must not replace the whole list: {ids:?}"
        );
    }
}

/// `getResourceStack` is ordered lowest priority first, and `SpriteSourceList` appends in that
/// order, so the lower pack's sources come first and a filter in the higher pack sees them.
#[test]
fn sources_are_concatenated_from_low_to_high_priority() {
    let stack = stack(base_pack(), addon_pack());
    let def = stack
        .load_atlas_definition(&ResourceLocation::vanilla("blocks"))
        .expect("definition");
    assert_eq!(
        def.sources.len(),
        2,
        "both files contribute: {:?}",
        def.sources
    );
    assert!(matches!(def.sources[0], AtlasSource::Directory { .. }));
    assert!(matches!(def.sources[1], AtlasSource::Single { .. }));
}

#[test]
fn a_higher_pack_filter_can_remove_a_lower_pack_sprite() {
    let mut high = MemoryPack::new("addon");
    high.insert(
        ATLAS,
        definition_json(r#"{"type":"minecraft:filter","pattern":{"path":"block/dirt"}}"#),
    );

    let ids = collected(&stack(base_pack(), high));
    assert!(
        !ids.contains(&"minecraft:block/dirt".to_string()),
        "the addon pack filters dirt out; got {ids:?}"
    );
    assert!(
        ids.contains(&"minecraft:block/stone".to_string()),
        "the rest of the vanilla directory survives; got {ids:?}"
    );
}

/// Three packs: every file is read, and nothing is dropped because a higher pack overrides.
#[test]
fn three_packs_all_contribute() {
    let mut middle = MemoryPack::new("middle");
    middle.insert("assets/minecraft/textures/custom/mid.png", b"mid".to_vec());
    middle.insert(
        ATLAS,
        definition_json(r#"{"type":"minecraft:single","resource":"minecraft:custom/mid"}"#),
    );

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(base_pack()));
    stack.append_pack(Box::new(middle));
    stack.push_pack(Box::new(addon_pack()));

    let def = stack
        .load_atlas_definition(&ResourceLocation::vanilla("blocks"))
        .expect("definition");
    assert_eq!(
        def.sources.len(),
        3,
        "every pack's file contributes one source: {:?}",
        def.sources
    );
}

/// No pack ships the file: the caller still gets a NotFound so it can fall back to the built-in
/// default definition for the category.
#[test]
fn a_missing_definition_is_reported() {
    let mut pack = MemoryPack::new("empty");
    pack.insert(
        "assets/minecraft/textures/block/stone.png",
        b"stone".to_vec(),
    );
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));

    let result = stack.load_atlas_definition(&ResourceLocation::vanilla("blocks"));
    assert!(result.is_err(), "no pack has blocks.json");
}

#[test]
fn a_broken_definition_in_one_pack_is_an_error_not_a_silent_skip() {
    let mut broken = MemoryPack::new("broken");
    broken.insert(ATLAS, b"{ this is not json".to_vec());

    let stack = stack(base_pack(), broken);
    let result = stack.load_atlas_definition(&ResourceLocation::vanilla("blocks"));
    assert!(
        result.is_err(),
        "a pack shipping an unparseable atlas must be reported, not quietly dropped"
    );
}

#[test]
fn a_single_pack_behaves_as_before() {
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(base_pack()));
    let def: AtlasDefinition = stack
        .load_atlas_definition(&ResourceLocation::vanilla("blocks"))
        .expect("definition");
    assert_eq!(def.sources.len(), 1);
    assert_eq!(
        collected(&stack),
        vec![
            "minecraft:block/dirt".to_string(),
            "minecraft:block/stone".to_string()
        ]
    );
}
