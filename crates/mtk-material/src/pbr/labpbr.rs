//! # LabPBR 1.3 Decoder
//!
//! Decodes raw `_n` / `_s` RGBA8 buffers into the canonical `MtkSurface` channels,
//! packed as `MtkPackedV1`:
//!
//! - `normal_gl`: RGB = OpenGL tangent normal, A = height
//! - `orm`:       R = AO, G = roughness, B = metallic, A = emission mask
//! - `aux`:       R = encoded specular IOR, G = porosity, B = SSS weight, A = transmission
//!
//! All arithmetic is `f32`; the packed output is 8-bit. The formulas are the
//! canonical contract documented in `pbr_channel_spec.md`.

use std::sync::LazyLock;

use mtk_texture::RgbaBuffer;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::error::MaterialError;
use crate::pbr::types::DecodedPbr;

/// Old decoder's default `_s.R` when a chunk has no specular companion.
/// `1.0 - 0.292893 = 0.707107` perceptual roughness.
const DEFAULT_SMOOTHNESS: f32 = 0.292_893;
const DEFAULT_IOR: f32 = 1.5;
const MAX_IOR: f32 = 8.0;

static IOR_LUT: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [0u8; 256];
    for (i, val) in table.iter_mut().enumerate() {
        *val = encode_ior(ior_from_specular_scalar(i as u8));
    }
    table
});

static EMISSION_LUT: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [0u8; 256];
    for (i, val) in table.iter_mut().enumerate() {
        *val = emission_byte_scalar(i as u8);
    }
    table
});

static POROSITY_LUT: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [0u8; 256];
    for (i, val) in table.iter_mut().enumerate() {
        *val = porosity_byte_scalar(i as u8);
    }
    table
});

static SSS_LUT: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [0u8; 256];
    for (i, val) in table.iter_mut().enumerate() {
        *val = sss_byte_scalar(i as u8);
    }
    table
});

/// Decode a LabPBR 1.3 `_n` / `_s` pair into canonical packed channels.
///
/// The two inputs may be `None` independently. When both are `None` the result
/// is empty and only carries metadata.
pub(crate) fn decode_labpbr13(
    normal: Option<&RgbaBuffer>,
    specular: Option<&RgbaBuffer>,
) -> Result<DecodedPbr, MaterialError> {
    if normal.is_none() && specular.is_none() {
        return Ok(DecodedPbr {
            normal_gl: None,
            orm: None,
            aux: None,
        });
    }

    let (width, height) = normal
        .map(|buffer| (buffer.width, buffer.height))
        .or_else(|| specular.map(|buffer| (buffer.width, buffer.height)))
        .unwrap_or((0, 0));

    if let Some(buffer) = normal {
        ensure_size(buffer, width, height)?;
    }
    if let Some(buffer) = specular {
        ensure_size(buffer, width, height)?;
    }
    if width == 0 || height == 0 {
        return Ok(DecodedPbr {
            normal_gl: None,
            orm: None,
            aux: None,
        });
    }

    let default_roughness = encode_unit(1.0 - DEFAULT_SMOOTHNESS);
    let total_bytes = (width as usize) * (height as usize) * 4;
    let row_stride = (width as usize) * 4;

    let mut normal_gl_pixels = normal.map(|_| vec![0u8; total_bytes]);
    let mut orm_pixels = vec![0u8; total_bytes];
    let mut aux_pixels = specular.map(|_| vec![0u8; total_bytes]);

    if let (Some(n_src), Some(s_src)) = (normal, specular) {
        let n_dst = normal_gl_pixels.as_mut().unwrap();
        let a_dst = aux_pixels.as_mut().unwrap();

        let process_row =
            |n_in: &[u8], s_in: &[u8], n_out: &mut [u8], o_out: &mut [u8], a_out: &mut [u8]| {
                for ((((n_px, s_px), n_o), o_o), a_o) in n_in
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(s_in.as_chunks::<4>().0.iter())
                    .zip(n_out.as_chunks_mut::<4>().0.iter_mut())
                    .zip(o_out.as_chunks_mut::<4>().0.iter_mut())
                    .zip(a_out.as_chunks_mut::<4>().0.iter_mut())
                {
                    let red = n_px[0];
                    let green = n_px[1];
                    let blue = n_px[2];
                    let alpha = n_px[3];

                    let normal_x = 2.0 * (red as f32 / 255.0) - 1.0;
                    let normal_y = 1.0 - 2.0 * (green as f32 / 255.0);
                    let normal_z = (1.0 - normal_x * normal_x - normal_y * normal_y)
                        .max(0.0)
                        .sqrt();

                    n_o[0] = red;
                    n_o[1] = 255 - green;
                    n_o[2] = encode_unit(0.5 * normal_z + 0.5);
                    n_o[3] = alpha;

                    let s_red = s_px[0];
                    let s_green = s_px[1];
                    let s_blue = s_px[2];
                    let s_alpha = s_px[3];

                    o_o[0] = blue; // AO from normal.B
                    o_o[1] = 255 - s_red; // Roughness from 1.0 - Smoothness
                    o_o[2] = if s_green >= 230 { 255 } else { 0 }; // Metal
                    o_o[3] = EMISSION_LUT[s_alpha as usize];

                    a_o[0] = IOR_LUT[s_green as usize];
                    a_o[1] = POROSITY_LUT[s_blue as usize];
                    a_o[2] = SSS_LUT[s_blue as usize];
                    a_o[3] = 0;
                }
            };

        #[cfg(feature = "parallel")]
        n_src
            .pixels
            .par_chunks_exact(row_stride)
            .zip(s_src.pixels.par_chunks_exact(row_stride))
            .zip(n_dst.par_chunks_exact_mut(row_stride))
            .zip(orm_pixels.par_chunks_exact_mut(row_stride))
            .zip(a_dst.par_chunks_exact_mut(row_stride))
            .for_each(|((((n_in, s_in), n_out), o_out), a_out)| {
                process_row(n_in, s_in, n_out, o_out, a_out);
            });

        #[cfg(not(feature = "parallel"))]
        n_src
            .pixels
            .chunks_exact(row_stride)
            .zip(s_src.pixels.chunks_exact(row_stride))
            .zip(n_dst.chunks_exact_mut(row_stride))
            .zip(orm_pixels.chunks_exact_mut(row_stride))
            .zip(a_dst.chunks_exact_mut(row_stride))
            .for_each(|((((n_in, s_in), n_out), o_out), a_out)| {
                process_row(n_in, s_in, n_out, o_out, a_out);
            });
    } else if let Some(n_src) = normal {
        let n_dst = normal_gl_pixels.as_mut().unwrap();

        let process_row = |n_in: &[u8], n_out: &mut [u8], o_out: &mut [u8]| {
            for ((n_px, n_o), o_o) in n_in
                .as_chunks::<4>()
                .0
                .iter()
                .zip(n_out.as_chunks_mut::<4>().0.iter_mut())
                .zip(o_out.as_chunks_mut::<4>().0.iter_mut())
            {
                let red = n_px[0];
                let green = n_px[1];
                let blue = n_px[2];
                let alpha = n_px[3];

                let normal_x = 2.0 * (red as f32 / 255.0) - 1.0;
                let normal_y = 1.0 - 2.0 * (green as f32 / 255.0);
                let normal_z = (1.0 - normal_x * normal_x - normal_y * normal_y)
                    .max(0.0)
                    .sqrt();

                n_o[0] = red;
                n_o[1] = 255 - green;
                n_o[2] = encode_unit(0.5 * normal_z + 0.5);
                n_o[3] = alpha;

                o_o[0] = blue;
                o_o[1] = default_roughness;
                o_o[2] = 0;
                o_o[3] = 0;
            }
        };

        #[cfg(feature = "parallel")]
        n_src
            .pixels
            .par_chunks_exact(row_stride)
            .zip(n_dst.par_chunks_exact_mut(row_stride))
            .zip(orm_pixels.par_chunks_exact_mut(row_stride))
            .for_each(|((n_in, n_out), o_out)| {
                process_row(n_in, n_out, o_out);
            });

        #[cfg(not(feature = "parallel"))]
        n_src
            .pixels
            .chunks_exact(row_stride)
            .zip(n_dst.chunks_exact_mut(row_stride))
            .zip(orm_pixels.chunks_exact_mut(row_stride))
            .for_each(|((n_in, n_out), o_out)| {
                process_row(n_in, n_out, o_out);
            });
    } else if let Some(s_src) = specular {
        let a_dst = aux_pixels.as_mut().unwrap();

        let process_row = |s_in: &[u8], o_out: &mut [u8], a_out: &mut [u8]| {
            for ((s_px, o_o), a_o) in s_in
                .as_chunks::<4>()
                .0
                .iter()
                .zip(o_out.as_chunks_mut::<4>().0.iter_mut())
                .zip(a_out.as_chunks_mut::<4>().0.iter_mut())
            {
                let s_red = s_px[0];
                let s_green = s_px[1];
                let s_blue = s_px[2];
                let s_alpha = s_px[3];

                o_o[0] = 255;
                o_o[1] = 255 - s_red;
                o_o[2] = if s_green >= 230 { 255 } else { 0 };
                o_o[3] = EMISSION_LUT[s_alpha as usize];

                a_o[0] = IOR_LUT[s_green as usize];
                a_o[1] = POROSITY_LUT[s_blue as usize];
                a_o[2] = SSS_LUT[s_blue as usize];
                a_o[3] = 0;
            }
        };

        #[cfg(feature = "parallel")]
        s_src
            .pixels
            .par_chunks_exact(row_stride)
            .zip(orm_pixels.par_chunks_exact_mut(row_stride))
            .zip(a_dst.par_chunks_exact_mut(row_stride))
            .for_each(|((s_in, o_out), a_out)| {
                process_row(s_in, o_out, a_out);
            });

        #[cfg(not(feature = "parallel"))]
        s_src
            .pixels
            .chunks_exact(row_stride)
            .zip(orm_pixels.chunks_exact_mut(row_stride))
            .zip(a_dst.chunks_exact_mut(row_stride))
            .for_each(|((s_in, o_out), a_out)| {
                process_row(s_in, o_out, a_out);
            });
    }

    let normal_gl = normal_gl_pixels.map(|pixels| RgbaBuffer {
        width,
        height,
        pixels,
    });
    let orm = Some(RgbaBuffer {
        width,
        height,
        pixels: orm_pixels,
    });
    let aux = aux_pixels.map(|pixels| RgbaBuffer {
        width,
        height,
        pixels,
    });

    Ok(DecodedPbr {
        normal_gl,
        orm,
        aux,
    })
}

#[inline]
fn ensure_size(buffer: &RgbaBuffer, width: u32, height: u32) -> Result<(), MaterialError> {
    if buffer.width == width && buffer.height == height {
        Ok(())
    } else {
        Err(MaterialError::PbrSizeMismatch {
            expected_width: width,
            expected_height: height,
            actual_width: buffer.width,
            actual_height: buffer.height,
        })
    }
}

/// LabPBR dielectric F0 / metal threshold -> IOR.
#[inline]
fn ior_from_specular_scalar(green: u8) -> f32 {
    if green >= 230 {
        return DEFAULT_IOR;
    }
    let f0 = green.min(229) as f32 / 255.0;
    let root = f0.sqrt();
    let denominator = 1.0 - root;
    if denominator <= 1e-6 {
        return MAX_IOR;
    }
    ((1.0 + root) / denominator).clamp(1.0, MAX_IOR)
}

/// LabPBR `_s.A`: 255 means "no data", 254 means 100%.
#[inline]
fn emission_byte_scalar(alpha: u8) -> u8 {
    if alpha == 255 {
        0
    } else {
        encode_unit(alpha.min(254) as f32 / 254.0)
    }
}

/// LabPBR `_s.B` 0..64 is porosity (64 = 100%).
#[inline]
fn porosity_byte_scalar(blue: u8) -> u8 {
    if blue <= 64 {
        encode_unit(blue as f32 / 64.0)
    } else {
        0
    }
}

/// LabPBR `_s.B` 65..255 is SSS (65 = 0%, 255 = 100%).
#[inline]
fn sss_byte_scalar(blue: u8) -> u8 {
    if blue >= 65 {
        encode_unit((blue - 65) as f32 / 190.0)
    } else {
        0
    }
}

#[inline]
pub(crate) fn encode_unit(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[inline]
pub(crate) fn encode_ior(ior: f32) -> u8 {
    encode_unit((ior.clamp(1.0, MAX_IOR) - 1.0) / (MAX_IOR - 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_ior(encoded: u8) -> f32 {
        encoded as f32 / 255.0 * 7.0 + 1.0
    }

    fn buffer(width: u32, height: u32, pixels: &[[u8; 4]]) -> RgbaBuffer {
        assert_eq!(pixels.len() as u32, width * height);
        let mut buffer = RgbaBuffer::new(width, height);
        for (index, pixel) in pixels.iter().enumerate() {
            buffer.set_pixel(index as u32 % width, index as u32 / width, *pixel);
        }
        buffer
    }

    #[test]
    fn missing_both_companions_is_empty() {
        let decoded = decode_labpbr13(None, None).expect("decode");
        assert!(decoded.normal_gl.is_none());
        assert!(decoded.orm.is_none());
        assert!(decoded.aux.is_none());
    }

    #[test]
    fn normal_only_reconstructs_gl_normal_and_default_orm() {
        let normal = buffer(2, 1, &[[128, 128, 255, 128], [255, 0, 0, 255]]);
        let decoded = decode_labpbr13(Some(&normal), None).expect("decode");
        let normal_gl = decoded.normal_gl.expect("normal_gl");
        let orm = decoded.orm.expect("orm");
        assert!(decoded.aux.is_none());

        assert_eq!(normal_gl.get_pixel(0, 0), [128, 127, 255, 128]);
        assert_eq!(normal_gl.get_pixel(1, 0), [255, 255, 128, 255]);

        // AO comes from _n.B; height is copied to normal_gl.A; default roughness 0.707.
        assert_eq!(orm.get_pixel(0, 0), [255, 180, 0, 0]);
        assert_eq!(orm.get_pixel(1, 0), [0, 180, 0, 0]);
    }

    #[test]
    fn specular_only_unpacks_orm_and_aux() {
        let specular = buffer(2, 1, &[[0, 0, 0, 0], [128, 10, 64, 254]]);
        let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
        assert!(decoded.normal_gl.is_none());
        let orm = decoded.orm.expect("orm");
        let aux = decoded.aux.expect("aux");

        // No normal -> AO defaults to 255.
        assert_eq!(orm.get_pixel(0, 0), [255, 255, 0, 0]);
        assert_eq!(aux.get_pixel(0, 0), [0, 0, 0, 0]);

        assert_eq!(orm.get_pixel(1, 0), [255, 127, 0, 255]);
        // IOR from F0=10/255 ~= 1.494 -> encoded 18; porosity 64 -> 255; SSS 0.
        assert_eq!(aux.get_pixel(1, 0), [18, 255, 0, 0]);
        assert!((decode_ior(18) - 1.494).abs() < 0.01);
    }

    #[test]
    fn both_companions_keep_ao_metal_and_sss() {
        let normal = buffer(1, 1, &[[128, 128, 255, 255]]);
        let specular = buffer(1, 1, &[[255, 230, 255, 128]]);
        let decoded = decode_labpbr13(Some(&normal), Some(&specular)).expect("decode");
        let orm = decoded.orm.expect("orm");
        let aux = decoded.aux.expect("aux");

        assert_eq!(orm.get_pixel(0, 0), [255, 0, 255, 129]);
        // Metal -> IOR 1.5; B=255 -> porosity 0, SSS 1.0.
        assert_eq!(aux.get_pixel(0, 0), [18, 0, 255, 0]);
    }

    #[test]
    fn emission_255_means_no_data() {
        let specular = buffer(3, 1, &[[0, 0, 0, 255], [0, 0, 0, 254], [0, 0, 0, 128]]);
        let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
        let orm = decoded.orm.expect("orm");
        assert_eq!(orm.get_pixel(0, 0)[3], 0);
        assert_eq!(orm.get_pixel(1, 0)[3], 255);
        assert_eq!(orm.get_pixel(2, 0)[3], 129);
    }

    #[test]
    fn porosity_and_sss_split_at_64_65() {
        let specular = buffer(3, 1, &[[0, 0, 64, 0], [0, 0, 65, 0], [0, 0, 255, 0]]);
        let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
        let aux = decoded.aux.expect("aux");

        assert_eq!(aux.get_pixel(0, 0)[1], 255);
        assert_eq!(aux.get_pixel(0, 0)[2], 0);

        assert_eq!(aux.get_pixel(1, 0)[1], 0);
        assert_eq!(aux.get_pixel(1, 0)[2], 0);

        assert_eq!(aux.get_pixel(2, 0)[1], 0);
        assert_eq!(aux.get_pixel(2, 0)[2], 255);
    }

    #[test]
    fn ior_is_clamped_above_eight() {
        // F0 = 229/255 is the largest dielectric value and maps far above IOR 8.
        let specular = buffer(2, 1, &[[0, 229, 0, 0], [0, 230, 0, 0]]);
        let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
        let aux = decoded.aux.expect("aux");

        assert_eq!(aux.get_pixel(0, 0)[0], 255);
        assert!((decode_ior(aux.get_pixel(0, 0)[0]) - 8.0).abs() < 1e-5);
        // Metal preset keeps the neutral 1.5 IOR (8-bit encoded).
        assert!((decode_ior(aux.get_pixel(1, 0)[0]) - 1.5).abs() < 0.01);
    }

    #[test]
    fn mismatched_dimensions_are_rejected() {
        let normal = RgbaBuffer::new(2, 1);
        let specular = RgbaBuffer::new(1, 1);
        let error = decode_labpbr13(Some(&normal), Some(&specular)).expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("2x1"), "{message}");
        assert!(message.contains("1x1"), "{message}");
    }
}
