//! Parity with Continuity's CTM properties reader.
//!
//! Reference: `../Continuity/src/main/java/me/pepperbell/continuity/client/...`
//! (`ContinuityClient` method registry, `BaseCtmProperties`, `BasicConnectingCtmProperties`,
//! `RandomCtmProperties`, `RepeatCtmProperties`, `CtmPropertiesLoader`).
//!
//! The decoder was written against OptiFine's docs; these tests pin the behaviour a pack author
//! actually gets from the mod that defines the format today.

use glam::IVec3;
use mtk_core::Direction;
use mtk_resource::{CtmMethod, CtmRule};

fn parse(body: &str) -> Option<CtmRule> {
    CtmRule::parse_properties(
        "assets/minecraft/optifine/ctm/test.properties",
        "minecraft",
        body,
    )
}

fn rule(body: &str) -> CtmRule {
    parse(body).unwrap_or_else(|| panic!("rule must parse:\n{body}"))
}

/// `ContinuityClient` registers `bookshelf` as an alias of `horizontal`, and `h+v` / `v+h` as
/// aliases of the two 7-tile grid methods.
#[test]
fn method_aliases_from_the_mod_registry_are_accepted() {
    assert!(matches!(
        rule("method=bookshelf\nmatchTiles=bookshelf\ntiles=0-3\n").method,
        CtmMethod::Horizontal
    ));
    assert!(matches!(
        rule("method=h+v\nmatchTiles=stone\ntiles=0-6\n").method,
        CtmMethod::HorizontalVertical
    ));
    assert!(matches!(
        rule("method=v+h\nmatchTiles=stone\ntiles=0-6\n").method,
        CtmMethod::VerticalHorizontal
    ));
}

/// Continuity logs "Unknown 'method'" and drops the whole rule. Falling back to the 47-tile
/// method silently applies the wrong algorithm (and usually no tile at all).
#[test]
fn an_unknown_method_drops_the_rule() {
    assert!(
        parse("method=bogus\nmatchTiles=stone\ntiles=0-46\n").is_none(),
        "an unknown method must not silently become full CTM"
    );
}

#[test]
fn a_rule_without_tiles_is_dropped() {
    assert!(
        parse("method=ctm\nmatchTiles=stone\n").is_none(),
        "Continuity: missing 'tiles' invalidates the rule"
    );
}

/// Continuity validates the tile amount per method (`TileAmountValidator`): a 47-tile method with
/// four tiles is a broken rule, not a rule that silently does nothing.
#[test]
fn the_tile_amount_is_validated_per_method() {
    for (method, tiles, ok) in [
        ("ctm", "0-45", false), // needs at least 47
        ("ctm", "0-46", true),
        ("ctm_compact", "0-3", false), // needs at least 5
        ("ctm_compact", "0-4", true),
        ("horizontal", "0-3", true),
        ("horizontal", "0-4", false), // exactly 4
        ("vertical", "0-2", false),
        ("horizontal+vertical", "0-6", true),
        ("horizontal+vertical", "0-5", false),
        ("vertical+horizontal", "0-6", true),
        ("top", "0", true),
        ("top", "0-1", false),
        ("fixed", "0", true),
        ("fixed", "0-1", false),
    ] {
        let body = format!("method={method}\nmatchTiles=stone\ntiles={tiles}\n");
        assert_eq!(
            parse(&body).is_some(),
            ok,
            "method={method} tiles={tiles} should {} parse",
            if ok { "" } else { "not" }
        );
    }
}

/// `tiles` is split on `[ ,]` and accepts the `<skip>.png` / `<default>.png` spellings.
#[test]
fn tiles_accept_commas_and_suffixed_sentinels() {
    let comma = rule("method=random\nmatchTiles=stone\ntiles=0,1,2,3\n");
    assert_eq!(comma.tiles.len(), 4, "commas separate tiles like spaces");

    // `random` has no tile-count requirement, so the sentinels are what the test can observe.
    let sentinels = rule("method=random\nmatchTiles=stone\ntiles=0 <skip>.png 2 <default>.png\n");
    assert_eq!(sentinels.tiles.len(), 4);
    assert!(sentinels.tiles[1].is_none(), "<skip> means no tile");
    assert!(sentinels.tiles[3].is_none(), "<default> means no tile");
}

/// `biomes=!a b` excludes the listed biomes instead of listing them.
#[test]
fn a_negated_biome_list_excludes_biomes() {
    let rule = rule("method=ctm\nmatchTiles=stone\ntiles=0-46\nbiomes=!minecraft:plains\n");
    let face = Direction::North;
    let pos = IVec3::new(0, 64, 0);
    let stone_tile = mtk_resource::ResourceLocation::new("minecraft", "block/stone");
    assert!(
        !rule.matches_block(
            "minecraft:stone",
            face,
            pos,
            Some(&stone_tile),
            Some("minecraft:plains")
        ),
        "the negated biome must be excluded"
    );
    assert!(
        rule.matches_block(
            "minecraft:stone",
            face,
            pos,
            Some(&stone_tile),
            Some("minecraft:forest")
        ),
        "every other biome stays eligible"
    );
}

/// An unparsable `faces` list invalidates the rule; `all` short-circuits to every face.
#[test]
fn faces_parsing_matches_the_mod() {
    assert!(
        parse("method=ctm\nmatchTiles=stone\ntiles=0-46\nfaces=sideways\n").is_none(),
        "an unknown face invalidates the rule instead of silently meaning every face"
    );
    let all = rule("method=ctm\nmatchTiles=stone\ntiles=0-46\nfaces=all north\n");
    for face in [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ] {
        assert!(all.faces.contains_dir(face), "faces=all covers {face:?}");
    }
}

/// `innerSeams=true` means "connect only when the block behind the neighbour does not connect":
/// the seam is interior to a thick wall, so its surface must not draw connected borders.
#[test]
fn inner_seams_needs_the_block_behind_the_neighbour() {
    let face = Direction::North; // -Z; the face's in-plane neighbours are up/down/east/west
    let pos = IVec3::new(0, 64, 0);
    let tangents = face.canonical_face_tangents();
    let stone = |p: IVec3| {
        matches!((p.x, p.y, p.z), (0, 65, 0) | (1, 64, 0) | (1, 65, 0)).then_some("minecraft:stone")
    };
    // Same in-plane neighbours, but the wall continues one block deeper (z = -1).
    let thick_stone = |p: IVec3| {
        stone(p).or_else(|| {
            matches!((p.x, p.y, p.z), (0, 65, -1) | (1, 64, -1) | (1, 65, -1))
                .then_some("minecraft:stone")
        })
    };

    let with_seams = rule("method=ctm\nmatchTiles=stone\ntiles=0-46\ninnerSeams=true\n");
    let without_seams = rule("method=ctm\nmatchTiles=stone\ntiles=0-46\n");

    let shell_tile = with_seams
        .solve_tile_with_tangents("minecraft:stone", face, tangents, pos, &stone)
        .expect("tile");
    let thick_tile = with_seams
        .solve_tile_with_tangents("minecraft:stone", face, tangents, pos, &thick_stone)
        .expect("tile");

    assert_eq!(
        shell_tile,
        without_seams
            .solve_tile_with_tangents("minecraft:stone", face, tangents, pos, &stone)
            .expect("tile"),
        "a one-block shell connects exactly like innerSeams=false"
    );
    assert_ne!(
        thick_tile, shell_tile,
        "with stone behind the neighbours the seam is interior, so it must not connect"
    );
}
