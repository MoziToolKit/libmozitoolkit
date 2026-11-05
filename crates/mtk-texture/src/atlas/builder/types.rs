use crate::{AtlasAddressMap, RgbaBuffer};

/// Configuration parameters for atlas generation.
#[derive(Debug, Clone)]
pub struct AtlasBuilderConfig {
    pub max_width: u32,
    pub max_height: u32,
    pub mip_level: u32,
    pub padding: u32,
}

impl Default for AtlasBuilderConfig {
    fn default() -> Self {
        Self {
            max_width: 4096,
            max_height: 4096,
            mip_level: 0,
            padding: 0,
        }
    }
}

/// A complete baked atlas sheet (chunk) with all active PBR buffers.
#[derive(Debug, Clone)]
pub struct BakedAtlasChunk {
    pub chunk_id: u16,
    pub category: String,
    pub is_animated: bool,
    pub category_chunk_index: usize,
    pub width: u32,
    pub height: u32,
    pub albedo: RgbaBuffer,
    /// Raw LabPBR `_n` companion (legacy in-memory access; not written to new caches).
    pub normal: Option<RgbaBuffer>,
    /// Raw LabPBR `_s` companion (legacy in-memory access; not written to new caches).
    pub specular: Option<RgbaBuffer>,
    pub overlay: Option<RgbaBuffer>,
    /// Decoded OpenGL normal RGB + height A (`MtkPackedV1`).
    pub normal_gl: Option<RgbaBuffer>,
    /// Decoded AO / roughness / metallic / emission mask (`MtkPackedV1`).
    pub orm: Option<RgbaBuffer>,
    /// Decoded IOR / porosity / SSS / transmission mask (`MtkPackedV1`).
    pub aux: Option<RgbaBuffer>,
}

impl BakedAtlasChunk {
    /// Canonical file stem for this atlas sheet, e.g. `"blocks_chunk_001"` or `"blocks_anim_chunk_001"`.
    pub fn file_stem(&self) -> String {
        if self.is_animated {
            format!(
                "{}_anim_chunk_{:03}",
                self.category, self.category_chunk_index
            )
        } else {
            format!("{}_chunk_{:03}", self.category, self.category_chunk_index)
        }
    }
}

/// Complete output containing all baked atlas sheets and authoritative address table.
#[derive(Debug, Clone)]
pub struct BakedAtlas {
    pub chunks: Vec<BakedAtlasChunk>,
    pub address_map: AtlasAddressMap,
}

impl BakedAtlas {
    /// Total number of baked atlas sheets / chunks.
    pub fn chunk_count(&self) -> usize {
        if !self.chunks.is_empty() {
            self.chunks.len()
        } else {
            self.address_map.chunks.len()
        }
    }

    /// Lookup chunk descriptor from address map by chunk ID.
    pub fn get_chunk_meta(
        &self,
        chunk_id: u16,
    ) -> Option<&crate::atlas::address_map::AtlasChunkMeta> {
        self.address_map.get_chunk_meta(chunk_id)
    }
}

/// Top-level coordinator for building vanilla Minecraft atlases with PBR sync.
pub struct AtlasBuilder {
    pub(crate) config: AtlasBuilderConfig,
}

/// Default OpenGL Normal buffer fill color: flat tangent-space normal (X=0.5, Y=0.5, Z=1.0, Height=1.0).
pub const NORMAL_DEFAULT_RGBA: [u8; 4] = [128, 128, 255, 255];

/// Default ORM buffer fill color: AO=1.0, Roughness=1.0, Metallic=0.0, Emission=0.0.
pub const ORM_DEFAULT_RGBA: [u8; 4] = [255, 255, 0, 0];

/// Unified container for companion PBR layers during atlas chunk assembly.
#[derive(Debug, Default)]
pub struct PbrLayerBuffers {
    pub normal: Option<RgbaBuffer>,
    pub specular: Option<RgbaBuffer>,
    pub overlay: Option<RgbaBuffer>,
    pub normal_gl: Option<RgbaBuffer>,
    pub orm: Option<RgbaBuffer>,
    pub aux: Option<RgbaBuffer>,
}

impl PbrLayerBuffers {
    /// Constructs buffers for all active PBR layers with standard neutral fill colors.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: u32,
        height: u32,
        has_normal: bool,
        has_specular: bool,
        has_overlay: bool,
        has_normal_gl: bool,
        has_orm: bool,
        has_aux: bool,
    ) -> Self {
        Self {
            normal: if has_normal {
                Some(RgbaBuffer::solid(
                    width,
                    height,
                    NORMAL_DEFAULT_RGBA[0],
                    NORMAL_DEFAULT_RGBA[1],
                    NORMAL_DEFAULT_RGBA[2],
                    NORMAL_DEFAULT_RGBA[3],
                ))
            } else {
                None
            },
            specular: if has_specular {
                Some(RgbaBuffer::new(width, height))
            } else {
                None
            },
            overlay: if has_overlay {
                Some(RgbaBuffer::new(width, height))
            } else {
                None
            },
            normal_gl: if has_normal_gl {
                Some(RgbaBuffer::solid(
                    width,
                    height,
                    NORMAL_DEFAULT_RGBA[0],
                    NORMAL_DEFAULT_RGBA[1],
                    NORMAL_DEFAULT_RGBA[2],
                    NORMAL_DEFAULT_RGBA[3],
                ))
            } else {
                None
            },
            orm: if has_orm {
                Some(RgbaBuffer::solid(
                    width,
                    height,
                    ORM_DEFAULT_RGBA[0],
                    ORM_DEFAULT_RGBA[1],
                    ORM_DEFAULT_RGBA[2],
                    ORM_DEFAULT_RGBA[3],
                ))
            } else {
                None
            },
            aux: if has_aux {
                Some(RgbaBuffer::new(width, height))
            } else {
                None
            },
        }
    }

    /// Blits all active PBR companion layers of a sprite with padding.
    pub fn blit_sprite_layers(
        &mut self,
        sp: &crate::DecodedSprite,
        inner_x: u32,
        inner_y: u32,
        width: u32,
        height: u32,
        padding: u32,
    ) {
        super::blit_and_pad_optional_layer(
            &mut self.normal,
            &sp.normal,
            inner_x,
            inner_y,
            width,
            height,
            padding,
        );
        super::blit_and_pad_optional_layer(
            &mut self.specular,
            &sp.specular,
            inner_x,
            inner_y,
            width,
            height,
            padding,
        );
        super::blit_and_pad_optional_layer(
            &mut self.overlay,
            &sp.overlay,
            inner_x,
            inner_y,
            width,
            height,
            padding,
        );
        super::blit_and_pad_optional_layer(
            &mut self.normal_gl,
            &sp.normal_gl,
            inner_x,
            inner_y,
            width,
            height,
            padding,
        );
        super::blit_and_pad_optional_layer(
            &mut self.orm,
            &sp.orm,
            inner_x,
            inner_y,
            width,
            height,
            padding,
        );
        super::blit_and_pad_optional_layer(
            &mut self.aux,
            &sp.aux,
            inner_x,
            inner_y,
            width,
            height,
            padding,
        );
    }
}
