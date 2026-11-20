//! Production atlas bakes must keep a sprite's edge texels away from its neighbours.
//!
//! Every sprite rect is sampled with bilinear filtering (and mipmaps above level 0), so a tap on
//! the UV boundary reaches outside the rect. Without padding that tap lands on the adjacent
//! sprite, which is why HD resource packs showed seams on block edges.

use mtk_resource::ResourceLocation;
use mtk_texture::{
    AtlasBuilder, AtlasBuilderConfig, AtlasSpriteLocation, BakedAtlas, DecodedSprite, RgbaBuffer,
};

fn solid(id: &str, w: u32, h: u32, rgba: [u8; 4]) -> DecodedSprite {
    DecodedSprite {
        sprite_id: ResourceLocation::parse(id).expect("id"),
        albedo: RgbaBuffer::solid(w, h, rgba[0], rgba[1], rgba[2], rgba[3]),
        normal: None,
        specular: None,
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: w,
        frame_height: h,
        frame_count: 1,
        metadata: None,
    }
}

fn bake(sprites: Vec<DecodedSprite>, config: AtlasBuilderConfig) -> BakedAtlas {
    AtlasBuilder::new(config)
        .build_from_sprites_with_category(sprites, "blocks")
        .expect("bake")
}

fn pixel(atlas: &BakedAtlas, chunk_id: u16, x: u32, y: u32) -> [u8; 4] {
    let chunk = atlas
        .chunks
        .iter()
        .find(|chunk| chunk.chunk_id == chunk_id)
        .expect("chunk");
    let px = chunk.albedo.get_pixel(x, y);
    [px[0], px[1], px[2], px[3]]
}

fn location(atlas: &BakedAtlas, id: &str) -> AtlasSpriteLocation {
    atlas
        .address_map
        .sprites
        .get(&ResourceLocation::parse(id).expect("id"))
        .expect("sprite")
        .clone()
}

/// `padding` must cover every mip level production can bake with, or the fix only works at mip 0.
#[test]
fn production_padding_covers_the_mip_level() {
    for mip in 0..=8 {
        let expected = AtlasBuilderConfig::padding_for_mip(mip);
        assert!(
            expected >= 1u32 << mip,
            "mip {mip}: padding {expected} cannot cover a {}-texel mip footprint",
            1u32 << mip
        );
        let config = AtlasBuilderConfig::production(1024, 1024, mip);
        assert_eq!(config.padding, expected);
        assert_eq!(config.mip_level, mip);
    }
    assert!(
        AtlasBuilderConfig::padding_for_mip(0) >= 2,
        "plain bilinear filtering already reads half a texel outside the rect"
    );
}

/// The texels directly around a sprite must repeat that sprite's own edges.
#[test]
fn neighbours_never_touch_a_sprite_edge() {
    let red = [255u8, 0, 0, 255];
    let blue = [0u8, 0, 255, 255];
    let atlas = bake(
        vec![
            solid("minecraft:block/pad_test_red", 16, 16, red),
            solid("minecraft:block/pad_test_blue", 16, 16, blue),
        ],
        AtlasBuilderConfig::production(512, 512, 0),
    );

    for (id, expected) in [
        ("minecraft:block/pad_test_red", red),
        ("minecraft:block/pad_test_blue", blue),
    ] {
        let loc = location(&atlas, id);
        let [x, y, w, h] = loc.pixel_rect;
        let padding = AtlasBuilderConfig::production(512, 512, 0).padding;
        assert!(padding > 0, "production must pad");

        // Ring of width `padding` around the sprite rect: all of it belongs to this sprite's slot.
        for py in y - padding..y + h + padding {
            for px in x - padding..x + w + padding {
                let inside = px >= x && px < x + w && py >= y && py < y + h;
                if inside {
                    continue;
                }
                assert_eq!(
                    pixel(&atlas, loc.chunk_id, px, py),
                    expected,
                    "{id}: texel at ({px}, {py}) is inside the padding ring but does not repeat \
                     the sprite edge; a bilinear tap at the UV boundary would blend it in"
                );
            }
        }
    }
}

/// Padding lives outside the addressable rect: UVs must still describe exactly the sprite.
#[test]
fn uv_bounds_exclude_the_padding() {
    let atlas = bake(
        vec![solid(
            "minecraft:block/pad_test_uv",
            16,
            16,
            [10, 20, 30, 255],
        )],
        AtlasBuilderConfig::production(512, 512, 0),
    );
    let loc = location(&atlas, "minecraft:block/pad_test_uv");
    let chunk = atlas
        .chunks
        .iter()
        .find(|chunk| chunk.chunk_id == loc.chunk_id)
        .expect("chunk");
    let [x, y, w, h] = loc.pixel_rect;
    let [u_min, v_min, u_max, v_max] = loc.uv_bounds;

    assert!((u_min - x as f32 / chunk.width as f32).abs() < 1e-6);
    assert!((u_max - (x + w) as f32 / chunk.width as f32).abs() < 1e-6);
    assert!((v_max - 1.0 + y as f32 / chunk.height as f32).abs() < 1e-6);
    assert!((v_min - 1.0 + (y + h) as f32 / chunk.height as f32).abs() < 1e-6);
    assert_eq!([w, h], [16, 16], "padding must not widen the sprite rect");
}
