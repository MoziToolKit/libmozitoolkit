//! The stack fingerprint is the cache invalidation key: it must change when a pack's *content*
//! changes, not only when its name does.

use mtk_resource::{DirectoryPack, MemoryPack, ResourcePackStack};
use std::fs;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mtk_fp_{tag}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, rel: &str, bytes: &[u8]) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn directory_pack_fingerprint_tracks_content_not_just_the_name() {
    let dir = temp_dir("dir");
    write(&dir, "assets/minecraft/textures/block/stone.png", b"first");

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("MyPack", &dir)));
    let before = stack.compute_stack_fingerprint();

    // Edit the pack in place: same directory, same pack name, different content.
    write(
        &dir,
        "assets/minecraft/textures/block/stone.png",
        b"second-version",
    );

    let after = stack.compute_stack_fingerprint();
    assert_ne!(
        before, after,
        "editing a pack must invalidate its fingerprint, otherwise the cache serves stale assets"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn adding_a_file_to_a_directory_pack_changes_the_fingerprint() {
    let dir = temp_dir("add");
    write(&dir, "assets/minecraft/textures/block/stone.png", b"stone");

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("MyPack", &dir)));
    let before = stack.compute_stack_fingerprint();

    write(&dir, "assets/minecraft/textures/block/dirt.png", b"dirt");

    assert_ne!(before, stack.compute_stack_fingerprint());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pack_order_still_changes_the_fingerprint() {
    let a = MemoryPack::new("A");
    let b = MemoryPack::new("B");

    let mut forward = ResourcePackStack::new();
    forward.push_pack(Box::new(a.clone()));
    forward.push_pack(Box::new(b.clone()));

    let mut reverse = ResourcePackStack::new();
    reverse.push_pack(Box::new(b));
    reverse.push_pack(Box::new(a));

    assert_ne!(
        forward.compute_stack_fingerprint(),
        reverse.compute_stack_fingerprint(),
        "two packs with the same names in a different order must not share a cache"
    );
}

/// The host decides READY/STALE on the UI thread from paths alone, before loading any archive.
/// That fast path must produce exactly the fingerprint the loaded stack produces.
#[test]
fn path_fingerprint_matches_the_loaded_stack() {
    let dir = temp_dir("parity");
    write(&dir, "assets/minecraft/textures/block/stone.png", b"stone");
    write(&dir, "pack.mcmeta", b"{\"pack\":{\"pack_format\":15}}");

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("parity", &dir)));

    let from_paths = ResourcePackStack::compute_fingerprint_from_paths([dir.to_str().unwrap()]);

    assert_eq!(
        from_paths,
        stack.compute_stack_fingerprint(),
        "path-only and loaded-stack fingerprints must agree; the host picks a cache by the former"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Archives are identified by the archive's own size and mtime. The path-only fast path must
/// agree with a stack that loaded the same file, otherwise the host would resolve a cache built
/// from different bytes.
#[test]
fn path_fingerprint_matches_the_loaded_archive_stack() {
    use std::io::Write;

    let dir = temp_dir("zip_parity");
    let archive_path = dir.join("pack.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("assets/minecraft/textures/block/stone.png", options)
            .unwrap();
        zip.write_all(b"stone").unwrap();
        zip.finish().unwrap();
    }

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(
        mtk_resource::ZipPack::from_file("pack", &archive_path).unwrap(),
    ));

    let from_paths =
        ResourcePackStack::compute_fingerprint_from_paths([archive_path.to_str().unwrap()]);

    assert_eq!(
        from_paths,
        stack.compute_stack_fingerprint(),
        "an archive must fingerprint identically from its path and from the loaded stack"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The host computes the fingerprint from paths before loading any pack, so the overlay handling
/// (overlay subtrees hidden from the root listing, overlay files counted) must match the stack.
#[test]
fn path_fingerprint_matches_the_loaded_stack_for_overlay_packs() {
    let dir = temp_dir("overlay_parity");
    write(&dir, "assets/minecraft/textures/block/stone.png", b"root");
    write(
        &dir,
        "pack.mcmeta",
        format!(
            r#"{{"overlays":{{"entries":[{{"formats":{},"directory":"overlay_1"}}]}}}}"#,
            mtk_resource::CURRENT_CLIENT_RESOURCE_FORMAT
        )
        .as_bytes(),
    );
    write(
        &dir,
        "overlay_1/assets/minecraft/textures/block/stone.png",
        b"overlay",
    );

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("overlay_parity", &dir)));

    let from_paths = ResourcePackStack::compute_fingerprint_from_paths([dir.to_str().unwrap()]);
    let loaded = stack.compute_stack_fingerprint();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        from_paths, loaded,
        "path-only and loaded-stack fingerprints must agree for packs with overlays too"
    );
}

/// The constant is hand-written, so pin it to the shipped `version.json`.
#[test]
fn the_client_resource_format_matches_the_unpacked_jar() {
    let Some(assets) = mtk_testkit::real_assets_root() else {
        mtk_testkit::skip("no unpacked jar (set MTK_TEST_ASSETS or place ../Minecraft_Source)");
        return;
    };
    let version_json = assets.join("version.json");
    let Ok(bytes) = fs::read(&version_json) else {
        mtk_testkit::skip("version.json not present in the assets root");
        return;
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("version.json");
    let major = value["pack_version"]["resource_major"]
        .as_u64()
        .expect("pack_version.resource_major");
    assert_eq!(
        major as u32,
        mtk_resource::CURRENT_CLIENT_RESOURCE_FORMAT,
        "CURRENT_CLIENT_RESOURCE_FORMAT must track version.json's resource_major"
    );
}
