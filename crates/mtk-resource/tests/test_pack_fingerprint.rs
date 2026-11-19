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
