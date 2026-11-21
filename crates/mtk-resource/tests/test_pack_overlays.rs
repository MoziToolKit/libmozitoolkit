//! `pack.mcmeta` overlays: a pack can ship per-version directories that shadow its root.
//!
//! Reference: `Minecraft_Source/net/minecraft/server/packs/OverlayMetadataSection.java`
//! (`overlays.entries[].{formats, directory}`, applicability tested against the *current* client
//! resource format, `version.json` -> `pack_version.resource_major` = 88 for 26.2) and
//! `CompositePackResources` (`reverse(overlays) + primary`), so the last declared applicable
//! overlay wins over earlier ones and over the pack root.

use mtk_resource::{DirectoryPack, MemoryPack, ResourcePack, ResourcePackStack, ZipPack};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const STONE: &str = "assets/minecraft/textures/block/stone.png";
const CURRENT: u32 = 88;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mtk_overlay_{tag}_{}_{:?}",
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

fn dir_pack(root: &Path) -> DirectoryPack {
    DirectoryPack::new("overlay_test", root)
}

/// Root ships `stone.png`; an applicable overlay directory ships a different one.
#[test]
fn an_applicable_overlay_shadows_the_pack_root() {
    let dir = temp_dir("shadow");
    write(&dir, STONE, b"root");
    write(
        &dir,
        "pack.mcmeta",
        format!(
            r#"{{"pack":{{"pack_format":{CURRENT}}},"overlays":{{"entries":[{{"formats":{CURRENT},"directory":"overlay_1"}}]}}}}"#
        )
        .as_bytes(),
    );
    write(&dir, &format!("overlay_1/{STONE}"), b"overlay");

    let pack = dir_pack(&dir);
    let bytes = pack.open(STONE).expect("read").expect("present");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        bytes, b"overlay",
        "an applicable overlay must shadow the pack root"
    );
}

/// `CompositePackResources` reverses the overlay list, so the last declared applicable entry wins.
#[test]
fn a_later_overlay_wins_over_an_earlier_one() {
    let dir = temp_dir("order");
    write(&dir, STONE, b"root");
    write(
        &dir,
        "pack.mcmeta",
        format!(
            r#"{{"overlays":{{"entries":[
                {{"formats":{CURRENT},"directory":"overlay_1"}},
                {{"formats":{CURRENT},"directory":"overlay_2"}}]}}}}"#
        )
        .as_bytes(),
    );
    write(&dir, &format!("overlay_1/{STONE}"), b"first");
    write(&dir, &format!("overlay_2/{STONE}"), b"second");

    let pack = dir_pack(&dir);
    let bytes = pack.open(STONE).expect("read").expect("present");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        bytes, b"second",
        "the later declared overlay has the higher priority"
    );
}

#[test]
fn an_inapplicable_overlay_is_ignored() {
    let dir = temp_dir("inapplicable");
    write(&dir, STONE, b"root");
    write(
        &dir,
        "pack.mcmeta",
        br#"{"overlays":{"entries":[{"formats":[1,2],"directory":"old_version"}]}}"#,
    );
    write(&dir, &format!("old_version/{STONE}"), b"old");

    let pack = dir_pack(&dir);
    let bytes = pack.open(STONE).expect("read").expect("present");
    let listed = pack.list_files("assets/");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        bytes, b"root",
        "a non-matching format range must be ignored"
    );
    assert!(
        !listed.iter().any(|path| path.contains("old_version")),
        "overlay directories must not leak into listings: {listed:?}"
    );
}

/// `formats` accepts the shapes Mojang's codec accepts: an int, a `[min, max]` pair, or a
/// `min_inclusive`/`max_inclusive` object.
#[test]
fn all_formats_shapes_are_understood() {
    for spec in [
        CURRENT.to_string(),
        format!("[1,{CURRENT}]"),
        r#"{"min_inclusive":1,"max_inclusive":88}"#.to_string(),
    ] {
        let dir = temp_dir("shapes");
        write(&dir, STONE, b"root");
        write(
            &dir,
            "pack.mcmeta",
            format!(
                r#"{{"overlays":{{"entries":[{{"formats":{spec},"directory":"overlay_1"}}]}}}}"#
            )
            .as_bytes(),
        );
        write(&dir, &format!("overlay_1/{STONE}"), b"overlay");

        let pack = dir_pack(&dir);
        let bytes = pack.open(STONE).expect("read").expect("present");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(bytes, b"overlay", "formats {spec} should be applicable");
    }
}

#[test]
fn overlay_entries_are_listed_without_the_overlay_prefix() {
    let dir = temp_dir("listing");
    write(&dir, STONE, b"root");
    write(
        &dir,
        "pack.mcmeta",
        format!(
            r#"{{"overlays":{{"entries":[{{"formats":{CURRENT},"directory":"overlay_1"}}]}}}}"#
        )
        .as_bytes(),
    );
    write(
        &dir,
        "overlay_1/assets/minecraft/textures/block/extra.png",
        b"extra",
    );

    let pack = dir_pack(&dir);
    let listed = pack.list_files("assets/");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        listed.contains(&STONE.to_string()),
        "root entry missing: {listed:?}"
    );
    assert!(
        listed.contains(&"assets/minecraft/textures/block/extra.png".to_string()),
        "overlay entry must be listed as a normal asset path: {listed:?}"
    );
    assert!(
        !listed.iter().any(|path| path.starts_with("overlay_1/")),
        "the overlay directory is not part of the asset namespace: {listed:?}"
    );
}

/// Editing a file inside an applicable overlay must invalidate the cache fingerprint.
#[test]
fn overlay_content_changes_the_fingerprint() {
    let dir = temp_dir("fingerprint");
    write(&dir, STONE, b"root");
    write(
        &dir,
        "pack.mcmeta",
        format!(
            r#"{{"overlays":{{"entries":[{{"formats":{CURRENT},"directory":"overlay_1"}}]}}}}"#
        )
        .as_bytes(),
    );
    write(&dir, &format!("overlay_1/{STONE}"), b"overlay-v1");

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(dir_pack(&dir)));
    let before = stack.compute_stack_fingerprint();

    write(&dir, &format!("overlay_1/{STONE}"), b"overlay-v2-longer");
    let after = stack.compute_stack_fingerprint();
    let _ = fs::remove_dir_all(&dir);

    assert_ne!(
        before, after,
        "an overlay edit must invalidate the fingerprint, otherwise the cache serves stale assets"
    );
}

#[test]
fn archives_support_overlays_too() {
    let dir = temp_dir("zip");
    let archive_path = dir.join("pack.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mcmeta = format!(
            r#"{{"overlays":{{"entries":[{{"formats":{CURRENT},"directory":"overlay_1"}}]}}}}"#
        );
        for (name, body) in [
            ("pack.mcmeta", mcmeta.as_bytes()),
            (STONE, b"root"),
            (
                "overlay_1/assets/minecraft/textures/block/stone.png",
                b"overlay",
            ),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    let pack = ZipPack::from_file("overlay_zip", &archive_path).expect("open archive");
    let bytes = pack.open(STONE).expect("read").expect("present");
    let listed = pack.list_files("assets/");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        bytes, b"overlay",
        "overlay entries must shadow the archive root"
    );
    assert!(
        listed.contains(&STONE.to_string()) && !listed.iter().any(|p| p.starts_with("overlay_1/")),
        "archive overlays must be listed as normal paths: {listed:?}"
    );
}

#[test]
fn packs_without_overlays_are_unchanged() {
    let dir = temp_dir("plain");
    write(&dir, STONE, b"root");
    let pack = dir_pack(&dir);
    let bytes = pack.open(STONE).expect("read").expect("present");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(bytes, b"root");
}

#[test]
fn a_memory_pack_still_behaves() {
    let mut pack = MemoryPack::new("mem");
    pack.insert(STONE, b"root");
    assert_eq!(pack.open(STONE).expect("read"), Some(b"root".to_vec()));
}
