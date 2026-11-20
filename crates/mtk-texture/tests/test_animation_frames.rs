//! `.png.mcmeta` can list its frames explicitly (`frames: [2, 0, 1]`), including repeats.
//!
//! The atlas bakes every animation as a vertical strip and the host plays it top to bottom, so the
//! strip must already be in *playback* order. Parsing `frames` and then ignoring it plays the
//! animation in file order, which is wrong for any pack that reorders or repeats frames.

use mtk_resource::{AnimationFrame, AnimationMetadata, ResourceLocation};
use mtk_texture::{
    AtlasBuilder, AtlasBuilderConfig, AtlasSpriteLocation, DecodedSprite, RgbaBuffer,
};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// Three 16x16 frames stacked vertically: row order 0=red, 1=green, 2=blue.
fn animated_sprite(id: &str, frames: Option<Vec<AnimationFrame>>) -> DecodedSprite {
    let mut albedo = RgbaBuffer::new(16, 48);
    for (index, color) in [RED, GREEN, BLUE].into_iter().enumerate() {
        for y in 0..16 {
            for x in 0..16 {
                albedo.set_pixel(x, index as u32 * 16 + y, color);
            }
        }
    }
    DecodedSprite {
        sprite_id: ResourceLocation::parse(id).expect("id"),
        albedo,
        normal: None,
        specular: None,
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: 16,
        frame_height: 16,
        frame_count: 3,
        metadata: Some(AnimationMetadata {
            frametime: 2,
            interpolate: false,
            width: None,
            height: None,
            frames,
        }),
    }
}

fn bake(sprite: DecodedSprite) -> (Vec<[u8; 4]>, RgbaBuffer) {
    let baked = AtlasBuilder::new(AtlasBuilderConfig::production(512, 512, 0))
        .build_from_sprites_with_category(vec![sprite], "blocks")
        .expect("bake");

    let loc: AtlasSpriteLocation = baked
        .address_map
        .lookup_animated(&ResourceLocation::parse("minecraft:block/frame_order").unwrap())
        .expect("animated sprite")
        .clone();
    let chunk = baked
        .chunks
        .iter()
        .find(|chunk| chunk.chunk_id == loc.chunk_id)
        .expect("chunk");

    // Sample the middle row of each frame in the baked strip.
    let y = loc.strip_pixel_rect[1];
    let (w, frame_h) = (
        loc.strip_pixel_rect[2],
        loc.strip_pixel_rect[3] / loc.frame_count,
    );
    let frames = (0..loc.frame_count)
        .map(|frame| {
            let py = y + frame * frame_h + frame_h / 2;
            let px = chunk.albedo.get_pixel(loc.strip_pixel_rect[0] + w / 2, py);
            [px[0], px[1], px[2], px[3]]
        })
        .collect();
    (frames, chunk.albedo.clone())
}

#[test]
fn explicit_frame_order_is_baked_into_the_strip() {
    let (frames, _) = bake(animated_sprite(
        "minecraft:block/frame_order",
        Some(vec![
            AnimationFrame::Index(2),
            AnimationFrame::Index(0),
            AnimationFrame::Index(1),
        ]),
    ));
    assert_eq!(
        frames,
        vec![BLUE, RED, GREEN],
        "the strip must follow `frames`, not file order"
    );
}

#[test]
fn repeated_frames_are_repeated_in_the_strip() {
    // Four entries over a three-frame source: the strip (and the frame count) must grow.
    let baked = AtlasBuilder::new(AtlasBuilderConfig::production(512, 512, 0))
        .build_from_sprites_with_category(
            vec![animated_sprite(
                "minecraft:block/frame_order",
                Some(vec![
                    AnimationFrame::Index(0),
                    AnimationFrame::Index(2),
                    AnimationFrame::Index(0),
                    AnimationFrame::Index(1),
                ]),
            )],
            "blocks",
        )
        .expect("bake");

    let loc = baked
        .address_map
        .lookup_animated(&ResourceLocation::parse("minecraft:block/frame_order").unwrap())
        .expect("animated sprite");
    let chunk = baked
        .chunks
        .iter()
        .find(|chunk| chunk.chunk_id == loc.chunk_id)
        .expect("chunk");
    assert_eq!(
        loc.frame_count, 4,
        "the played animation is longer than the image"
    );
    assert_eq!(
        loc.strip_pixel_rect[3],
        4 * 16,
        "strip must hold every entry"
    );

    let sampled: Vec<[u8; 4]> = (0..4)
        .map(|frame| {
            let py = loc.strip_pixel_rect[1] + frame * 16 + 8;
            let px = chunk.albedo.get_pixel(loc.strip_pixel_rect[0] + 8, py);
            [px[0], px[1], px[2], px[3]]
        })
        .collect();
    assert_eq!(sampled, vec![RED, BLUE, RED, GREEN]);
}

#[test]
fn missing_frame_order_keeps_file_order() {
    let (frames, _) = bake(animated_sprite("minecraft:block/frame_order", None));
    assert_eq!(frames, vec![RED, GREEN, BLUE]);
}

/// The static sheet (used when animation is off) must show the *first played* frame.
#[test]
fn static_slot_uses_the_first_played_frame() {
    let baked = AtlasBuilder::new(AtlasBuilderConfig::production(512, 512, 0))
        .build_from_sprites_with_category(
            vec![animated_sprite(
                "minecraft:block/frame_order",
                Some(vec![AnimationFrame::Index(1), AnimationFrame::Index(2)]),
            )],
            "blocks",
        )
        .expect("bake");

    let id = ResourceLocation::parse("minecraft:block/frame_order").unwrap();
    let loc = baked.address_map.lookup_static(&id).expect("static slot");
    let chunk = baked
        .chunks
        .iter()
        .find(|chunk| chunk.chunk_id == loc.chunk_id)
        .expect("chunk");
    let [x, y, w, h] = loc.pixel_rect;
    let centre = chunk.albedo.get_pixel(x + w / 2, y + h / 2);
    assert_eq!(
        [centre[0], centre[1], centre[2], centre[3]],
        GREEN,
        "frame 0 of the static sheet must be the first frame of the animation"
    );
}

/// After the bake the strip *is* the playback order, so the exported metadata must not ask the
/// host to reorder again.
#[test]
fn baked_metadata_no_longer_lists_frames() {
    let baked = AtlasBuilder::new(AtlasBuilderConfig::production(512, 512, 0))
        .build_from_sprites_with_category(
            vec![animated_sprite(
                "minecraft:block/frame_order",
                Some(vec![AnimationFrame::Index(2), AnimationFrame::Index(0)]),
            )],
            "blocks",
        )
        .expect("bake");
    let loc = baked
        .address_map
        .lookup_animated(&ResourceLocation::parse("minecraft:block/frame_order").unwrap())
        .expect("animated sprite");
    let animation = loc.animation.as_ref().expect("animation metadata");
    assert_eq!(loc.frame_count, 2);
    assert!(
        animation.frames.is_none(),
        "the strip is already reordered; leaving `frames` would make a host apply it twice"
    );
    assert_eq!(animation.frametime, 2, "timing must survive the reorder");
}
