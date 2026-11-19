//! The asset source has to behave identically for a cache and a scene package.
//!
//! Both containers are the same on disk, so a host that opens "a package" must get the same
//! payloads — models, atlas map, textures, colormaps — regardless of which profile wrote it.

use std::fs;

use libmtk::{
    export_scene_package, precompile_all_assets, AssetBundle, PrecompileConfig, SceneExportRequest,
};
use mtk_resource::ResourcePackStack;
use mtk_texture::RgbaBuffer;
use mtk_voxel::SparseVoxelPayload;

fn png16(rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

fn test_stack() -> ResourcePackStack {
    let mut pack = mtk_resource::MemoryPack::new("BundleTest");
    pack.insert(
        "assets/minecraft/textures/block/stone.png",
        png16([128, 128, 128]),
    );
    pack.insert(
        "assets/minecraft/models/block/stone.json",
        br##"{
            "textures": { "all": "minecraft:block/stone" },
            "elements": [{
                "from": [0, 0, 0], "to": [16, 16, 16],
                "faces": {
                    "up":    { "texture": "#all", "cullface": "up" },
                    "down":  { "texture": "#all", "cullface": "down" },
                    "north": { "texture": "#all", "cullface": "north" },
                    "south": { "texture": "#all", "cullface": "south" },
                    "west":  { "texture": "#all", "cullface": "west" },
                    "east":  { "texture": "#all", "cullface": "east" }
                }
            }]
        }"##
        .to_vec(),
    );
    pack.insert(
        "assets/minecraft/blockstates/stone.json",
        br##"{ "variants": { "": { "model": "minecraft:block/stone" } } }"##.to_vec(),
    );
    pack.insert(
        "assets/minecraft/textures/block/dirt.png",
        png16([132, 96, 62]),
    );
    pack.insert(
        "assets/minecraft/models/block/dirt.json",
        br##"{ "parent": "minecraft:block/stone", "textures": { "all": "minecraft:block/dirt" } }"##
            .to_vec(),
    );
    pack.insert(
        "assets/minecraft/blockstates/dirt.json",
        br##"{ "variants": { "": { "model": "minecraft:block/dirt" } } }"##.to_vec(),
    );

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));
    stack
}

#[test]
fn cache_and_scene_packages_expose_the_same_assets() {
    let dir = std::env::temp_dir().join(format!("mtk_bundle_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");

    let stack = test_stack();

    // 1. Asset cache written by the prebake pipeline.
    let cache_dir = dir.join("cache");
    fs::create_dir_all(&cache_dir).expect("cache dir");
    let config = PrecompileConfig {
        max_atlas_width: 256,
        max_atlas_height: 256,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: Some(1),
        resolver_overrides: None,
    };
    let precompiled = precompile_all_assets(&stack, &cache_dir, &config).expect("precompile");
    let cache = AssetBundle::open(&precompiled.package_path).expect("open cache");
    assert_eq!(
        cache.manifest().format_version,
        libmtk::CACHE_FORMAT_VERSION
    );
    assert!(cache.models.is_some(), "cache must carry models");
    assert!(
        cache.atlas_mapping.is_some(),
        "cache must carry the atlas map"
    );
    assert!(!cache.texture_names.is_empty(), "cache must carry textures");

    // 2. Scene package written from the same assets.
    let mut point_cloud = mtk_voxel::VoxelPointCloud::new();
    point_cloud.push_with_entity(
        [0.5, 0.5, 0.5],
        [0, 0, 0],
        "minecraft:stone".to_string(),
        "minecraft:plains".to_string(),
        15,
        None,
    );
    point_cloud.bounds = Some([0, 0, 0, 1, 1, 1]);
    let cloud = SparseVoxelPayload::from_point_cloud(&point_cloud).expect("sparse payload");
    let cache_entry = AssetBundle::open(&precompiled.package_path).expect("reopen cache");
    let models = cache_entry.models.clone().expect("models");
    let atlas_map = cache_entry.atlas_mapping.clone().expect("atlas map");
    let mut textures: std::collections::HashMap<String, RgbaBuffer> =
        std::collections::HashMap::new();
    for chunk in atlas_map.chunks.iter() {
        let id = format!("atlas/textures/{}.png", chunk.file_stem());
        if let Ok(buffer) = cache_entry.texture_rgba(&id, false) {
            textures.insert(chunk.file_stem(), buffer);
        }
    }
    // `HashMap<String, RgbaBuffer>` implements `SceneAtlasSource`, so the cache's atlas textures
    // can drive the scene export directly.
    let mut atlas_source = textures;
    let scene_path = dir.join("scene.mtkscene");
    export_scene_package(
        &scene_path,
        SceneExportRequest {
            voxels: &cloud,
            source_models: &models,
            source_atlas_map: &atlas_map,
            atlas_source: &mut atlas_source,
            config: &libmtk::SceneExporterConfig::default(),
            dynamic_entity_atlases: None,
            dynamic_entity_slots: None,
            ctm_mode: libmtk::CtmMode::Disabled,
        },
    )
    .expect("export scene");

    let scene = AssetBundle::open(&scene_path).expect("open scene");
    assert_eq!(
        scene.manifest().format_version,
        libmtk::SCENE_PACKAGE_FORMAT_VERSION
    );
    assert!(
        scene.manifest().voxel_count > 0,
        "scene must describe its voxels"
    );
    assert!(scene.models.is_some(), "scene must carry models");
    assert!(
        scene.atlas_mapping.is_some(),
        "scene must carry the atlas map"
    );
    assert!(
        !scene.texture_names.is_empty(),
        "scene must carry its mini-atlas textures"
    );

    // 3. Same payloads, same shapes: this is what makes one host code path possible.
    let cache_models = cache.models.as_ref().expect("cache models");
    let scene_models = scene.models.as_ref().expect("scene models");
    let cache_keys: Vec<&String> = cache_models.keys().collect();
    assert!(
        !cache_keys.is_empty(),
        "cache must bake at least one model; keys: {cache_keys:?}"
    );
    // A scene package is tree-shaken to what the scene needs, so it keeps a subset of the cache's
    // model set rather than a copy of it.
    assert!(
        !scene_models.is_empty(),
        "scene must carry tree-shaken models"
    );
    for key in scene_models.keys() {
        assert!(
            cache_models.get(key).is_some(),
            "scene model '{key}' must come from the same database the cache holds"
        );
    }
    let cache_chunks = cache
        .atlas_mapping
        .as_ref()
        .expect("cache map")
        .chunks
        .len();
    assert_eq!(
        scene
            .atlas_mapping
            .as_ref()
            .expect("scene map")
            .chunks
            .len(),
        cache_chunks,
        "both packages must describe the same atlas chunk count"
    );

    let _ = fs::remove_dir_all(&dir);
}
