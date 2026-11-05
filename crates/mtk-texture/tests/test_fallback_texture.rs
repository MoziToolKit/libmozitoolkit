use mtk_resource::ResourceLocation;
use mtk_texture::{
    create_fallback_checkerboard_buffer, AtlasBuilderConfig, DecodedSprite, RgbaBuffer,
    FALLBACK_TEXTURE_KEY,
};

#[test]
fn test_fallback_checkerboard_buffer_pattern() {
    let buf = create_fallback_checkerboard_buffer();
    assert_eq!(buf.width, 16);
    assert_eq!(buf.height, 16);

    // Verify 4x4 checkerboard tiles
    // Magenta: [255, 0, 255, 255], Dark gray/black: [24, 24, 24, 255]
    let magenta = [255, 0, 255, 255];
    let dark = [24, 24, 24, 255];

    // (0, 0) tile is magenta: ((0/4) + (0/4)) % 2 == 0
    assert_eq!(buf.get_pixel(0, 0), magenta);
    assert_eq!(buf.get_pixel(3, 3), magenta);

    // (4, 0) tile is dark: ((4/4) + (0/4)) % 2 == 1
    assert_eq!(buf.get_pixel(4, 0), dark);
    assert_eq!(buf.get_pixel(7, 3), dark);

    // (0, 4) tile is dark: ((0/4) + (4/4)) % 2 == 1
    assert_eq!(buf.get_pixel(0, 4), dark);

    // (4, 4) tile is magenta: ((4/4) + (4/4)) % 2 == 0
    assert_eq!(buf.get_pixel(4, 4), magenta);
}

#[test]
fn test_fallback_sprite_pinned_at_chunk0_slot0() {
    // Construct some dummy sprites with larger dimensions (e.g. 32x32, 64x64)
    // to prove that fallback (16x16) is strictly pinned at (0, 0) of chunk 0
    // regardless of the Height-descending sorting algorithm.
    let mut sprites = Vec::new();

    // Sprite A: 64x64
    let big_buf = RgbaBuffer::solid(64, 64, 100, 100, 100, 255);
    sprites.push(DecodedSprite {
        sprite_id: ResourceLocation::new("minecraft", "block/stone"),
        albedo: big_buf,
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
    });

    // Sprite B: 32x32
    let med_buf = RgbaBuffer::solid(32, 32, 150, 150, 150, 255);
    sprites.push(DecodedSprite {
        sprite_id: ResourceLocation::new("minecraft", "block/dirt"),
        albedo: med_buf,
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
    });

    let config = AtlasBuilderConfig {
        max_width: 1024,
        max_height: 1024,
        mip_level: 0,
        padding: 0,
    };

    let builder = mtk_texture::AtlasBuilder::new(config);
    let baked = builder
        .build_from_sprites_with_category(sprites, "blocks")
        .expect("Static atlas build must succeed");

    assert!(!baked.chunks.is_empty(), "Must produce at least 1 chunk");
    let chunk0 = &baked.chunks[0];
    assert_eq!(chunk0.chunk_id, 0);

    let address_map = &baked.address_map;

    // 1. Verify fallback sprite location in address map
    let fallback_loc = address_map
        .lookup_str(FALLBACK_TEXTURE_KEY)
        .expect("FALLBACK_TEXTURE_KEY must exist in address map");

    assert_eq!(fallback_loc.chunk_id, 0);
    assert_eq!(fallback_loc.texture_id, 0);
    assert_eq!(fallback_loc.pixel_rect, [0, 0, 16, 16]);

    // In UV space with Y-flipped (v_max is top=1.0, v_min is bottom):
    assert_eq!(fallback_loc.uv_bounds[0], 0.0); // u_min = 0 / width = 0
    assert_eq!(fallback_loc.uv_bounds[3], 1.0); // v_max = 1.0 - 0/height = 1.0

    // 2. Verify aliases point to the exact same fallback slot
    let missingno_loc = address_map
        .lookup_str("missingno")
        .expect("missingno alias must resolve");
    assert_eq!(missingno_loc.pixel_rect, [0, 0, 16, 16]);

    let mc_missingno_loc = address_map
        .lookup_str("minecraft:missingno")
        .expect("minecraft:missingno alias must resolve");
    assert_eq!(mc_missingno_loc.pixel_rect, [0, 0, 16, 16]);

    // 3. Verify lookup_fallback() authoritative resolution
    let resolved_fb = address_map
        .lookup_fallback()
        .expect("lookup_fallback must return fallback slot");
    assert_eq!(resolved_fb.chunk_id, 0);
    assert_eq!(resolved_fb.texture_id, 0);
    assert_eq!(resolved_fb.pixel_rect, [0, 0, 16, 16]);

    // 4. Verify pixel content of chunk 0 at (0, 0) is the checkerboard pattern
    let magenta = [255, 0, 255, 255];
    let dark = [24, 24, 24, 255];
    assert_eq!(chunk0.albedo.get_pixel(0, 0), magenta);
    assert_eq!(chunk0.albedo.get_pixel(3, 3), magenta);
    assert_eq!(chunk0.albedo.get_pixel(4, 0), dark);
}
