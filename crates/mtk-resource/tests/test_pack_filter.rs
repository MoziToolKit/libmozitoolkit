//! `pack.mcmeta`'s `filter.block` hides resources from the filtering pack *and everything below it*.
//!
//! Reference: `Minecraft_Source/net/minecraft/server/packs/resources/FallbackResourceManager.java`
//!
//! ```java
//! // getResource: the pack's own resource is checked first, then its filter stops the search
//! if (resource != null) return ...;
//! if (entry.isFiltered(location)) { LOGGER.warn(...); return Optional.empty(); }
//! // listResources: filterAll() prunes what the lower packs already contributed
//! entry.filterAll(topResourceForFileLocation.keySet());
//! ```
//!
//! Patterns are `IdentifierPattern`s (`{namespace?, path?}` regexes) matched against the resource
//! id (`minecraft:textures/block/stone.png`), not the physical asset path.

use mtk_resource::{DirectoryPack, MemoryPack, ResourcePack, ResourcePackStack};
use std::fs;
use std::path::{Path, PathBuf};

const STONE: &str = "assets/minecraft/textures/block/stone.png";
const DIRT: &str = "assets/minecraft/textures/block/dirt.png";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mtk_pack_filter_{tag}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn pack_with_filter(root: &Path, filter_json: &str) {
    write(
        root,
        "pack.mcmeta",
        format!(r#"{{"filter":{{"block":{filter_json}}}}}"#).as_bytes(),
    );
}

fn memory_pack(name: &str, entries: &[(&str, &[u8])]) -> MemoryPack {
    let mut pack = MemoryPack::new(name);
    for (path, bytes) in entries {
        pack.insert(*path, bytes.to_vec());
    }
    pack
}

/// The filtering pack does not ship the file: the lower pack's copy must disappear.
#[test]
fn a_filter_hides_a_lower_packs_resource() {
    let dir = temp_dir("hide_lower");
    pack_with_filter(&dir, r#"[{"path":"textures/block/"}]"#);

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(memory_pack(
        "vanilla",
        &[(STONE, b"vanilla stone"), (DIRT, b"vanilla dirt")],
    )));
    stack.push_pack(Box::new(DirectoryPack::new("hiding", &dir)));

    let stone = stack.open_asset_raw(STONE).expect("no read error");
    let listed = stack.list_asset_files("assets/minecraft/textures/");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        stone.is_none(),
        "the filter must hide the vanilla stone, got {stone:?}"
    );
    assert!(
        !listed.contains(&STONE.to_string()),
        "a filtered path must not be listed either: {listed:?}"
    );
}

/// Minecraft checks the pack's own resource before its filter, so a pack can ship a file and still
/// hide the lower copies.
#[test]
fn a_filtering_pack_keeps_its_own_copy() {
    let dir = temp_dir("own_copy");
    write(&dir, STONE, b"pack stone");
    pack_with_filter(&dir, r#"[{"path":"textures/block/"}]"#);

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(memory_pack("vanilla", &[(STONE, b"vanilla")])));
    stack.push_pack(Box::new(DirectoryPack::new("pack", &dir)));

    let bytes = stack.open_asset_raw(STONE).expect("read");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        bytes.as_deref(),
        Some(&b"pack stone"[..]),
        "the filtering pack's own resource wins over its filter"
    );
}

/// A filter only applies from its own position downwards: a higher pack's copy survives.
#[test]
fn a_filter_does_not_hide_a_higher_packs_resource() {
    let dir = temp_dir("below");
    pack_with_filter(&dir, r#"[{"path":"textures/block/"}]"#);

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(DirectoryPack::new("filtering", &dir)));
    stack.push_pack(Box::new(memory_pack("top", &[(STONE, b"top stone")])));

    let bytes = stack.open_asset_raw(STONE).expect("read");
    let listed = stack.list_asset_files("assets/");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(bytes.as_deref(), Some(&b"top stone"[..]));
    assert!(listed.contains(&STONE.to_string()), "{listed:?}");
}

/// Unrelated paths keep resolving.
#[test]
fn unrelated_paths_are_unaffected() {
    let dir = temp_dir("unrelated");
    pack_with_filter(&dir, r#"[{"path":"textures/block/stone"}]"#);

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(memory_pack(
        "vanilla",
        &[(STONE, b"stone"), (DIRT, b"dirt")],
    )));
    stack.push_pack(Box::new(DirectoryPack::new("pack", &dir)));

    let stone = stack.open_asset_raw(STONE).expect("read");
    let dirt = stack.open_asset_raw(DIRT).expect("read");
    let _ = fs::remove_dir_all(&dir);

    assert!(stone.is_none(), "stone was filtered");
    assert_eq!(dirt.as_deref(), Some(&b"dirt"[..]), "dirt is unrelated");
}

/// Patterns are regexes, and the namespace field matches on its own.
#[test]
fn regex_and_namespace_patterns_work() {
    let dir = temp_dir("regex");

    // Locations keep their extension, so the pattern matches `textures/block/diamond_ore.png`.
    pack_with_filter(&dir, r#"[{"path":"block/diamond_.*png$"}]"#);
    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(memory_pack(
        "vanilla",
        &[
            ("assets/minecraft/textures/block/diamond_ore.png", b"ore"),
            ("assets/minecraft/textures/block/stone.png", b"stone"),
        ],
    )));
    stack.push_pack(Box::new(DirectoryPack::new("regex", &dir)));
    let ore = stack.open_asset_raw("assets/minecraft/textures/block/diamond_ore.png");
    let stone = stack.open_asset_raw(STONE);
    let _ = fs::remove_dir_all(&dir);
    assert!(ore.expect("read").is_none(), "diamond_ore matched");
    assert!(stone.expect("read").is_some(), "stone did not match");

    let ns_dir = temp_dir("namespace");
    pack_with_filter(&ns_dir, r#"[{"namespace":"mymod"}]"#);
    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(memory_pack(
        "mod_pack",
        &[("assets/mymod/textures/block/thing.png", b"thing")],
    )));
    stack.push_pack(Box::new(DirectoryPack::new("ns", &ns_dir)));
    let mod_asset = stack.open_asset_raw("assets/mymod/textures/block/thing.png");
    let _ = fs::remove_dir_all(&ns_dir);
    assert!(
        mod_asset.expect("read").is_none(),
        "the whole mymod namespace was filtered"
    );
}

/// Archives carry the same metadata.
#[test]
fn archives_apply_their_filter_too() {
    use std::io::Write;

    let dir = temp_dir("zip");
    let archive_path = dir.join("filtering.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            (
                "pack.mcmeta",
                r#"{"filter":{"block":[{"path":"textures/block/"}]}}"#.as_bytes(),
            ),
            (
                "assets/minecraft/textures/block/stone.png",
                b"root" as &[u8],
            ),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(memory_pack("vanilla", &[(STONE, b"vanilla")])));
    stack.push_pack(Box::new(
        mtk_resource::ZipPack::from_file("zip", &archive_path).unwrap(),
    ));
    let bytes = stack.open_asset_raw(STONE).expect("read");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        bytes.as_deref(),
        Some(&b"root"[..]),
        "the archive's own copy wins over its filter"
    );
}

#[test]
fn a_pack_without_a_filter_hides_nothing() {
    let dir = temp_dir("none");
    write(&dir, "pack.mcmeta", br#"{"pack":{"pack_format":88}}"#);
    let pack = DirectoryPack::new("plain", &dir);
    assert!(pack.filter().is_none(), "no filter section -> no filter");
    let _ = fs::remove_dir_all(&dir);
}
