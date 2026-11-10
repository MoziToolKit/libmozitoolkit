use crate::error::TextureError;
use image::{ImageBuffer, Rgba};
use std::io::Cursor;

/// High-performance 2D RGBA pixel buffer for atlas generation and blitting.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA 4 bytes per pixel, row-major
}

impl RgbaBuffer {
    /// Create a new blank buffer initialized to transparent black (0, 0, 0, 0).
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width * height * 4) as usize;
        Self {
            width,
            height,
            pixels: vec![0; size],
        }
    }

    /// Create a new buffer filled with a solid RGBA color.
    pub fn solid(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let mut buf = Self::new(width, height);
        for chunk in buf.pixels.as_chunks_mut::<4>().0 {
            *chunk = [r, g, b, a];
        }
        buf
    }

    /// Create from decoded ImageBuffer<Rgba<u8>, Vec<u8>>.
    pub fn from_image_buffer(img: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Self {
        let (width, height) = img.dimensions();
        Self {
            width,
            height,
            pixels: img.into_raw(),
        }
    }

    /// Decode raw PNG bytes into an RgbaBuffer.
    pub fn from_png_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?;
        let rgba = img.to_rgba8();
        Ok(Self::from_image_buffer(rgba))
    }

    /// Encode buffer into PNG byte vector without cloning pixel buffer.
    pub fn to_png_bytes(&self) -> Result<Vec<u8>, TextureError> {
        let mut cursor = Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new(&mut cursor);
        image::ImageEncoder::write_image(
            encoder,
            &self.pixels,
            self.width,
            self.height,
            image::ExtendedColorType::Rgba8,
        )?;
        Ok(cursor.into_inner())
    }

    #[inline]
    pub fn pixel_index(&self, x: u32, y: u32) -> usize {
        ((y * self.width + x) * 4) as usize
    }

    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let idx = self.pixel_index(x, y);
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = self.pixel_index(x, y);
        self.pixels[idx] = color[0];
        self.pixels[idx + 1] = color[1];
        self.pixels[idx + 2] = color[2];
        self.pixels[idx + 3] = color[3];
    }

    /// Copy a rectangular region from `src` directly onto `self` at destination (dst_x, dst_y).
    pub fn blit(
        &mut self,
        src: &RgbaBuffer,
        src_x: u32,
        src_y: u32,
        dst_x: u32,
        dst_y: u32,
        width: u32,
        height: u32,
    ) {
        let copy_w = width
            .min(src.width.saturating_sub(src_x))
            .min(self.width.saturating_sub(dst_x));
        let copy_h = height
            .min(src.height.saturating_sub(src_y))
            .min(self.height.saturating_sub(dst_y));

        if copy_w == 0 || copy_h == 0 {
            return;
        }

        for row in 0..copy_h {
            let src_start = src.pixel_index(src_x, src_y + row);
            let src_end = src_start + (copy_w * 4) as usize;
            let dst_start = self.pixel_index(dst_x, dst_y + row);
            let dst_end = dst_start + (copy_w * 4) as usize;

            self.pixels[dst_start..dst_end].copy_from_slice(&src.pixels[src_start..src_end]);
        }
    }

    /// Extract a sub-rectangle as a new RgbaBuffer.
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        let mut sub = Self::new(width, height);
        sub.blit(self, x, y, 0, 0, width, height);
        sub
    }

    /// Tile this buffer vertically until it reaches `target_height`.
    /// Used to align 1-frame or fewer-frame PBR companion textures (normal/specular)
    /// with multi-frame animated albedo textures.
    pub fn tile_vertical(&self, target_height: u32) -> Self {
        if self.height == 0 || self.width == 0 || self.height == target_height {
            return self.clone();
        }

        let mut tiled = Self::new(self.width, target_height);
        let mut y = 0u32;
        while y < target_height {
            let chunk_h = (target_height - y).min(self.height);
            tiled.blit(self, 0, 0, 0, y, self.width, chunk_h);
            y += self.height;
        }
        tiled
    }

    /// Resize buffer to exact (new_width, new_height) using nearest neighbor interpolation.
    /// Preserves crisp pixel boundaries and avoids PBR channel interpolation artifacts.
    pub fn resize_nearest(&self, new_width: u32, new_height: u32) -> Self {
        if self.width == new_width && self.height == new_height {
            return self.clone();
        }
        if new_width == 0 || new_height == 0 || self.width == 0 || self.height == 0 {
            return Self::new(new_width, new_height);
        }

        let mut output = Self::new(new_width, new_height);
        for dy in 0..new_height {
            let sy = (dy as u64 * self.height as u64 / new_height as u64) as u32;
            for dx in 0..new_width {
                let sx = (dx as u64 * self.width as u64 / new_width as u64) as u32;
                let color = self.get_pixel(sx, sy);
                output.set_pixel(dx, dy, color);
            }
        }
        output
    }

    /// Align a PBR companion buffer (`_n` or `_s`) with its corresponding Albedo texture metrics.
    ///
    /// - If companion has matching total dimensions, it is returned as-is.
    /// - If companion is a single-frame texture (or has fewer frames than albedo):
    ///   1. Its single frame is resized to match `(albedo_fw, albedo_fh)`.
    ///   2. If albedo has multiple frames (`albedo_fc > 1`), it is vertically tiled to match `albedo_fh * albedo_fc`.
    /// - If companion is already a multi-frame strip matching `albedo_fc`, it is resized to `(albedo_fw, albedo_fh * albedo_fc)`.
    pub fn align_companion_to_albedo(
        &self,
        albedo_fw: u32,
        albedo_fh: u32,
        albedo_fc: u32,
    ) -> Self {
        let target_total_height = albedo_fh.saturating_mul(albedo_fc.max(1));
        if self.width == albedo_fw && self.height == target_total_height {
            return self.clone();
        }

        if self.width == 0 || self.height == 0 || albedo_fw == 0 || albedo_fh == 0 {
            return self.clone();
        }

        let is_multi_frame_matching =
            albedo_fc > 1 && self.height > self.width && (self.height / self.width) == albedo_fc;

        if is_multi_frame_matching {
            self.resize_nearest(albedo_fw, target_total_height)
        } else {
            let single_frame = if self.height > self.width && albedo_fc > 1 {
                self.crop(0, 0, self.width, self.width)
            } else {
                self.clone()
            };

            let resized_single = single_frame.resize_nearest(albedo_fw, albedo_fh);
            if albedo_fc > 1 {
                resized_single.tile_vertical(target_total_height)
            } else {
                resized_single
            }
        }
    }

    /// In-place multiplies RGB channels by a tint color `[R, G, B]` (0..255).
    ///
    /// Preserves Alpha channel. Uses fast integer arithmetic with rounding.
    pub fn tint(&mut self, tint_rgb: [u8; 3]) {
        if tint_rgb == [255, 255, 255] {
            return;
        }
        let tr = tint_rgb[0] as u32;
        let tg = tint_rgb[1] as u32;
        let tb = tint_rgb[2] as u32;

        for chunk in self.pixels.as_chunks_mut::<4>().0 {
            chunk[0] = ((chunk[0] as u32 * tr + 127) / 255) as u8;
            chunk[1] = ((chunk[1] as u32 * tg + 127) / 255) as u8;
            chunk[2] = ((chunk[2] as u32 * tb + 127) / 255) as u8;
        }
    }

    /// Returns a copy of the buffer with RGB channels multiplied by `tint_rgb`.
    pub fn to_tinted(&self, tint_rgb: [u8; 3]) -> Self {
        let mut cloned = self.clone();
        cloned.tint(tint_rgb);
        cloned
    }

    /// Standard Porter-Duff source-over alpha blend of `overlay` onto `self`.
    pub fn blend_over(&mut self, overlay: &RgbaBuffer) {
        self.blend_over_tinted(overlay, [255, 255, 255]);
    }

    /// Standard Porter-Duff source-over alpha blend of `overlay` onto `self`,
    /// modulating overlay RGB by `tint_rgb` prior to blending.
    pub fn blend_over_tinted(&mut self, overlay: &RgbaBuffer, tint_rgb: [u8; 3]) {
        let w = self.width.min(overlay.width);
        let h = self.height.min(overlay.height);
        let tr = tint_rgb[0] as u32;
        let tg = tint_rgb[1] as u32;
        let tb = tint_rgb[2] as u32;

        for y in 0..h {
            for x in 0..w {
                let src = overlay.get_pixel(x, y);
                let src_a = src[3];
                if src_a == 0 {
                    continue;
                }

                // Modulate overlay RGB by tint
                let tinted_r = ((src[0] as u32 * tr + 127) / 255) as u8;
                let tinted_g = ((src[1] as u32 * tg + 127) / 255) as u8;
                let tinted_b = ((src[2] as u32 * tb + 127) / 255) as u8;

                let dst = self.get_pixel(x, y);
                let dst_a = dst[3];

                if dst_a == 0 {
                    self.set_pixel(x, y, [tinted_r, tinted_g, tinted_b, src_a]);
                    continue;
                }

                if src_a == 255 && dst_a == 255 {
                    self.set_pixel(x, y, [tinted_r, tinted_g, tinted_b, 255]);
                    continue;
                }

                // Mathematically exact non-premultiplied Porter-Duff Over
                let a = src_a as f32 / 255.0;
                let inv_a = 1.0 - a;
                let dst_norm_a = dst_a as f32 / 255.0;
                let out_a_f = a + dst_norm_a * inv_a;
                if out_a_f <= 0.0 {
                    self.set_pixel(x, y, [0, 0, 0, 0]);
                    continue;
                }

                let r = ((tinted_r as f32 * a + dst[0] as f32 * dst_norm_a * inv_a) / out_a_f)
                    .round() as u8;
                let g = ((tinted_g as f32 * a + dst[1] as f32 * dst_norm_a * inv_a) / out_a_f)
                    .round() as u8;
                let b = ((tinted_b as f32 * a + dst[2] as f32 * dst_norm_a * inv_a) / out_a_f)
                    .round() as u8;
                let out_a = (out_a_f * 255.0).round().min(255.0) as u8;
                self.set_pixel(x, y, [r, g, b, out_a]);
            }
        }
    }

    /// In-place vertically flip (V-axis / Y-axis inversion) the pixel rows.
    pub fn flip_v(&mut self) {
        if self.height <= 1 || self.width == 0 {
            return;
        }
        let row_bytes = (self.width * 4) as usize;
        let half = (self.height / 2) as usize;
        let h = self.height as usize;
        for y in 0..half {
            let top_idx = y * row_bytes;
            let bot_idx = (h - 1 - y) * row_bytes;
            let (first, second) = self.pixels.split_at_mut(bot_idx);
            first[top_idx..top_idx + row_bytes].swap_with_slice(&mut second[..row_bytes]);
        }
    }

    /// Converts the RGBA8 buffer to a normalized [0.0, 1.0] float32 buffer (`width * height * 4`).
    /// If `flip_v` is true, rows are streamed bottom-to-top (Blender native Image.pixels coordinate system).
    pub fn to_f32_buffer(&self, flip_v: bool) -> Vec<f32> {
        let total_floats = (self.width * self.height * 4) as usize;
        let mut f32_buf = Vec::with_capacity(total_floats);
        let row_bytes = (self.width * 4) as usize;
        let h = self.height as usize;
        let scale = 1.0f32 / 255.0f32;

        if flip_v {
            for y in (0..h).rev() {
                let row_start = y * row_bytes;
                let row_slice = &self.pixels[row_start..row_start + row_bytes];
                for &b in row_slice {
                    f32_buf.push(b as f32 * scale);
                }
            }
        } else {
            for &b in &self.pixels {
                f32_buf.push(b as f32 * scale);
            }
        }
        f32_buf
    }

    /// Returns `true` if all pixels in this buffer have alpha == 255 (completely opaque).
    #[inline]
    pub fn is_opaque(&self) -> bool {
        if self.pixels.is_empty() {
            return true;
        }
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|chunk| chunk[3] == 255)
    }

    /// Returns `true` if any pixel in this buffer has alpha < 255 (contains cutout or translucent pixels).
    #[inline]
    pub fn has_transparency(&self) -> bool {
        !self.is_opaque()
    }

    /// Calculates the average RGB solid color across the entire image buffer,
    /// weighted by pixel alpha to avoid darkening transparent borders/cutouts.
    /// Returns a standard uppercase hex string, e.g. `"#80A755"`, without an alpha channel.
    /// If the image contains zero non-transparent pixels, fallback to `"#FFFFFF"`.
    pub fn average_rgb_hex(&self) -> String {
        let (r, g, b) = self.average_rgb();
        format!("#{:02X}{:02X}{:02X}", r, g, b)
    }

    /// Calculates the average RGB solid color components [0..255] weighted by alpha.
    pub fn average_rgb(&self) -> (u8, u8, u8) {
        if self.pixels.is_empty() {
            return (255, 255, 255);
        }

        let mut sum_r = 0.0f64;
        let mut sum_g = 0.0f64;
        let mut sum_b = 0.0f64;
        let mut total_weight = 0.0f64;

        for chunk in self.pixels.as_chunks::<4>().0 {
            let r = chunk[0] as f64;
            let g = chunk[1] as f64;
            let b = chunk[2] as f64;
            let a = chunk[3] as f64 / 255.0;

            if a > 0.0 {
                sum_r += r * a;
                sum_g += g * a;
                sum_b += b * a;
                total_weight += a;
            }
        }

        if total_weight > 0.0 {
            let avg_r = (sum_r / total_weight).round().clamp(0.0, 255.0) as u8;
            let avg_g = (sum_g / total_weight).round().clamp(0.0, 255.0) as u8;
            let avg_b = (sum_b / total_weight).round().clamp(0.0, 255.0) as u8;
            (avg_r, avg_g, avg_b)
        } else {
            (255, 255, 255)
        }
    }
}

#[cfg(test)]
#[path = "buffer_tests.rs"]
mod tests;
