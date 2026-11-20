//! "Not in this pack" and "in this pack but unreadable" are different answers.
//!
//! A pack that fails to read a file (permissions, a corrupt entry, an I/O error) must say so. If
//! the failure is reported as absence, the stack silently serves a lower-priority pack's file -
//! the user repaints a texture in their pack, the read fails, and the render shows vanilla with no
//! hint that anything went wrong.

use mtk_resource::{DirectoryPack, MemoryPack, ResourcePack, ResourcePackStack};
use std::fs;
use std::path::PathBuf;

const ASSET: &str = "assets/minecraft/textures/block/stone.png";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mtk_pack_read_{tag}_{}_{:?}",
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

/// The high-priority pack holds the file but cannot read it; the low-priority pack also holds it.
#[cfg(unix)]
#[test]
fn an_unreadable_asset_does_not_fall_back_to_a_lower_priority_pack() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("unreadable");
    write(&dir, ASSET, b"from-the-user-pack");
    fs::set_permissions(dir.join(ASSET), fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read(dir.join(ASSET)).is_ok() {
        // Running as root: permissions do not apply, so this scenario cannot be built here.
        let _ = fs::remove_dir_all(&dir);
        mtk_testkit::skip("file permissions are not enforced for this user (root?)");
        return;
    }

    let mut fallback = MemoryPack::new("vanilla");
    fallback.insert(ASSET, b"from-vanilla");

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(fallback));
    stack.push_pack(Box::new(DirectoryPack::new("user", &dir)));

    let result = stack.open_asset_raw(ASSET);
    let _ = fs::remove_dir_all(&dir);

    match result {
        Err(mtk_resource::ResourceError::ReadFailed { pack, path, .. }) => {
            assert_eq!(pack, "user", "the error must name the pack that failed");
            assert_eq!(path, ASSET, "the error must name the file that failed");
        }
        Ok(bytes) => panic!(
            "an unreadable file must not resolve to the lower-priority pack's bytes: {:?}",
            bytes.map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        ),
        Err(other) => panic!("unexpected error kind: {other}"),
    }
}

/// A path that exists but is not a file is genuinely absent, not an error.
#[test]
fn a_directory_at_the_asset_path_is_absent() {
    let dir = temp_dir("directory");
    fs::create_dir_all(dir.join(ASSET)).unwrap();
    let pack = DirectoryPack::new("user", &dir);

    let result = pack
        .open(ASSET)
        .expect("a directory is absent, not unreadable");
    let _ = fs::remove_dir_all(&dir);

    assert!(result.is_none(), "a directory is not a readable asset");
}

/// A plain miss stays a plain miss.
#[test]
fn a_missing_asset_is_absent() {
    let dir = temp_dir("missing");
    let pack = DirectoryPack::new("user", &dir);
    let result = pack.open(ASSET).expect("missing is not an error");
    let _ = fs::remove_dir_all(&dir);
    assert!(result.is_none());
}
