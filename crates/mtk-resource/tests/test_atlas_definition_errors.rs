//! A pack that ships a broken `atlases/*.json` must be reported.
//!
//! `load_atlas_category` falls back to the built-in definition for a category when *no pack* has
//! the file, which is correct. Swallowing a parse error through the same path silently discards
//! every source the packs declared - including the ones that parsed fine - and renders the atlas
//! without them.

use mtk_resource::{AtlasCategory, MemoryPack, ResourceLocation, ResourcePackStack};

const ATLAS: &str = "assets/minecraft/atlases/blocks.json";

#[test]
fn a_broken_atlas_definition_is_reported_by_the_category_loader() {
    let mut broken = MemoryPack::new("broken_pack");
    broken.insert(ATLAS, b"{ this is not json".to_vec());
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(broken));

    let result = stack.load_atlas_category(&AtlasCategory::Blocks);
    match result {
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("blocks.json") && message.contains("broken_pack"),
                "the error must name the file and the pack, got: {message}"
            );
        }
        Ok(def) => panic!(
            "a broken atlas definition must be reported, got {} sources",
            def.sources.len()
        ),
    }
}

/// No pack ships the file: the category's built-in definition is the correct fallback.
#[test]
fn a_missing_definition_still_falls_back_to_the_builtin_category() {
    let mut pack = MemoryPack::new("plain");
    pack.insert(
        "assets/minecraft/textures/block/stone.png",
        b"stone".to_vec(),
    );
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));

    let def = stack
        .load_atlas_category(&AtlasCategory::Blocks)
        .expect("a missing file falls back to the built-in definition");
    assert!(
        !def.sources.is_empty(),
        "the built-in definition must not be empty"
    );
}

/// An unreadable definition is a read failure, not a missing file: it must not fall back either.
#[cfg(unix)]
#[test]
fn an_unreadable_definition_is_reported() {
    use mtk_resource::DirectoryPack;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!(
        "mtk_atlas_err_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let atlas_path = dir.join(ATLAS);
    fs::create_dir_all(atlas_path.parent().unwrap()).unwrap();
    fs::write(&atlas_path, br#"{"sources":[]}"#).unwrap();
    fs::set_permissions(&atlas_path, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read(&atlas_path).is_ok() {
        let _ = fs::remove_dir_all(&dir);
        mtk_testkit::skip("file permissions are not enforced for this user (root?)");
        return;
    }

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(DirectoryPack::new("locked", &dir)));
    let result = stack.load_atlas_category(&AtlasCategory::Blocks);
    let _ = fs::remove_dir_all(&dir);

    assert!(
        result.is_err(),
        "an unreadable atlas definition must be reported instead of falling back to the builtin"
    );
}

/// The per-category post-processing (end portal injection) still runs on a merged definition.
#[test]
fn category_post_processing_still_applies() {
    let mut pack = MemoryPack::new("vanilla_like");
    pack.insert(
        ATLAS,
        br#"{"sources":[{"type":"minecraft:directory","prefix":"block/","source":"block"}]}"#
            .to_vec(),
    );
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));

    let def = stack
        .load_atlas_category(&AtlasCategory::Blocks)
        .expect("definition");
    let end_portal = ResourceLocation::vanilla("entity/end_portal/end_portal");
    assert!(
        def.sources.iter().any(|source| matches!(
            source,
            mtk_resource::AtlasSource::Single { resource, .. } if resource == &end_portal
        )),
        "the blocks category still injects the end portal sprite"
    );
}
