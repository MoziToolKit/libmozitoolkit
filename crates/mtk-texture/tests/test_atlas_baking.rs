use mtk_resource::{AnimationMetadata, ResourceLocation};
use mtk_texture::{
    bake_paletted_permutation, AtlasBuilder, AtlasBuilderConfig, DecodedSprite, RgbaBuffer,
};

#[test]
fn test_paletted_permutation_baking() {
    // 1x2 palette key: Red (255, 0, 0) and Green (0, 255, 0)
    let mut key_pal = RgbaBuffer::new(2, 1);
    key_pal.set_pixel(0, 0, [255, 0, 0, 255]);
    key_pal.set_pixel(1, 0, [0, 255, 0, 255]);

    // 1x2 target permutation: Blue (0, 0, 255) and Yellow (255, 255, 0)
    let mut perm_pal = RgbaBuffer::new(2, 1);
    perm_pal.set_pixel(0, 0, [0, 0, 255, 255]);
    perm_pal.set_pixel(1, 0, [255, 255, 0, 255]);

    // Base 2x2 image with Red, Green, and White pixels
    let mut base = RgbaBuffer::new(2, 2);
    base.set_pixel(0, 0, [255, 0, 0, 255]); // Will become Blue
    base.set_pixel(1, 0, [0, 255, 0, 255]); // Will become Yellow
    base.set_pixel(0, 1, [255, 255, 255, 255]); // Unchanged White
    base.set_pixel(1, 1, [0, 0, 0, 0]); // Transparent

    let baked = bake_paletted_permutation(&base, &key_pal, &perm_pal).unwrap();
    assert_eq!(baked.get_pixel(0, 0), [0, 0, 255, 255]);
    assert_eq!(baked.get_pixel(1, 0), [255, 255, 0, 255]);
    assert_eq!(baked.get_pixel(0, 1), [255, 255, 255, 255]);
    assert_eq!(baked.get_pixel(1, 1), [0, 0, 0, 0]);
}

#[test]
fn test_atlas_builder_with_pbr_and_animation() {
    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: 1024,
        max_height: 1024,
        mip_level: 0,
        padding: 1,
    });

    let stone_id = ResourceLocation::parse("minecraft:block/stone").unwrap();
    let diamond_id = ResourceLocation::parse("minecraft:block/diamond_ore").unwrap();

    let stone_albedo = RgbaBuffer::solid(16, 16, 120, 120, 120, 255);
    let diamond_albedo = RgbaBuffer::solid(16, 32, 0, 200, 255, 255); // 2 frames of 16x16
    let diamond_normal = RgbaBuffer::solid(16, 16, 128, 128, 255, 255); // 1 frame static normal
    let diamond_spec = RgbaBuffer::solid(16, 16, 255, 255, 255, 255); // 1 frame static specular

    let sprites = vec![
        DecodedSprite {
            sprite_id: stone_id.clone(),
            albedo: stone_albedo,
            normal: None,
            specular: None,
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 16,
            frame_height: 16,
            frame_count: 1,
            metadata: None,
        },
        DecodedSprite {
            sprite_id: diamond_id.clone(),
            albedo: diamond_albedo,
            normal: Some(diamond_normal),
            specular: Some(diamond_spec),
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 16,
            frame_height: 16,
            frame_count: 2,
            metadata: Some(AnimationMetadata {
                frametime: 2,
                interpolate: true,
                ..Default::default()
            }),
        },
    ];

    let baked = builder.build_from_sprites(sprites).unwrap();
    // Dual chunks: 1 static chunk + 1 animated chunk
    assert_eq!(baked.chunks.len(), 2);

    let static_chunk = &baked.chunks[0];
    assert!(!static_chunk.is_animated);
    assert_eq!(static_chunk.file_stem(), "blocks_chunk_001");

    let anim_chunk = &baked.chunks[1];
    assert!(anim_chunk.is_animated);
    assert_eq!(anim_chunk.file_stem(), "blocks_anim_chunk_001");
    assert!(anim_chunk.normal.is_some());
    assert!(anim_chunk.specular.is_some());

    // Check address map
    let stone_loc = baked.address_map.lookup_static(&stone_id).unwrap();
    assert!(!stone_loc.is_animated);
    assert_eq!(stone_loc.chunk_id, 0);
    assert!(!stone_loc.has_normal);
    assert!(!stone_loc.has_specular);
    assert!(!stone_loc.has_overlay);
    assert_eq!(stone_loc.frame_size, [16, 16]);
    assert_eq!(stone_loc.average_color.as_deref(), Some("#787878"));

    // Static Frame 0 lookup for diamond_ore (chunk 0)
    let diamond_static = baked.address_map.lookup_static(&diamond_id).unwrap();
    assert!(!diamond_static.is_animated);
    assert_eq!(diamond_static.chunk_id, 0);
    assert_eq!(diamond_static.average_color.as_deref(), Some("#00C8FF"));

    // Dedicated animated strip lookup for diamond_ore (chunk 1)
    let diamond_anim = baked.address_map.lookup_animated(&diamond_id).unwrap();
    assert!(diamond_anim.is_animated);
    assert_eq!(diamond_anim.chunk_id, 1);
    assert!(diamond_anim.has_normal);
    assert!(diamond_anim.has_specular);
    assert!(!diamond_anim.has_overlay);
    assert_eq!(diamond_anim.frame_count, 2);
    assert!(diamond_anim.animation.is_some());
    assert_eq!(diamond_anim.frame_size, [16, 16]);
    assert!(diamond_anim.frame_uv_step[1] > 0.0);
    assert_eq!(diamond_anim.average_color.as_deref(), Some("#00C8FF"));
}

#[test]
fn test_pbr_auto_tiling_for_animated_sprite() {
    let builder = AtlasBuilder::new(AtlasBuilderConfig::default());
    let water_id = ResourceLocation::parse("minecraft:block/water_still").unwrap();

    // 4 frames of 16x16 = 16x64 albedo
    let mut water_albedo = RgbaBuffer::new(16, 64);
    for frame in 0..4 {
        for y in 0..16 {
            for x in 0..16 {
                water_albedo.set_pixel(x, frame * 16 + y, [0, 100, 200 + frame as u8 * 10, 255]);
            }
        }
    }

    // 1 frame 16x16 static normal
    let water_normal = RgbaBuffer::solid(16, 16, 128, 128, 255, 255);

    let sprites = vec![DecodedSprite {
        sprite_id: water_id.clone(),
        albedo: water_albedo,
        normal: Some(water_normal),
        specular: None,
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: 16,
        frame_height: 16,
        frame_count: 4,
        metadata: Some(AnimationMetadata {
            frametime: 1,
            ..Default::default()
        }),
    }];

    let baked = builder.build_from_sprites(sprites).unwrap();
    // 2 chunks: 1 static chunk (Frame 0) + 1 animated chunk (4-frame strip)
    assert_eq!(baked.chunks.len(), 2);
    let anim_chunk = &baked.chunks[1];
    assert!(anim_chunk.is_animated);
    assert_eq!(anim_chunk.file_stem(), "blocks_anim_chunk_001");

    let norm_buf = anim_chunk.normal.as_ref().unwrap();
    let loc = baked.address_map.lookup_animated(&water_id).unwrap();
    // Normal buffer should have 4 tiled frames at the sprite's strip location
    for frame in 0..4 {
        let sample_y = loc.pixel_rect[1] + frame * 16 + 8;
        let px = norm_buf.get_pixel(loc.pixel_rect[0] + 8, sample_y);
        assert_eq!(
            px,
            [128, 128, 255, 255],
            "Frame {} normal pixel mismatch",
            frame
        );
    }
}

#[test]
fn test_companion_dimension_scaling_and_alignment() {
    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: 1024,
        max_height: 1024,
        mip_level: 0,
        padding: 0,
    });

    // Simulate lava_flow: 32x32 single-frame, 2 frames = 32x64 albedo
    // But companion specular is 16x16 solid emission!
    let lava_id = ResourceLocation::parse("minecraft:block/lava_flow").unwrap();
    let lava_albedo = RgbaBuffer::solid(32, 64, 255, 100, 0, 255);
    let lava_spec_16 = RgbaBuffer::solid(16, 16, 10, 20, 250, 255); // Blue channel = 250 (Emission)

    // DecodedSprite with companion auto-aligned
    let aligned_spec = lava_spec_16.align_companion_to_albedo(32, 32, 2);
    assert_eq!(aligned_spec.width, 32);
    assert_eq!(aligned_spec.height, 64);

    let sprites = vec![DecodedSprite {
        sprite_id: lava_id.clone(),
        albedo: lava_albedo,
        normal: None,
        specular: Some(aligned_spec),
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: 32,
        frame_height: 32,
        frame_count: 2,
        metadata: Some(AnimationMetadata {
            frametime: 1,
            ..Default::default()
        }),
    }];

    let baked = builder.build_from_sprites(sprites).unwrap();
    assert_eq!(baked.chunks.len(), 2);

    // 1. Static chunk (Frame 0: 32x32)
    let static_chunk = &baked.chunks[0];
    let static_spec = static_chunk.specular.as_ref().unwrap();
    let static_loc = baked.address_map.lookup_static(&lava_id).unwrap();

    // Verify all four quadrants of the 32x32 static sprite have full emission (250)
    for sample_x in [static_loc.pixel_rect[0] + 4, static_loc.pixel_rect[0] + 28] {
        for sample_y in [static_loc.pixel_rect[1] + 4, static_loc.pixel_rect[1] + 28] {
            let px = static_spec.get_pixel(sample_x, sample_y);
            assert_eq!(
                px,
                [10, 20, 250, 255],
                "Static frame 0 pixel at ({}, {}) was not fully covered",
                sample_x,
                sample_y
            );
        }
    }

    // 2. Animated chunk (Frame 0 and Frame 1: 32x64)
    let anim_chunk = &baked.chunks[1];
    let anim_spec = anim_chunk.specular.as_ref().unwrap();
    let anim_loc = baked.address_map.lookup_animated(&lava_id).unwrap();
    for frame in 0..2 {
        let sample_y = anim_loc.pixel_rect[1] + frame * 32 + 16;
        let px = anim_spec.get_pixel(anim_loc.pixel_rect[0] + 16, sample_y);
        assert_eq!(
            px,
            [10, 20, 250, 255],
            "Animated frame {} specular emission mismatch",
            frame
        );
    }
}

#[test]
fn test_atlas_builder_with_overlay() {
    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: 1024,
        max_height: 1024,
        mip_level: 0,
        padding: 0,
    });

    let grass_side_id = ResourceLocation::parse("minecraft:block/grass_block_side").unwrap();
    let stone_id = ResourceLocation::parse("minecraft:block/stone").unwrap();

    // grass_block_side has albedo and overlay
    let grass_albedo = RgbaBuffer::solid(16, 16, 134, 96, 67, 255);
    let mut grass_overlay = RgbaBuffer::new(16, 16);
    // top 4 pixels are green overlay, rest transparent
    for y in 0..4 {
        for x in 0..16 {
            grass_overlay.set_pixel(x, y, [100, 200, 50, 255]);
        }
    }

    let stone_albedo = RgbaBuffer::solid(16, 16, 120, 120, 120, 255);

    let sprites = vec![
        DecodedSprite {
            sprite_id: grass_side_id.clone(),
            albedo: grass_albedo,
            normal: None,
            specular: None,
            overlay: Some(grass_overlay),
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 16,
            frame_height: 16,
            frame_count: 1,
            metadata: None,
        },
        DecodedSprite {
            sprite_id: stone_id.clone(),
            albedo: stone_albedo,
            normal: None,
            specular: None,
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 16,
            frame_height: 16,
            frame_count: 1,
            metadata: None,
        },
    ];

    let baked = builder.build_from_sprites(sprites).unwrap();
    assert_eq!(baked.chunks.len(), 1);

    let chunk = &baked.chunks[0];
    assert!(chunk.overlay.is_some());
    assert!(baked.address_map.chunks[0].has_overlay);

    let grass_loc = baked.address_map.lookup_static(&grass_side_id).unwrap();
    assert!(grass_loc.has_overlay);

    // Verify overlay alias in address_map
    let overlay_id = grass_side_id.with_suffix("_overlay");
    let overlay_loc = baked
        .address_map
        .lookup_static(&overlay_id)
        .expect("Overlay alias must exist in address_map");
    assert_eq!(overlay_loc.pixel_rect, grass_loc.pixel_rect);
    assert_eq!(overlay_loc.uv_bounds, grass_loc.uv_bounds);
    assert!(overlay_loc.has_overlay);

    let stone_loc = baked.address_map.lookup_static(&stone_id).unwrap();
    assert!(!stone_loc.has_overlay);

    let overlay_buf = chunk.overlay.as_ref().unwrap();
    // Check grass overlay pixel at (x=8, y=2)
    let px = overlay_buf.get_pixel(grass_loc.pixel_rect[0] + 8, grass_loc.pixel_rect[1] + 2);
    assert_eq!(px, [100, 200, 50, 255]);

    // Check stone region in overlay buffer is transparent [0, 0, 0, 0]
    let stone_px = overlay_buf.get_pixel(stone_loc.pixel_rect[0] + 8, stone_loc.pixel_rect[1] + 8);
    assert_eq!(stone_px, [0, 0, 0, 0]);
}

#[test]
fn test_independent_atlases_not_hijacked_by_entities() {
    use mtk_resource::AtlasCategory;

    let banner_base_id = ResourceLocation::parse("minecraft:entity/banner/banner_base").unwrap();
    let pot_base_id =
        ResourceLocation::parse("minecraft:entity/decorated_pot/decorated_pot_base").unwrap();
    let zombie_id = ResourceLocation::parse("minecraft:entity/zombie/zombie").unwrap();

    let sprites_banner = vec![DecodedSprite {
        sprite_id: banner_base_id.clone(),
        albedo: RgbaBuffer::solid(64, 64, 255, 0, 0, 255),
        normal: None,
        specular: None,
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: 64,
        frame_height: 64,
        frame_count: 1,
        metadata: None,
    }];
    let sprites_pot = vec![DecodedSprite {
        sprite_id: pot_base_id.clone(),
        albedo: RgbaBuffer::solid(32, 32, 200, 100, 50, 255),
        normal: None,
        specular: None,
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: 32,
        frame_height: 32,
        frame_count: 1,
        metadata: None,
    }];

    let builder = AtlasBuilder::new(AtlasBuilderConfig::default());
    let baked_banner = builder
        .build_from_sprites_with_category(sprites_banner, "banner_patterns")
        .unwrap();
    let baked_pot = builder
        .build_from_sprites_with_category(sprites_pot, "decorated_pot")
        .unwrap();

    let mut combined_map = baked_banner.address_map;
    combined_map.merge(baked_pot.address_map);

    let mut entities_sprites = vec![
        DecodedSprite {
            sprite_id: banner_base_id.clone(),
            albedo: RgbaBuffer::solid(64, 64, 255, 0, 0, 255),
            normal: None,
            specular: None,
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 64,
            frame_height: 64,
            frame_count: 1,
            metadata: None,
        },
        DecodedSprite {
            sprite_id: pot_base_id.clone(),
            albedo: RgbaBuffer::solid(32, 32, 200, 100, 50, 255),
            normal: None,
            specular: None,
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 32,
            frame_height: 32,
            frame_count: 1,
            metadata: None,
        },
        DecodedSprite {
            sprite_id: zombie_id.clone(),
            albedo: RgbaBuffer::solid(64, 64, 0, 255, 0, 255),
            normal: None,
            specular: None,
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width: 64,
            frame_height: 64,
            frame_count: 1,
            metadata: None,
        },
    ];

    // Filter out already allocated sprites and independent categories
    entities_sprites.retain(|sp| !combined_map.sprites.contains_key(&sp.sprite_id));
    entities_sprites.retain(|sp| {
        let cat = AtlasCategory::classify_texture_path(&sp.sprite_id.path);
        cat == AtlasCategory::Entities || cat == AtlasCategory::Misc
    });

    let baked_entities = builder
        .build_from_sprites_with_category(entities_sprites, "entities")
        .unwrap();
    combined_map.merge(baked_entities.address_map);

    // Assertions: banner and pot MUST maintain their independent atlas categories
    let banner_loc = combined_map.lookup_static(&banner_base_id).unwrap();
    assert_eq!(banner_loc.category, "banner_patterns");

    let pot_loc = combined_map.lookup_static(&pot_base_id).unwrap();
    assert_eq!(pot_loc.category, "decorated_pot");

    let zombie_loc = combined_map.lookup_static(&zombie_id).unwrap();
    assert_eq!(zombie_loc.category, "entities");
}

#[test]
fn test_emissive_companion_animation_synthesis() {
    let bush_id = ResourceLocation::new("minecraft", "block/firefly_bush");
    // Base bush: 16x16 solid green (0, 180, 0, 255)
    let bush_base = RgbaBuffer::solid(16, 16, 0, 180, 0, 255);
    let bush_bytes = bush_base.to_png_bytes().unwrap();

    // Emissive companion: 16x32 (2 frames of 16x16)
    // Frame 0: transparent except (2, 2) has full-bright yellow (255, 255, 0, 255)
    // Frame 1: transparent except (8, 8) has full-bright yellow (255, 255, 0, 255)
    let mut emissive = RgbaBuffer::new(16, 32);
    emissive.set_pixel(2, 2, [255, 255, 0, 255]);
    emissive.set_pixel(8, 8 + 16, [255, 255, 0, 255]);
    let emissive_bytes = emissive.to_png_bytes().unwrap();

    let discovered = mtk_resource::DiscoveredSprite {
        sprite_id: bush_id.clone(),
        texture_location: bush_id.clone(),
        raw_albedo: Some(bush_bytes),
        raw_normal: None,
        raw_specular: None,
        raw_overlay: None,
        raw_emissive: Some(emissive_bytes),
        metadata: None,
        emissive_metadata: Some(AnimationMetadata {
            frametime: 3,
            interpolate: false,
            ..Default::default()
        }),
    };

    let decoded = DecodedSprite::from_discovered(discovered).unwrap();
    assert_eq!(decoded.frame_width, 16);
    assert_eq!(decoded.frame_height, 16);
    assert_eq!(decoded.frame_count, 2);
    assert!(decoded.metadata.is_some());
    assert_eq!(decoded.albedo.width, 16);
    assert_eq!(decoded.albedo.height, 32);

    // Frame 0 checks
    assert_eq!(decoded.albedo.get_pixel(0, 0), [0, 180, 0, 255]);
    assert_eq!(decoded.albedo.get_pixel(2, 2), [255, 255, 0, 255]); // Firefly pixel blended
    assert_eq!(decoded.albedo.get_pixel(8, 8), [0, 180, 0, 255]);

    // Frame 1 checks
    assert_eq!(decoded.albedo.get_pixel(0, 16), [0, 180, 0, 255]);
    assert_eq!(decoded.albedo.get_pixel(2, 2 + 16), [0, 180, 0, 255]);
    assert_eq!(decoded.albedo.get_pixel(8, 8 + 16), [255, 255, 0, 255]); // Firefly pixel blended
}
