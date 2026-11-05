//! High-throughput UV alpha sampling and transparent face analysis.

use glam::Vec2;
use mtk_core::get_uv_center;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Sampling strategy for evaluating face transparency against a texture buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleMode {
    /// Sample alpha at the UV geometric center of the face.
    Center,
    /// Sample alpha at all loop corners and the center; all must be transparent (<= threshold).
    AllCorners,
    /// Compute the arithmetic average alpha of all loop corners and the center.
    Average,
}

impl SampleMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "ALL_CORNERS" | "ALLCORNERS" | "CORNERS" => Self::AllCorners,
            "AVERAGE" | "AVG" => Self::Average,
            _ => Self::Center,
        }
    }
}

/// Convert normalized UV coordinate to integer pixel coordinate (x, y) with wrap-around.
#[inline]
pub fn uv_to_pixel_coord(u: f32, v: f32, width: u32, height: u32, invert_y: bool) -> (u32, u32) {
    let w = width as f32;
    let h = height as f32;

    let px = (u * w).floor() as i64;
    let x = px.rem_euclid(width as i64) as u32;

    let target_v = if invert_y { 1.0 - v } else { v };
    let py = (target_v * h).floor() as i64;
    let y = py.rem_euclid(height as i64) as u32;

    (x, y)
}

/// Sample alpha [0.0, 1.0] from an RGBA u8 buffer at normalized (u, v).
#[inline]
pub fn sample_alpha_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    u: f32,
    v: f32,
    invert_y: bool,
) -> f32 {
    if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
        return 1.0;
    }
    let (x, y) = uv_to_pixel_coord(u, v, width, height, invert_y);
    let idx = ((y * width + x) * 4 + 3) as usize;
    if idx < pixels.len() {
        pixels[idx] as f32 / 255.0
    } else {
        1.0
    }
}

/// Sample alpha [0.0, 1.0] from an RGBA f32 buffer at normalized (u, v).
#[inline]
pub fn sample_alpha_f32(
    width: u32,
    height: u32,
    pixels: &[f32],
    u: f32,
    v: f32,
    invert_y: bool,
) -> f32 {
    if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
        return 1.0;
    }
    let (x, y) = uv_to_pixel_coord(u, v, width, height, invert_y);
    let idx = ((y * width + x) * 4 + 3) as usize;
    if idx < pixels.len() {
        pixels[idx]
    } else {
        1.0
    }
}

/// Check if a single face's UVs map to transparent pixels in an RGBA u8 buffer.
pub fn is_face_transparent_u8(
    face_uvs: &[Vec2],
    width: u32,
    height: u32,
    pixels: &[u8],
    mode: SampleMode,
    threshold: f32,
    invert_y: bool,
) -> bool {
    if face_uvs.is_empty() {
        return false;
    }

    let center = get_uv_center(face_uvs);

    match mode {
        SampleMode::Center => {
            let alpha = sample_alpha_u8(width, height, pixels, center.x, center.y, invert_y);
            alpha <= threshold
        }
        SampleMode::AllCorners => {
            let center_alpha = sample_alpha_u8(width, height, pixels, center.x, center.y, invert_y);
            if center_alpha > threshold {
                return false;
            }
            face_uvs
                .iter()
                .all(|uv| sample_alpha_u8(width, height, pixels, uv.x, uv.y, invert_y) <= threshold)
        }
        SampleMode::Average => {
            let mut sum_alpha =
                sample_alpha_u8(width, height, pixels, center.x, center.y, invert_y);
            for uv in face_uvs {
                sum_alpha += sample_alpha_u8(width, height, pixels, uv.x, uv.y, invert_y);
            }
            let avg = sum_alpha / ((face_uvs.len() + 1) as f32);
            avg <= threshold
        }
    }
}

/// Check if a single face's UVs map to transparent pixels in an RGBA f32 buffer.
pub fn is_face_transparent_f32(
    face_uvs: &[Vec2],
    width: u32,
    height: u32,
    pixels: &[f32],
    mode: SampleMode,
    threshold: f32,
    invert_y: bool,
) -> bool {
    if face_uvs.is_empty() {
        return false;
    }

    let center = get_uv_center(face_uvs);

    match mode {
        SampleMode::Center => {
            let alpha = sample_alpha_f32(width, height, pixels, center.x, center.y, invert_y);
            alpha <= threshold
        }
        SampleMode::AllCorners => {
            let center_alpha =
                sample_alpha_f32(width, height, pixels, center.x, center.y, invert_y);
            if center_alpha > threshold {
                return false;
            }
            face_uvs.iter().all(|uv| {
                sample_alpha_f32(width, height, pixels, uv.x, uv.y, invert_y) <= threshold
            })
        }
        SampleMode::Average => {
            let mut sum_alpha =
                sample_alpha_f32(width, height, pixels, center.x, center.y, invert_y);
            for uv in face_uvs {
                sum_alpha += sample_alpha_f32(width, height, pixels, uv.x, uv.y, invert_y);
            }
            let avg = sum_alpha / ((face_uvs.len() + 1) as f32);
            avg <= threshold
        }
    }
}

/// Batch analyze transparent faces against an RGBA u8 buffer.
pub fn batch_analyze_transparent_faces_u8(
    faces_uvs: &[Vec<Vec2>],
    width: u32,
    height: u32,
    pixels: &[u8],
    mode: SampleMode,
    threshold: f32,
    invert_y: bool,
) -> Vec<bool> {
    #[cfg(feature = "parallel")]
    {
        if faces_uvs.len() > 256 {
            return faces_uvs
                .par_iter()
                .map(|uvs| {
                    is_face_transparent_u8(uvs, width, height, pixels, mode, threshold, invert_y)
                })
                .collect();
        }
    }

    faces_uvs
        .iter()
        .map(|uvs| is_face_transparent_u8(uvs, width, height, pixels, mode, threshold, invert_y))
        .collect()
}

/// Batch analyze transparent faces against an RGBA f32 buffer.
pub fn batch_analyze_transparent_faces_f32(
    faces_uvs: &[Vec<Vec2>],
    width: u32,
    height: u32,
    pixels: &[f32],
    mode: SampleMode,
    threshold: f32,
    invert_y: bool,
) -> Vec<bool> {
    #[cfg(feature = "parallel")]
    {
        if faces_uvs.len() > 256 {
            return faces_uvs
                .par_iter()
                .map(|uvs| {
                    is_face_transparent_f32(uvs, width, height, pixels, mode, threshold, invert_y)
                })
                .collect();
        }
    }

    faces_uvs
        .iter()
        .map(|uvs| is_face_transparent_f32(uvs, width, height, pixels, mode, threshold, invert_y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uv_to_pixel_coord() {
        assert_eq!(uv_to_pixel_coord(0.0, 0.0, 16, 16, false), (0, 0));
        assert_eq!(uv_to_pixel_coord(0.5, 0.5, 16, 16, false), (8, 8));
        assert_eq!(uv_to_pixel_coord(1.0, 1.0, 16, 16, false), (0, 0)); // wraps to 0
        assert_eq!(uv_to_pixel_coord(0.99, 0.99, 16, 16, false), (15, 15));

        // Test with invert_y (top-left origin)
        assert_eq!(uv_to_pixel_coord(0.0, 0.0, 16, 16, true), (0, 0)); // 1.0 - 0.0 = 1.0 -> wraps to 0
        assert_eq!(uv_to_pixel_coord(0.0, 0.99, 16, 16, true), (0, 0)); // 1.0 - 0.99 = 0.01 -> row 0
    }

    #[test]
    fn test_sample_modes_u8() {
        // Create a 2x2 image:
        // Top row: [Transparent (0,0), Opaque (1,0)]
        // Bottom row: [Opaque (0,1), Transparent (1,1)]
        let mut pixels = vec![0u8; 2 * 2 * 4];
        // (x=0, y=0) -> transparent (a=0)
        pixels[3] = 0;
        // (x=1, y=0) -> opaque (a=255)
        pixels[4 + 3] = 255;
        // (x=0, y=1) -> opaque (a=255)
        pixels[8 + 3] = 255;
        // (x=1, y=1) -> transparent (a=0)
        pixels[12 + 3] = 0;

        let face_quad_00 = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.4, 0.0),
            Vec2::new(0.4, 0.4),
            Vec2::new(0.0, 0.4),
        ];

        // Center is at (0.2, 0.2) -> pixel (0, 0) -> transparent!
        assert!(is_face_transparent_u8(
            &face_quad_00,
            2,
            2,
            &pixels,
            SampleMode::Center,
            0.01,
            false
        ));
        assert!(is_face_transparent_u8(
            &face_quad_00,
            2,
            2,
            &pixels,
            SampleMode::AllCorners,
            0.01,
            false
        ));
        assert!(is_face_transparent_u8(
            &face_quad_00,
            2,
            2,
            &pixels,
            SampleMode::Average,
            0.01,
            false
        ));

        // Quad sampling all 4 quadrants of the 2x2 image
        let sample_all_quad = vec![
            Vec2::new(0.25, 0.25), // pixel (0, 0): a=0
            Vec2::new(0.75, 0.25), // pixel (1, 0): a=255
            Vec2::new(0.75, 0.75), // pixel (1, 1): a=0
            Vec2::new(0.25, 0.75), // pixel (0, 1): a=255
        ];
        // Center is at (0.5, 0.5) -> pixel (1, 1) -> a=0 -> Center says transparent!
        assert!(is_face_transparent_u8(
            &sample_all_quad,
            2,
            2,
            &pixels,
            SampleMode::Center,
            0.01,
            false
        ));
        // But corners (1,0) and (0,1) have a=255 -> AllCorners says NOT transparent!
        assert!(!is_face_transparent_u8(
            &sample_all_quad,
            2,
            2,
            &pixels,
            SampleMode::AllCorners,
            0.01,
            false
        ));
        // Average is (0 + 255 + 0 + 255 + 0) / 5 = 102/255 > 0.01 -> NOT transparent!
        assert!(!is_face_transparent_u8(
            &sample_all_quad,
            2,
            2,
            &pixels,
            SampleMode::Average,
            0.01,
            false
        ));
    }
}
