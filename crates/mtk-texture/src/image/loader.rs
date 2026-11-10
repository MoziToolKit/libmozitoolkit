use crate::error::TextureError;
use crate::image::buffer::RgbaBuffer;
use mtk_resource::{AnimationMetadata, DiscoveredSprite, ResourceLocation};

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
        let normal = match discovered.raw_normal {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };
        let specular = match discovered.raw_specular {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };
        let overlay = match discovered.raw_overlay {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };
        let emissive_raw = match discovered.raw_emissive {
            Some(ref bytes) => RgbaBuffer::from_png_bytes(bytes).ok(),
            None => None,
        };

        // Determine initial single-frame dimensions from base albedo
        let mut final_metadata = discovered.metadata;
        let (mut frame_width, mut frame_height, mut frame_count) =
            if let Some(ref meta) = final_metadata {
                let fw = meta.width.unwrap_or(albedo.width);
                let fh =
                    meta.height
                        .unwrap_or(if albedo.height >= albedo.width && albedo.width > 0 {
                            albedo.width
                        } else {
                            albedo.height
                        });
                let fc = albedo.height.checked_div(fh).unwrap_or(1);
                (fw, fh, fc.max(1))
            } else {
                (albedo.width, albedo.height, 1)
            };

        // If this sprite has an emissive companion (e.g. firefly_bush_emissive on firefly_bush)
        // and the emissive layer is animated while albedo is static, expand the static albedo
        // into a multi-frame animation matching the emissive layer before blending.
        let emissive = if let Some(em) = emissive_raw {
            if frame_count <= 1 && (discovered.emissive_metadata.is_some() || em.height > em.width)
            {
                let em_fw = em.width;
                let em_fh = if let Some(ref em_meta) = discovered.emissive_metadata {
                    em_meta.height.unwrap_or(em_fw)
                } else if em.height >= em_fw && em_fw > 0 {
                    em_fw
                } else {
                    em.height
                };
                let em_fc = em.height.checked_div(em_fh).unwrap_or(1).max(1);

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
            sprites.into_par_iter().map(Self::from_discovered).collect()
        }

        #[cfg(not(feature = "parallel"))]
        {
            sprites.into_iter().map(Self::from_discovered).collect()
        }
    }
}
