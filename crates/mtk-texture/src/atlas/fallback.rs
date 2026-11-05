//! Canonical fallback (missing texture) checkerboard sprite and constants.

use crate::{DecodedSprite, RgbaBuffer};
use mtk_resource::ResourceLocation;

/// Canonical fallback texture identifier SSOT across libmozitoolkit and MoziToolKit.
pub const FALLBACK_TEXTURE_KEY: &str = "mozi:fallback";

/// Generates the canonical 16x16 Minecraft missing texture checkerboard:
/// 4x4 magenta (#FF00FF / [255, 0, 255, 255]) and dark gray/black (#181818 / [24, 24, 24, 255]) tiles.
pub fn create_fallback_checkerboard_buffer() -> RgbaBuffer {
    let mut buf = RgbaBuffer::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let is_magenta = ((x / 4) + (y / 4)) % 2 == 0;
            let color = if is_magenta {
                [255, 0, 255, 255]
            } else {
                [24, 24, 24, 255]
            };
            buf.set_pixel(x, y, color);
        }
    }
    buf
}

/// Creates the canonical `DecodedSprite` for `mozi:fallback`.
pub fn create_fallback_sprite() -> DecodedSprite {
    DecodedSprite {
        sprite_id: ResourceLocation::new("mozi", "fallback"),
        albedo: create_fallback_checkerboard_buffer(),
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
    }
}
