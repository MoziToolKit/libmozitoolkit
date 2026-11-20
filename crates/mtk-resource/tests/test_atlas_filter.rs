//! Atlas `filter` sources are regex patterns over sprite ids, applied in list order.
//!
//! Reference: `Minecraft_Source/net/minecraft/client/renderer/texture/atlas/sources/SourceFilter.java`
//! (`pattern` is an `IdentifierPattern`: `{ namespace?: PATTERN, path?: PATTERN }` where PATTERN is a
//! Java regex matched with `Pattern.asPredicate()` = `matcher.find()`, i.e. a substring search), and
//! `SpriteSourceList.list()` which runs every source in list order against one shared sprite map
//! (`output.removeAll(...)`), so a filter only affects sprites collected *before* it.

use mtk_resource::{AtlasDefinition, MemoryPack, ResourceLocation, ResourcePackStack};

const STONE: &str = "minecraft:block/stone";
const DIAMOND_ORE: &str = "minecraft:block/diamond_ore";
const DIRT: &str = "minecraft:block/dirt";

fn pack_with_blocks() -> MemoryPack {
    let mut pack = MemoryPack::new("test");
    pack.insert(
        "assets/minecraft/textures/block/stone.png",
        b"stone".to_vec(),
    );
    pack.insert(
        "assets/minecraft/textures/block/diamond_ore.png",
        b"ore".to_vec(),
    );
    pack.insert("assets/minecraft/textures/block/dirt.png", b"dirt".to_vec());
    pack
}

fn collect(def: &AtlasDefinition, pack: MemoryPack) -> Vec<String> {
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));
    let mut ids: Vec<String> = stack
        .collect_sprites_for_atlas(def)
        .expect("collect")
        .into_iter()
        .map(|sprite| sprite.sprite_id.as_string())
        .collect();
    ids.sort();
    ids
}

fn definition(sources: &str) -> AtlasDefinition {
    AtlasDefinition::parse_json(&format!(r#"{{"sources":[{sources}]}}"#))
        .expect("atlas definition must parse; vanilla-shaped filters are data packs write")
}

#[test]
fn a_literal_filter_removes_only_the_named_sprite() {
    let def = definition(
        r#"{"type":"minecraft:directory","prefix":"block/","source":"block"},
           {"type":"minecraft:filter","pattern":{"path":"block/stone"}}"#,
    );
    let ids = collect(&def, pack_with_blocks());
    assert!(
        !ids.contains(&STONE.to_string()),
        "block/stone matched the filter and must be removed; got {ids:?}"
    );
    assert!(ids.contains(&DIAMOND_ORE.to_string()), "got {ids:?}");
    assert!(ids.contains(&DIRT.to_string()), "got {ids:?}");
}

/// A pattern is a regex (`find`, not anchored), exactly like `IdentifierPattern` in Minecraft.
#[test]
fn a_regex_filter_matches_like_minecraft() {
    let def = definition(
        r#"{"type":"minecraft:directory","prefix":"block/","source":"block"},
           {"type":"minecraft:filter","pattern":{"path":"block/.*_ore"}}"#,
    );
    let ids = collect(&def, pack_with_blocks());
    assert!(!ids.contains(&DIAMOND_ORE.to_string()), "got {ids:?}");
    assert!(ids.contains(&STONE.to_string()), "got {ids:?}");
    assert!(ids.contains(&DIRT.to_string()), "got {ids:?}");
}

#[test]
fn an_anchored_regex_only_matches_the_whole_path() {
    let def = definition(
        r#"{"type":"minecraft:directory","prefix":"block/","source":"block"},
           {"type":"minecraft:filter","pattern":{"path":"^block/stone$"}}"#,
    );
    let ids = collect(&def, pack_with_blocks());
    assert!(!ids.contains(&STONE.to_string()), "got {ids:?}");
    assert!(ids.contains(&DIAMOND_ORE.to_string()), "got {ids:?}");
}

/// Vanilla applies `filter` at its position in the list, not at the end.
#[test]
fn a_filter_before_the_directory_source_removes_nothing() {
    let def = definition(
        r#"{"type":"minecraft:filter","pattern":{"path":"block/stone"}},
           {"type":"minecraft:directory","prefix":"block/","source":"block"}"#,
    );
    let ids = collect(&def, pack_with_blocks());
    assert!(
        ids.contains(&STONE.to_string()),
        "a filter cannot remove sprites collected later; got {ids:?}"
    );
}

/// `IdentifierPattern` matches the namespace and the path independently.
#[test]
fn a_namespace_only_pattern_ignores_the_path() {
    let def = definition(
        r#"{"type":"minecraft:directory","prefix":"block/","source":"block"},
           {"type":"minecraft:filter","pattern":{"namespace":"test"}}"#,
    );
    let ids = collect(&def, pack_with_blocks());
    assert_eq!(
        ids.len(),
        3,
        "namespace `test` matches nothing; got {ids:?}"
    );
}

/// An invalid regex makes Minecraft drop the whole atlas definition (it logs and keeps the base
/// definition), so parsing must not accept it silently.
#[test]
fn an_invalid_regex_is_rejected() {
    let result = AtlasDefinition::parse_json(
        r#"{"sources":[{"type":"minecraft:filter","pattern":{"path":"block/[unclosed"}}]}"#,
    );
    assert!(
        result.is_err(),
        "an invalid regex must fail the atlas definition, not be treated literally"
    );
}

/// Minecraft's codec has `pattern` as a required field; a filter without it would match (and
/// therefore remove) everything.
#[test]
fn a_filter_without_a_pattern_is_rejected() {
    let result = AtlasDefinition::parse_json(r#"{"sources":[{"type":"minecraft:filter"}]}"#);
    assert!(
        result.is_err(),
        "a filter must carry a pattern; silently matching everything would delete the atlas"
    );
}

/// Sanity: the atlas location itself is never affected by a filter that names it.
#[test]
fn unrelated_sources_still_collect() {
    let def = definition(r#"{"type":"minecraft:directory","prefix":"block/","source":"block"}"#);
    let ids = collect(&def, pack_with_blocks());
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&ResourceLocation::parse(STONE).unwrap().as_string()));
}
