use mtk_material::{
    clean_identifier, decode_grid_atlas_uv, remap_local_to_atlas,
    remap_mesh_multi_uvs_parallel_with_options, GridAtlasSpec, MaterialResolver,
};
use mtk_resource::ResourceLocation;
use mtk_texture::{AtlasAddressMap, AtlasSpriteLocation, SpriteKind};
use std::collections::HashMap;

#[test]
fn test_grid_atlas_spec_detection() {
    let spec = GridAtlasSpec {
        atlas_name_patterns: vec!["TerrainRGB".to_string(), "terrain".to_string()],
        atlas_suffix_patterns: vec!["-rgba".to_string(), "_rgb".to_string()],
        ..Default::default()
    };

    assert!(spec.matches_atlas_name("TerrainRGB.png"));
    assert!(spec.matches_atlas_name("terrain-rgba.001.png"));
    assert!(spec.matches_atlas_name("world_rgb.png"));
    assert!(!spec.matches_atlas_name("stone.png"));
}

#[test]
fn test_grid_atlas_uv_decode() {
    let mut swatch_map = HashMap::new();
    swatch_map.insert(
        0,
        vec!["grass_block_top".to_string(), "grass_top".to_string()],
    );

    let spec = GridAtlasSpec {
        swatch_size: 18.0,
        tile_size: 16.0,
        border: 1.0,
        image_width: 1024,
        image_height: 1024,
        atlas_name_patterns: vec!["terrain".to_string()],
        atlas_suffix_patterns: vec![],
        swatch_to_candidates: swatch_map,
    };

    // Swatch 0 is at col 0, row 0 on 1024x1024 image
    // Center of swatch 0: x in [1..17], y in [1..17] -> pixel (9, 9)
    let u = 9.0 / 1024.0;
    let v = 1.0 - (9.0 / 1024.0);
    let (cands, local_uv) = decode_grid_atlas_uv(u, v, &spec);

    assert!(cands.is_some());
    assert_eq!(cands.unwrap()[0], "grass_block_top");
    // Local UV should be around center (0.5, 0.5)
    assert!((local_uv[0] - 0.5).abs() < 1e-2);
    assert!((local_uv[1] - 0.5).abs() < 1e-2);
}

#[test]
fn test_clean_identifier() {
    assert_eq!(clean_identifier("block/stone.png"), "stone");
    assert_eq!(clean_identifier("minecraft:stone.001"), "stone");
    assert_eq!(
        clean_identifier("textures/block/grass_block_top"),
        "grass_block_top"
    );
}

#[test]
fn test_clean_identifier_strips_longest_biome_suffix() {
    // Short suffixes (`_forest`) must never win over longer ones (`_dark_forest`).
    assert_eq!(clean_identifier("stone_dark_forest"), "stone");
    assert_eq!(clean_identifier("stone-dark_forest"), "stone");
    assert_eq!(clean_identifier("stone_birch_forest"), "stone");
    assert_eq!(clean_identifier("stone_cherry_grove"), "stone");
    assert_eq!(clean_identifier("stone_forest"), "stone");
    assert_eq!(clean_identifier("stone_y"), "stone");
    // Slash-qualified names keep their namespace, only the suffix is stripped.
    assert_eq!(
        clean_identifier("mod:block/stone_dark_forest"),
        "mod:block/stone"
    );
}

#[test]
fn test_resolver_and_atlas_projection() {
    let mut address_map = AtlasAddressMap::new();
    let stone_loc = ResourceLocation::new("minecraft", "block/stone");

    address_map.sprites.insert(
        stone_loc.clone(),
        AtlasSpriteLocation {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            sprite_kind: SpriteKind::StaticAtlas,
            texture_id: 42,
            uv_bounds: [0.0, 0.0, 0.25, 0.25],
            frame_0_uv_bounds: [0.0, 0.0, 0.25, 0.25],
            local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            frame_uv_step: [0.0, 0.0],
            pixel_rect: [0, 0, 16, 16],
            strip_pixel_rect: [0, 0, 16, 16],
            frame_size: [16, 16],
            frame_count: 1,
            animation: None,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            average_color: None,
        },
    );

    let mut custom_aliases = HashMap::new();
    custom_aliases.insert(
        "custom_rock_mat".to_string(),
        vec!["block/stone".to_string()],
    );

    let resolved =
        MaterialResolver::resolve("custom_rock_mat", Some(&custom_aliases), &address_map);
    assert!(resolved.is_some());
    let (res_id, sprite) = resolved.unwrap();
    assert_eq!(res_id.path, "block/stone");
    assert_eq!(sprite.chunk_id, 0);

    // Project local [0.5, 0.5] to atlas
    let projected = remap_local_to_atlas(0.5, 0.5, sprite);
    assert_eq!(projected, [0.125, 0.125]);
}

#[test]
fn test_parallel_batch_mesh_remap() {
    let mut address_map = AtlasAddressMap::new();
    let dirt_loc = ResourceLocation::new("minecraft", "block/dirt");

    address_map.sprites.insert(
        dirt_loc,
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            is_animated: false,
            sprite_kind: SpriteKind::StaticAtlas,
            texture_id: 10,
            uv_bounds: [0.5, 0.5, 1.0, 1.0],
            frame_0_uv_bounds: [0.5, 0.5, 1.0, 1.0],
            local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            frame_uv_step: [0.0, 0.0],
            pixel_rect: [32, 32, 16, 16],
            strip_pixel_rect: [32, 32, 16, 16],
            frame_size: [16, 16],
            frame_count: 1,
            animation: None,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            average_color: None,
        },
    );

    let mut custom_aliases = HashMap::new();
    custom_aliases.insert("muddy_ground".to_string(), vec!["block/dirt".to_string()]);

    // 2 quads (8 loops)
    let uvs = vec![
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];
    let face_materials = vec!["muddy_ground".to_string(), "dirt".to_string()];
    let face_loop_ranges = vec![(0, 4), (4, 4)];

    let multi_res = remap_mesh_multi_uvs_parallel_with_options(
        &uvs,
        &face_materials,
        &face_loop_ranges,
        &address_map,
        Some(&custom_aliases),
        None,
        false,
    );
    assert_eq!(multi_res.face_count, 2);
    assert_eq!(multi_res.local_uvs.len(), 8);
    assert_eq!(multi_res.atlas_uvs.len(), 8);
}

#[test]
fn test_fluid_material_resolver_prioritization() {
    let mut address_map = AtlasAddressMap::new();

    // Painting "water" that would otherwise collide if looked up naively
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "water"),
        AtlasSpriteLocation {
            chunk_id: 99,
            category: "paintings".to_string(),
            texture_id: 999,
            ..Default::default()
        },
    );

    // Block water textures
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_still"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 201,
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/water_flow"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 202,
            ..Default::default()
        },
    );

    // Block lava textures
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/lava_still"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 203,
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/lava_flow"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 204,
            ..Default::default()
        },
    );

    // 1. "water" resolves to "block/water_still" (chunk 1, id 201), NOT painting (chunk 99)
    let res = MaterialResolver::resolve("water", None, &address_map);
    assert!(res.is_some());
    let (loc, sp) = res.unwrap();
    assert_eq!(loc.path, "block/water_still");
    assert_eq!(sp.texture_id, 201);

    // 2. "flowing_water" resolves to "block/water_flow"
    let res = MaterialResolver::resolve("flowing_water", None, &address_map);
    assert!(res.is_some());
    let (loc, sp) = res.unwrap();
    assert_eq!(loc.path, "block/water_flow");
    assert_eq!(sp.texture_id, 202);

    // 3. "lava" resolves to "block/lava_still"
    let res = MaterialResolver::resolve("lava", None, &address_map);
    assert!(res.is_some());
    let (loc, sp) = res.unwrap();
    assert_eq!(loc.path, "block/lava_still");
    assert_eq!(sp.texture_id, 203);

    // 4. "flowing_lava" resolves to "block/lava_flow"
    let res = MaterialResolver::resolve("flowing_lava", None, &address_map);
    assert!(res.is_some());
    let (loc, sp) = res.unwrap();
    assert_eq!(loc.path, "block/lava_flow");
    assert_eq!(sp.texture_id, 204);
}

#[test]
fn test_special_block_material_aliases() {
    let mut address_map = AtlasAddressMap::new();

    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "entity/end_portal/end_portal"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 301,
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "entity/conduit/base"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 302,
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "entity/banner/banner_base"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 303,
            ..Default::default()
        },
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/oak_hanging_sign"),
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            texture_id: 304,
            ..Default::default()
        },
    );

    // 1. End portal / Gateway
    let res = MaterialResolver::resolve("end_portal", None, &address_map);
    assert!(res.is_some());
    assert_eq!(res.unwrap().0.path, "entity/end_portal/end_portal");

    let res = MaterialResolver::resolve("minecraft:block/end_portal", None, &address_map);
    assert!(res.is_some());
    assert_eq!(res.unwrap().0.path, "entity/end_portal/end_portal");

    let res = MaterialResolver::resolve("end_gateway", None, &address_map);
    assert!(res.is_some());
    assert_eq!(res.unwrap().0.path, "entity/end_portal/end_portal");

    // 2. Conduit
    let res = MaterialResolver::resolve("conduit", None, &address_map);
    assert!(res.is_some());
    assert_eq!(res.unwrap().0.path, "entity/conduit/base");

    // 3. Banner base
    let res = MaterialResolver::resolve("banner_base", None, &address_map);
    assert!(res.is_some());
    assert_eq!(res.unwrap().0.path, "entity/banner/banner_base");

    // 4. Hanging sign legacy path
    let res = MaterialResolver::resolve("minecraft:entity/signs/hanging/oak", None, &address_map);
    assert!(res.is_some());
    assert_eq!(res.unwrap().0.path, "block/oak_hanging_sign");
}

fn animated_water_atlas() -> AtlasAddressMap {
    let mut address_map = AtlasAddressMap::new();
    let key = ResourceLocation::new("minecraft", "block/water_still");

    address_map.chunks.push(mtk_texture::AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 1024,
        height: 1024,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });
    address_map.chunks.push(mtk_texture::AtlasChunkMeta {
        chunk_id: 1,
        category: "blocks".to_string(),
        is_animated: true,
        category_chunk_index: 1,
        width: 1024,
        height: 1024,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    address_map.sprites.insert(
        key.clone(),
        AtlasSpriteLocation {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            sprite_kind: SpriteKind::StaticAtlas,
            texture_id: 10,
            uv_bounds: [0.0, 0.0, 0.25, 0.25],
            frame_0_uv_bounds: [0.0, 0.0, 0.25, 0.25],
            frame_size: [16, 16],
            frame_count: 1,
            ..Default::default()
        },
    );

    address_map.anim_sprites.insert(
        key,
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            is_animated: true,
            sprite_kind: SpriteKind::AnimatedAtlas,
            texture_id: 11,
            uv_bounds: [0.5, 0.75, 1.0, 1.0],
            frame_0_uv_bounds: [0.5, 0.75, 1.0, 1.0],
            frame_uv_step: [0.0, 0.25],
            frame_size: [16, 16],
            frame_count: 4,
            animation: Some(mtk_resource::AnimationMetadata {
                frametime: 2,
                interpolate: true,
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    address_map
}

#[test]
fn test_animated_routing_toggle_and_metrics() {
    use mtk_material::remap_mesh_multi_uvs_parallel_with_options;

    let atlas = animated_water_atlas();
    let face_materials = vec!["water_still".to_string()];
    let face_loop_ranges = vec![(0, 4)];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let animated = remap_mesh_multi_uvs_parallel_with_options(
        &uvs,
        &face_materials,
        &face_loop_ranges,
        &atlas,
        None,
        None,
        true,
    );
    assert_eq!(animated.face_chunk_ids, vec![1]);
    assert_eq!(animated.face_texture_ids, vec![11]);
    assert!(animated.face_is_animated[0]);
    assert_eq!(animated.face_anim_timing, vec![[4.0, 2.0, 1.0]]);
    assert_eq!(animated.face_anim_frame_size, vec![[16.0, 16.0, 0.0]]);
    assert!((animated.atlas_uvs[0][0] - 0.5).abs() < 1e-6);
    assert!((animated.atlas_uvs[0][1] - 0.75).abs() < 1e-6);

    let static_only = remap_mesh_multi_uvs_parallel_with_options(
        &uvs,
        &face_materials,
        &face_loop_ranges,
        &atlas,
        None,
        None,
        false,
    );
    assert_eq!(static_only.face_chunk_ids, vec![0]);
    assert_eq!(static_only.face_texture_ids, vec![10]);
    assert!(!static_only.face_is_animated[0]);
    assert_eq!(static_only.face_anim_timing, vec![[1.0, 1.0, 0.0]]);
    assert_eq!(static_only.atlas_uvs[0], [0.0, 0.0]);
}

#[test]
fn test_resolve_routes_and_reroute_animation() {
    use mtk_material::{reroute_mesh_animation, resolve_mesh_material_routes};

    let atlas = animated_water_atlas();
    let keys = vec!["minecraft:block/water_still".to_string()];

    let routes = resolve_mesh_material_routes(&keys, None, &atlas, true);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].chunk_id, 1);
    assert!(routes[0].is_animated);
    assert_eq!(routes[0].anim_timing, [4.0, 2.0, 1.0]);

    // Start from the static sprite's atlas UVs.
    let static_uvs = vec![[0.0, 0.0], [0.25, 0.0], [0.25, 0.25], [0.0, 0.25]];
    let ranges = vec![(0, 4)];
    let old_chunks = vec![0u16];

    let animated =
        reroute_mesh_animation(&static_uvs, &ranges, &keys, &old_chunks, &atlas, None, true);
    assert_eq!(animated.face_chunk_ids, vec![1]);
    assert!(animated.face_is_animated[0]);
    assert_eq!(animated.unmapped_faces, 0);
    assert_eq!(animated.uv_unmapped_faces, 0);
    assert!((animated.atlas_uvs[0][0] - 0.5).abs() < 1e-6);
    assert!((animated.atlas_uvs[0][1] - 0.75).abs() < 1e-6);

    // And back to the static baseline without any original local UVs.
    let back = reroute_mesh_animation(
        &animated.atlas_uvs,
        &ranges,
        &keys,
        &animated.face_chunk_ids,
        &atlas,
        None,
        false,
    );
    assert_eq!(back.face_chunk_ids, vec![0]);
    assert!(!back.face_is_animated[0]);
    for (actual, expected) in back.atlas_uvs.iter().zip(static_uvs.iter()) {
        assert!((actual[0] - expected[0]).abs() < 1e-6);
        assert!((actual[1] - expected[1]).abs() < 1e-6);
    }
}

#[test]
fn test_paletted_multi_uv_remap_and_source_key_palette() {
    use mtk_material::remap_mesh_multi_uvs_paletted_with_options;

    let atlas = animated_water_atlas();
    let palette = vec!["water_still".to_string()];
    let indices = vec![0u32, 0u32];
    let ranges = vec![(0, 4), (4, 4)];
    let uvs = vec![
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];

    let result = remap_mesh_multi_uvs_paletted_with_options(
        &uvs, &palette, &indices, &ranges, &atlas, None, None, None, None, true,
    );
    assert_eq!(result.mesh.face_count, 2);
    assert_eq!(result.mesh.face_chunk_ids, vec![1, 1]);
    assert!(result.mesh.face_is_animated.iter().all(|v| *v));
    assert_eq!(
        result.source_key_palette,
        vec!["minecraft:block/water_still".to_string()]
    );
    assert_eq!(result.face_source_key_indices, vec![0, 0]);
}

#[test]
fn test_paletted_source_key_prefers_base_over_overlay() {
    use mtk_material::remap_mesh_multi_uvs_paletted_with_options;

    let mut atlas = AtlasAddressMap::new();
    for key in [
        "minecraft:block/grass_block_side",
        "minecraft:block/grass_block_side_overlay",
    ] {
        atlas.sprites.insert(
            ResourceLocation::parse(key).unwrap(),
            AtlasSpriteLocation {
                chunk_id: 0,
                texture_id: 1147,
                frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                ..Default::default()
            },
        );
    }

    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let result = remap_mesh_multi_uvs_paletted_with_options(
        &uvs,
        &["minecraft:block/grass_block_side".to_string()],
        &[0u32],
        &[(0, 4)],
        &atlas,
        None,
        None,
        None,
        None,
        false,
    );
    assert_eq!(
        result.source_key_palette,
        vec!["minecraft:block/grass_block_side".to_string()]
    );
}
