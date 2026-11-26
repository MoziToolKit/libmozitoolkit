use crate::error::TextureError;
use crate::image::buffer::RgbaBuffer;
use crate::image::palette::bake_paletted_permutation;
use mtk_resource::{AnimationMetadata, DiscoveredSprite, ResourceLocation, UnstitchRegionSpec};

/// A fully decoded sprite with single-frame metrics and optional PBR companions.
#[derive(Debug, Clone)]
pub struct DecodedSprite {
    pub sprite_id: ResourceLocation,
    pub albedo: RgbaBuffer,
    pub normal: Option<RgbaBuffer>,
    pub specular: Option<RgbaBuffer>,
    pub overlay: Option<RgbaBuffer>,
    pub emissive: Option<RgbaBuffer>,
    pub normal_gl: Option<RgbaBuffer>,
    pub orm: Option<RgbaBuffer>,
    pub aux: Option<RgbaBuffer>,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u32,
    pub metadata: Option<AnimationMetadata>,
}

fn crop_unstitch_layer(
    buf: RgbaBuffer,
    base_w: u32,
    base_h: u32,
    spec: &UnstitchRegionSpec,
) -> RgbaBuffer {
    if base_w == 0 || base_h == 0 || buf.width == 0 || buf.height == 0 {
        return buf;
    }
    let scale_x = buf.width as f64 / spec.divisor_x.max(1e-9);
    let scale_y = buf.height as f64 / spec.divisor_y.max(1e-9);
    let x = (spec.x * scale_x).floor().max(0.0) as u32;
    let y = (spec.y * scale_y).floor().max(0.0) as u32;
    let w = (spec.width * scale_x).floor().max(1.0) as u32;
    let h = (spec.height * scale_y).floor().max(1.0) as u32;
    let cx = x.min(buf.width.saturating_sub(1));
    let cy = y.min(buf.height.saturating_sub(1));
    let cw = w.min(buf.width - cx).max(1);
    let ch = h.min(buf.height - cy).max(1);
    buf.crop(cx, cy, cw, ch)
}

/// Unwraps a multi-column animation grid (`cols > 1`) into a single vertical strip.
fn unwrap_grid_strip(buf: RgbaBuffer, frame_w: u32, frame_h: u32) -> RgbaBuffer {
    let cols = buf.width.checked_div(frame_w).unwrap_or(1).max(1);
    let rows = buf.height.checked_div(frame_h).unwrap_or(1).max(1);
    if cols <= 1 {
        return buf;
    }
    let total = cols * rows;
    let mut strip = RgbaBuffer::new(frame_w, frame_h * total);
    for idx in 0..total {
        let src_x = (idx % cols) * frame_w;
        let src_y = (idx / cols) * frame_h;
        strip.blit(&buf, src_x, src_y, 0, idx * frame_h, frame_w, frame_h);
    }
    strip
}

/// Computes single-frame dimensions matching `AnimationMetadataSection.calculateFrameSize`.
fn calculate_frame_size(meta: &AnimationMetadata, img_w: u32, img_h: u32) -> (u32, u32) {
    match (meta.width, meta.height) {
        (Some(w), Some(h)) => (w.max(1), h.max(1)),
        (Some(w), None) => (w.max(1), img_h.max(1)),
        (None, Some(h)) => (img_w.max(1), h.max(1)),
        (None, None) => {
            let min_dim = img_w.min(img_h).max(1);
            (min_dim, min_dim)
        }
    }
}

impl DecodedSprite {
    /// Decode a discovered sprite entry from its raw PNG byte buffers.
    pub fn from_discovered(discovered: DiscoveredSprite) -> Result<Self, TextureError> {
        let raw_albedo = discovered.raw_albedo.ok_or_else(|| {
            TextureError::Baking(format!(
                "Missing albedo bytes for sprite '{}'",
                discovered.sprite_id
            ))
        })?;

        let mut albedo = RgbaBuffer::from_png_bytes(&raw_albedo)?;
        let mut normal = match discovered.raw_normal {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };
        let mut specular = match discovered.raw_specular {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };
        let mut overlay = match discovered.raw_overlay {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };
        let mut emissive_raw = match discovered.raw_emissive {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };

        // Apply Unstitcher crop before frame/companion alignment
        if let Some(ref spec) = discovered.unstitch {
            let base_w = albedo.width;
            let base_h = albedo.height;
            albedo = crop_unstitch_layer(albedo, base_w, base_h, spec);
            normal = normal.map(|b| crop_unstitch_layer(b, base_w, base_h, spec));
            specular = specular.map(|b| crop_unstitch_layer(b, base_w, base_h, spec));
            overlay = overlay.map(|b| crop_unstitch_layer(b, base_w, base_h, spec));
            emissive_raw = emissive_raw.map(|b| crop_unstitch_layer(b, base_w, base_h, spec));
        }

        // Apply PalettedPermutations remapping if present
        if let Some(ref pal) = discovered.palette {
            let key_img = RgbaBuffer::from_png_bytes(&pal.raw_palette_key)?;
            let perm_img = RgbaBuffer::from_png_bytes(&pal.raw_permutation)?;
            albedo = bake_paletted_permutation(&albedo, &key_img, &perm_img)?;
        }

        // Determine initial single-frame dimensions from base albedo
        let mut final_metadata = discovered.metadata;
        let (mut frame_width, mut frame_height, mut frame_count) =
            if let Some(ref meta) = final_metadata {
                let (fw, fh) = calculate_frame_size(meta, albedo.width, albedo.height);
                if fw > 0 && fh > 0 && albedo.width % fw == 0 && albedo.height % fh == 0 {
                    let cols = (albedo.width / fw).max(1);
                    let rows = (albedo.height / fh).max(1);
                    let fc = (cols * rows).max(1);
                    albedo = unwrap_grid_strip(albedo, fw, fh);
                    (fw, fh, fc)
                } else {
                    (albedo.width, albedo.height, 1)
                }
            } else {
                (albedo.width, albedo.height, 1)
            };

        // If this sprite has an emissive companion (e.g. firefly_bush_emissive on firefly_bush)
        // and the emissive layer is animated while albedo is static, expand the static albedo
        // into a multi-frame animation matching the emissive layer before blending.
        let emissive = if let Some(mut em) = emissive_raw {
            if frame_count <= 1 && (discovered.emissive_metadata.is_some() || em.height > em.width)
            {
                let (em_fw, em_fh) = if let Some(ref em_meta) = discovered.emissive_metadata {
                    calculate_frame_size(em_meta, em.width, em.height)
                } else if em.height >= em.width && em.width > 0 {
                    (em.width, em.width)
                } else {
                    (em.width, em.height)
                };
                let (em_fw, em_fh, em_fc) =
                    if em_fw > 0 && em_fh > 0 && em.width % em_fw == 0 && em.height % em_fh == 0 {
                        let cols = (em.width / em_fw).max(1);
                        let rows = (em.height / em_fh).max(1);
                        em = unwrap_grid_strip(em, em_fw, em_fh);
                        (em_fw, em_fh, (cols * rows).max(1))
                    } else {
                        (em.width, em.height, 1)
                    };

                // Resize base albedo frame to match emissive frame dimensions, then tile vertically
                if albedo.width != em_fw || albedo.height != em_fh {
                    albedo = albedo.resize_nearest(em_fw, em_fh);
                }
                albedo = albedo.tile_vertical(em_fh * em_fc);

                // Blend the animated emissive frames over the tiled albedo
                albedo.blend_over(&em);

                frame_width = em_fw;
                frame_height = em_fh;
                frame_count = em_fc;
                final_metadata = discovered.emissive_metadata;

                Some(em)
            } else {
                let aligned_em =
                    em.align_companion_to_albedo(frame_width, frame_height, frame_count);
                albedo.blend_over(&aligned_em);
                Some(aligned_em)
            }
        } else {
            None
        };

        // Align PBR companion buffers with albedo frame metrics and vertical tiling
        let normal =
            normal.map(|n| n.align_companion_to_albedo(frame_width, frame_height, frame_count));
        let specular =
            specular.map(|s| s.align_companion_to_albedo(frame_width, frame_height, frame_count));
        let overlay =
            overlay.map(|o| o.align_companion_to_albedo(frame_width, frame_height, frame_count));

        Ok(Self {
            sprite_id: discovered.sprite_id,
            albedo,
            normal,
            specular,
            overlay,
            emissive,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width,
            frame_height,
            frame_count,
            metadata: final_metadata,
        })
    }

    /// Convenience constructor for a basic single-frame sprite without PBR companions.
    pub fn simple(
        sprite_id: ResourceLocation,
        albedo: RgbaBuffer,
        frame_width: u32,
        frame_height: u32,
    ) -> Self {
        Self {
            sprite_id,
            albedo,
            normal: None,
            specular: None,
            overlay: None,
            emissive: None,
            normal_gl: None,
            orm: None,
            aux: None,
            frame_width,
            frame_height,
            frame_count: 1,
            metadata: None,
        }
    }

    /// Parallel or sequential batch decode of discovered sprites.
    pub fn decode_batch(
        sprites: Vec<DiscoveredSprite>,
    ) -> Result<Vec<DecodedSprite>, TextureError> {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            mtk_core::threading::install(|| {
                sprites.into_par_iter().map(Self::from_discovered).collect()
            })
        }

        #[cfg(not(feature = "parallel"))]
        {
            sprites.into_iter().map(Self::from_discovered).collect()
        }
    }
}
