//! A pack file that exists but cannot be read must fail the bake.
//!
//! Reporting it as "missing" would bake the block with a lower-priority pack's model (or with the
//! fallback cube) and cache that result, so the user would see wrong geometry forever with no
//! indication that their pack was unreadable.

#![cfg(unix)]

use libmtk::{precompile_all_assets, PrecompileConfig};
use mtk_resource::{DirectoryPack, ResourcePackStack};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const MODEL: &str = "assets/minecraft/models/block/read_test.json";

fn png(color: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba(color));
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    bytes
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// A pack on disk with one blockstate, one model and one texture.
fn build_pack(root: &Path) {
    write(
        root,
        MODEL,
        br##"{"textures":{"all":"minecraft:block/read_test"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#all","cullface":"up"}}}]}"##,
    );
    write(
        root,
        "assets/minecraft/blockstates/read_test.json",
        br##"{"variants":{"":{"model":"minecraft:block/read_test"}}}"##,
    );
    write(
        root,
        "assets/minecraft/textures/block/read_test.png",
        &png([10, 20, 30, 255]),
    );
}

fn config() -> PrecompileConfig {
    PrecompileConfig {
        max_atlas_width: 1024,
        max_atlas_height: 1024,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: None,
        resolver_overrides: None,
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mtk_pack_fail_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn stack_for(root: &Path) -> ResourcePackStack {
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("user", root)));
    stack
}

#[test]
fn a_model_that_cannot_be_read_fails_the_precompile() {
    let pack_dir = temp_dir("model");
    build_pack(&pack_dir);

    // Baseline: the same pack bakes fine while every file is readable.
    let reader_check = fs::read(pack_dir.join(MODEL)).expect("model readable");
    assert!(!reader_check.is_empty());

    fs::set_permissions(pack_dir.join(MODEL), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(pack_dir.join(MODEL)).is_ok() {
        let _ = fs::remove_dir_all(&pack_dir);
        mtk_testkit::skip("file permissions are not enforced for this user (root?)");
        return;
    }

    let cache_dir = temp_dir("model_cache");
    let result = precompile_all_assets(&stack_for(&pack_dir), &cache_dir, &config());

    let _ = fs::remove_dir_all(&pack_dir);
    let _ = fs::remove_dir_all(&cache_dir);

    match result {
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("read_test.json"),
                "the error must name the unreadable file, got: {message}"
            );
        }
        Ok(_) => panic!("an unreadable model must fail the bake instead of silently skipping it"),
    }
}

#[test]
fn a_failing_pack_is_not_replaced_by_a_lower_priority_pack() {
    let user_dir = temp_dir("user");
    build_pack(&user_dir);

    // Lower priority pack provides the same model with different geometry.
    let mut vanilla = mtk_resource::MemoryPack::new("vanilla");
    vanilla.insert(
        MODEL,
        br##"{"textures":{"all":"minecraft:block/read_test"},"elements":[{"from":[0,0,0],"to":[8,8,8],"faces":{"up":{"texture":"#all","cullface":"up"}}}]}"##
            .to_vec(),
    );

    let mut stack = ResourcePackStack::new();
    stack.append_pack(Box::new(vanilla));
    stack.push_pack(Box::new(DirectoryPack::new("user", &user_dir)));

    fs::set_permissions(user_dir.join(MODEL), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(user_dir.join(MODEL)).is_ok() {
        let _ = fs::remove_dir_all(&user_dir);
        mtk_testkit::skip("file permissions are not enforced for this user (root?)");
        return;
    }

    let result = stack.open_model_raw("minecraft:block/read_test");
    let _ = fs::remove_dir_all(&user_dir);

    assert!(
        result.is_err(),
        "the stack must report the read failure instead of returning the lower pack's model"
    );
}

/// Sanity: the fixture itself bakes when nothing is unreadable, so the tests above fail for the
/// right reason.
#[test]
fn the_fixture_bakes_when_readable() {
    let pack_dir = temp_dir("readable");
    build_pack(&pack_dir);
    let cache_dir = temp_dir("readable_cache");

    let result = precompile_all_assets(&stack_for(&pack_dir), &cache_dir, &config());

    let _ = fs::remove_dir_all(&pack_dir);
    let _ = fs::remove_dir_all(&cache_dir);

    let result = result.expect("fixture must bake");
    assert!(result.success);
    assert!(result.baked_models >= 1);
}
