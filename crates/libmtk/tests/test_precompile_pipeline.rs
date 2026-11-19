use libmtk::{
    precompile_all_assets, AssetCacheReader, PrecompileConfig, ASSET_CACHE_FORMAT_VERSION,
};
use mtk_resource::{MemoryPack, ResourcePackStack};
use std::fs;
use std::path::Path;

#[test]
fn test_precompile_all_assets_end_to_end() {
    let mut stack = ResourcePackStack::new();
    let mut pack = MemoryPack::new("TestVanilla");

    // 1. Add sample texture (16x16 red PNG)
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]));
    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    pack.insert("assets/minecraft/textures/block/stone.png", png_bytes);

    // 2. Add sample block model JSON
    let model_json = r##"{
        "textures": { "all": "minecraft:block/stone" },
        "elements": [
            {
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "up":    { "texture": "#all", "cullface": "up" },
                    "down":  { "texture": "#all", "cullface": "down" },
                    "north": { "texture": "#all", "cullface": "north" },
                    "south": { "texture": "#all", "cullface": "south" },
                    "west":  { "texture": "#all", "cullface": "west" },
                    "east":  { "texture": "#all", "cullface": "east" }
                }
            }
        ]
    }"##;
    pack.insert(
        "assets/minecraft/models/block/stone.json",
        model_json.as_bytes().to_vec(),
    );

    // 3. Add sample blockstate JSON
    let blockstate_json = r##"{
        "variants": {
            "": { "model": "minecraft:block/stone" }
        }
    }"##;
    pack.insert(
        "assets/minecraft/blockstates/stone.json",
        blockstate_json.as_bytes().to_vec(),
    );

    // 4. Add sample item texture and model
    let item_img = image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 255, 255]));
    let mut item_png = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut item_png);
    item_img
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    pack.insert("assets/minecraft/textures/item/diamond_sword.png", item_png);

    let item_json = r##"{
        "parent": "minecraft:item/generated",
        "textures": { "layer0": "minecraft:item/diamond_sword" }
    }"##;
    pack.insert(
        "assets/minecraft/models/item/diamond_sword.json",
        item_json.as_bytes().to_vec(),
    );

    stack.push_pack(Box::new(pack));

    let temp_cache_dir =
        std::env::temp_dir().join(format!("mtk_test_cache_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_cache_dir);

    let config = PrecompileConfig {
        max_atlas_width: 1024,
        max_atlas_height: 1024,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: None,
        resolver_overrides: None,
    };

    let result = precompile_all_assets(&stack, &temp_cache_dir, &config).unwrap();
    assert!(result.success);
    assert_eq!(result.pack_count, 1);
    assert!(result.atlas_chunks >= 1);
    assert!(result.baked_models >= 1);

    // Verify .mtkcache file existence
    let package_path = Path::new(&result.package_path);
    assert!(package_path.exists());
    assert_eq!(package_path.extension().unwrap(), "mtkcache");

    // Verify AssetCacheReader header check & full load
    assert!(AssetCacheReader::is_valid_cache_file(
        package_path,
        &stack.compute_stack_fingerprint()
    ));

    let reader = AssetCacheReader::open(package_path).unwrap();
    let manifest = reader.manifest().unwrap();
    assert_eq!(manifest.format_version, ASSET_CACHE_FORMAT_VERSION);
    assert_eq!(manifest.pack_count, 1);
    assert_eq!(manifest.fingerprint, stack.compute_stack_fingerprint());

    let models = reader.load_models().unwrap();
    assert!(!models.is_empty());
    assert!(models.get_item("minecraft:diamond_sword").is_some());
    assert!(models.get_item("diamond_sword").is_some());
    assert!(models.get_item("item/diamond_sword").is_some());

    let atlas_map = reader.load_atlas_mapping().unwrap();
    assert!(!atlas_map.chunks.is_empty());

    // Verify extracting atlas textures on-demand
    let extract_dir = temp_cache_dir.join("extracted");
    let extracted = reader.extract_all_atlas_textures(&extract_dir).unwrap();
    assert!(!extracted.is_empty());

    // Clean up
    let _ = fs::remove_dir_all(&temp_cache_dir);
}

/// A manifest from an older asset-cache format must be rejected even when the fingerprint matches,
/// otherwise a cache written before a baking change is served as if it were current.
#[test]
fn cache_manifest_validation_checks_the_format_version() {
    let fp = "mtk_fp_0123456789abcdef";
    let current = libmtk::CacheManifest {
        format_version: ASSET_CACHE_FORMAT_VERSION,
        fingerprint: fp.to_string(),
        pack_count: 1,
        atlas_chunks: 1,
        baked_models: 1,
        created_at_epoch_secs: 0,
    };
    assert!(current.is_valid_for(fp));

    let stale = libmtk::CacheManifest {
        format_version: ASSET_CACHE_FORMAT_VERSION - 1,
        ..current.clone()
    };
    assert!(!stale.is_valid_for(fp));
}

#[test]
fn fingerprint_truncation_never_collides() {
    let shared_prefix = "mtk_fp_0123456789abcdef";
    let a = format!("{shared_prefix}_alpha");
    let b = format!("{shared_prefix}_beta");
    assert_ne!(
        libmtk::fingerprint_str_to_bytes16(&a),
        libmtk::fingerprint_str_to_bytes16(&b),
        "16-byte truncation makes two distinct fingerprints share one cache key"
    );
}

/// The resolver validates caches from the 64-byte header alone. A package written by an older
/// asset-cache build can carry the same header fingerprint, so the manifest format version has to
/// be consulted too; otherwise the stale cache is accepted and the host renders old geometry.
#[test]
fn cache_header_validation_rejects_a_stale_manifest_format_version() {
    use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, PackageProfile};

    let fingerprint = "mtk_fp_0011223344556677";
    let fp_bytes = libmtk::fingerprint_str_to_bytes16(fingerprint);

    let dir = std::env::temp_dir().join(format!("mtk_cache_fmt_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let write_cache = |name: &str, format_version: u32| -> std::path::PathBuf {
        let path = dir.join(name);
        let mut writer = MtkPackageFileWriter::create(&path, PackageProfile::AssetCache, fp_bytes)
            .expect("create package");
        writer
            .add_json_chunk(
                *b"META",
                "manifest",
                &libmtk::CacheManifest {
                    format_version,
                    fingerprint: fingerprint.to_string(),
                    pack_count: 1,
                    atlas_chunks: 1,
                    baked_models: 1,
                    created_at_epoch_secs: 0,
                },
                &ChunkWriteOptions::zstd_fast(),
            )
            .expect("write manifest");
        writer.finish().expect("finish package");
        path
    };

    let current = write_cache("current.mtkcache", ASSET_CACHE_FORMAT_VERSION);
    let stale = write_cache("stale.mtkcache", ASSET_CACHE_FORMAT_VERSION - 1);

    assert!(
        AssetCacheReader::is_valid_cache_file(&current, fingerprint),
        "a cache in the current format must be accepted"
    );
    assert!(
        !AssetCacheReader::is_valid_cache_file(&stale, fingerprint),
        "a cache from an older asset-cache format must be rejected, not silently served"
    );

    let _ = fs::remove_dir_all(&dir);
}
