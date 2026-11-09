use super::*;

#[test]
fn test_rgba_buffer_flip_v() {
    let mut buf = RgbaBuffer::new(2, 2);
    // Row 0: Red, Green
    buf.set_pixel(0, 0, [255, 0, 0, 255]);
    buf.set_pixel(1, 0, [0, 255, 0, 255]);
    // Row 1: Blue, White
    buf.set_pixel(0, 1, [0, 0, 255, 255]);
    buf.set_pixel(1, 1, [255, 255, 255, 255]);

    buf.flip_v();

    // After flip, Row 0 should be old Row 1 (Blue, White)
    assert_eq!(buf.get_pixel(0, 0), [0, 0, 255, 255]);
    assert_eq!(buf.get_pixel(1, 0), [255, 255, 255, 255]);
    // Row 1 should be old Row 0 (Red, Green)
    assert_eq!(buf.get_pixel(0, 1), [255, 0, 0, 255]);
    assert_eq!(buf.get_pixel(1, 1), [0, 255, 0, 255]);
}

#[test]
fn test_rgba_buffer_to_f32_buffer() {
    let mut buf = RgbaBuffer::new(2, 2);
    // Row 0: Red, Green
    buf.set_pixel(0, 0, [255, 0, 0, 255]);
    buf.set_pixel(1, 0, [0, 255, 0, 255]);
    // Row 1: Blue, White
    buf.set_pixel(0, 1, [0, 0, 255, 255]);
    buf.set_pixel(1, 1, [255, 255, 255, 255]);

    let f32_flipped = buf.to_f32_buffer(true);
    assert_eq!(f32_flipped.len(), 16);
    // Row 0 (bottom row in Blender) should be Blue [0, 0, 1, 1]
    assert!((f32_flipped[0] - 0.0).abs() < 1e-4);
    assert!((f32_flipped[1] - 0.0).abs() < 1e-4);
    assert!((f32_flipped[2] - 1.0).abs() < 1e-4);
    assert!((f32_flipped[3] - 1.0).abs() < 1e-4);

    // Row 1 (top row in Blender) should be Red [1, 0, 0, 1]
    assert!((f32_flipped[8] - 1.0).abs() < 1e-4);
    assert!((f32_flipped[9] - 0.0).abs() < 1e-4);
    assert!((f32_flipped[10] - 0.0).abs() < 1e-4);
    assert!((f32_flipped[11] - 1.0).abs() < 1e-4);
}

#[test]
fn test_rgba_buffer_average_rgb_hex() {
    // 1. Pure solid red
    let red_buf = RgbaBuffer::solid(2, 2, 255, 0, 0, 255);
    assert_eq!(red_buf.average_rgb_hex(), "#FF0000");

    // 2. Half red, half transparent (transparent pixels [0,0,0,0] must NOT darken result)
    let mut cutout_buf = RgbaBuffer::new(2, 2);
    cutout_buf.set_pixel(0, 0, [0, 255, 0, 255]); // Green
    cutout_buf.set_pixel(1, 0, [0, 255, 0, 255]); // Green
    cutout_buf.set_pixel(0, 1, [0, 0, 0, 0]); // Transparent
    cutout_buf.set_pixel(1, 1, [0, 0, 0, 0]); // Transparent
    assert_eq!(cutout_buf.average_rgb_hex(), "#00FF00");

    // 3. Fully transparent buffer fallbacks to #FFFFFF
    let blank_buf = RgbaBuffer::new(2, 2);
    assert_eq!(blank_buf.average_rgb_hex(), "#FFFFFF");

    // 4. Mix 50% red + 50% blue
    let mut mixed_buf = RgbaBuffer::new(2, 1);
    mixed_buf.set_pixel(0, 0, [200, 0, 0, 255]);
    mixed_buf.set_pixel(1, 0, [0, 0, 100, 255]);
    assert_eq!(mixed_buf.average_rgb_hex(), "#640032");
}

#[test]
fn test_rgba_buffer_tint() {
    let mut buf = RgbaBuffer::new(1, 1);
    buf.set_pixel(0, 0, [200, 100, 50, 255]);

    // Tint with cyan [0, 255, 255]
    buf.tint([0, 255, 255]);
    assert_eq!(buf.get_pixel(0, 0), [0, 100, 50, 255]);

    // Tint with 50% gray [128, 128, 128]
    let mut gray_buf = RgbaBuffer::new(1, 1);
    gray_buf.set_pixel(0, 0, [255, 255, 255, 200]);
    gray_buf.tint([128, 128, 128]);
    assert_eq!(gray_buf.get_pixel(0, 0), [128, 128, 128, 200]); // Alpha preserved
}

#[test]
fn test_rgba_buffer_blend_over_tinted() {
    // Base: solid red [255, 0, 0, 255]
    let mut base = RgbaBuffer::solid(2, 2, 255, 0, 0, 255);

    // Overlay: white grayscale mask with 50% alpha [255, 255, 255, 128]
    let mut mask = RgbaBuffer::new(2, 2);
    mask.set_pixel(0, 0, [255, 255, 255, 255]); // Fully opaque
    mask.set_pixel(1, 0, [255, 255, 255, 128]); // 50% semi-transparent
    mask.set_pixel(0, 1, [0, 0, 0, 0]); // Fully transparent

    // Tint with blue [0, 0, 255]
    base.blend_over_tinted(&mask, [0, 0, 255]);

    // (0, 0) should be 100% blue [0, 0, 255, 255]
    assert_eq!(base.get_pixel(0, 0), [0, 0, 255, 255]);

    // (1, 0) should be 50% red + 50% blue -> approx [127, 0, 128, 255]
    let p_half = base.get_pixel(1, 0);
    assert!((p_half[0] as i32 - 127).abs() <= 2);
    assert_eq!(p_half[1], 0);
    assert!((p_half[2] as i32 - 128).abs() <= 2);
    assert_eq!(p_half[3], 255);

    // (0, 1) should remain pure red [255, 0, 0, 255]
    assert_eq!(base.get_pixel(0, 1), [255, 0, 0, 255]);
}

#[test]
fn test_rgba_buffer_opacity_check() {
    let opaque_buf = RgbaBuffer::solid(4, 4, 100, 150, 200, 255);
    assert!(opaque_buf.is_opaque());
    assert!(!opaque_buf.has_transparency());

    let mut cutout_buf = RgbaBuffer::solid(4, 4, 100, 150, 200, 255);
    cutout_buf.set_pixel(2, 2, [0, 0, 0, 0]);
    assert!(!cutout_buf.is_opaque());
    assert!(cutout_buf.has_transparency());

    let mut translucent_buf = RgbaBuffer::solid(4, 4, 100, 150, 200, 255);
    translucent_buf.set_pixel(1, 1, [100, 150, 200, 254]);
    assert!(!translucent_buf.is_opaque());
    assert!(translucent_buf.has_transparency());
}
