//! # Atlas Builder and PBR Stitching Pipeline
//!
//! Provides multi-sheet packing, static/animated separation, PBR companion
//! layer alignment, and paletted permutations baking.

pub mod anim_atlas;
pub mod paletted;
pub mod static_atlas;
pub mod types;

pub use anim_atlas::build_anim_atlas_chunks;
pub use paletted::process_paletted_permutations;
pub use static_atlas::build_static_atlas_chunks;
pub use types::*;

use crate::atlas::address_map::AtlasAddressMap;
use crate::error::TextureError;
use crate::image::loader::DecodedSprite;
use mtk_resource::{AtlasDefinition, ResourcePackStack};

impl AtlasBuilder {
    pub fn new(config: AtlasBuilderConfig) -> Self {
        Self { config }
    }

    /// Bake an atlas given a ResourcePackStack and an AtlasDefinition (defaults to "blocks" category).
    pub fn build(
        &self,
        stack: &ResourcePackStack,
        definition: &AtlasDefinition,
    ) -> Result<BakedAtlas, TextureError> {
        self.build_with_category(stack, definition, "blocks")
    }

    /// Bake an atlas for a specific category (e.g. `AtlasCategory::Blocks`, `AtlasCategory::Items`).
    pub fn build_category(
        &self,
        stack: &ResourcePackStack,
        category: &mtk_resource::AtlasCategory,
    ) -> Result<BakedAtlas, TextureError> {
        let definition = stack.load_atlas_category(category);
        self.build_with_category(stack, &definition, category.as_str())
    }

    /// Bake multiple atlas categories into a single unified BakedAtlas with unified AddressMap.
    pub fn build_categories(
        &self,
        stack: &ResourcePackStack,
        definitions: &[(String, AtlasDefinition)],
    ) -> Result<BakedAtlas, TextureError> {
        let mut baked_chunks = Vec::new();
        let mut address_map = AtlasAddressMap::new();
        let mut global_chunk_counter = 0u16;
        let mut texture_id_counter = 0u32;

        for (category_name, definition) in definitions {
            // 1. Discover raw sprites (including CTM tiles for blocks)
            let discovered = if category_name == "blocks" {
                let ctm_rules = stack.load_ctm_rules();
                if !ctm_rules.is_empty() {
                    stack.collect_sprites_including_ctm(definition, &ctm_rules)?
                } else {
                    stack.collect_sprites_for_atlas(definition)?
                }
            } else {
                stack.collect_sprites_for_atlas(definition)?
            };
            if discovered.is_empty() && definition.sources.is_empty() {
                continue;
            }

            // 2. Decode discovered sprites in parallel
            let mut decoded = DecodedSprite::decode_batch(discovered)?;

            // 3. Process PalettedPermutations sources if any
            process_paletted_permutations(&definition.sources, stack, &mut decoded);

            if decoded.is_empty() {
                continue;
            }

            // Filter out sprites that have already been allocated to an authoritative atlas
            decoded.retain(|sp| !address_map.sprites.contains_key(&sp.sprite_id));

            // Dedicated independent atlases (e.g. banner_patterns, decorated_pot, chests, etc.)
            // must never be swept into the catch-all "entities" sheet.
            if category_name == "entities" {
                decoded.retain(|sp| {
                    let cat =
                        mtk_resource::AtlasCategory::classify_texture_path(&sp.sprite_id.path);
                    cat == mtk_resource::AtlasCategory::Entities
                        || cat == mtk_resource::AtlasCategory::Misc
                });
            }

            if decoded.is_empty() {
                continue;
            }

            // 4. Separate static and anim sprites
            let (static_sprites, anim_sprites) = separate_static_and_anim_sprites(decoded);

            // 5. Build Static Atlas Chunks
            build_static_atlas_chunks(
                static_sprites,
                &self.config,
                category_name,
                &mut global_chunk_counter,
                &mut texture_id_counter,
                &mut baked_chunks,
                &mut address_map,
            )?;

            // 6. Build Dedicated Animated Atlas Chunks (Full Strips + PBR Tiling)
            build_anim_atlas_chunks(
                anim_sprites,
                &self.config,
                category_name,
                &mut global_chunk_counter,
                &mut texture_id_counter,
                &mut baked_chunks,
                &mut address_map,
            )?;
        }

        Ok(BakedAtlas {
            chunks: baked_chunks,
            address_map,
        })
    }

    /// Bake an atlas with an explicit category name string.
    pub fn build_with_category(
        &self,
        stack: &ResourcePackStack,
        definition: &AtlasDefinition,
        category_name: &str,
    ) -> Result<BakedAtlas, TextureError> {
        self.build_categories(stack, &[(category_name.to_string(), definition.clone())])
    }

    /// Build directly from pre-decoded sprites (defaults to "blocks" category).
    pub fn build_from_sprites(
        &self,
        sprites: Vec<DecodedSprite>,
    ) -> Result<BakedAtlas, TextureError> {
        self.build_from_sprites_with_category(sprites, "blocks")
    }

    /// Build directly from pre-decoded sprites with an explicit category name,
    /// separating static and animated sprites into isolated chunks.
    pub fn build_from_sprites_with_category(
        &self,
        sprites: Vec<DecodedSprite>,
        category_name: &str,
    ) -> Result<BakedAtlas, TextureError> {
        let (static_sprites, anim_sprites) = separate_static_and_anim_sprites(sprites);

        let mut baked_chunks = Vec::new();
        let mut address_map = AtlasAddressMap::new();
        let mut global_chunk_counter = 0u16;
        let mut texture_id_counter = 0u32;

        build_static_atlas_chunks(
            static_sprites,
            &self.config,
            category_name,
            &mut global_chunk_counter,
            &mut texture_id_counter,
            &mut baked_chunks,
            &mut address_map,
        )?;

        build_anim_atlas_chunks(
            anim_sprites,
            &self.config,
            category_name,
            &mut global_chunk_counter,
            &mut texture_id_counter,
            &mut baked_chunks,
            &mut address_map,
        )?;

        Ok(BakedAtlas {
            chunks: baked_chunks,
            address_map,
        })
    }
}

/// Helper function to separate sprites into static frame 0 representations and full animation strips.
fn separate_static_and_anim_sprites(
    sprites: Vec<DecodedSprite>,
) -> (Vec<DecodedSprite>, Vec<DecodedSprite>) {
    let mut static_sprites = Vec::with_capacity(sprites.len());
    let mut anim_sprites = Vec::new();

    for sp in sprites {
        let static_sp = if sp.frame_count > 1 {
            DecodedSprite {
                sprite_id: sp.sprite_id.clone(),
                albedo: sp.albedo.crop(0, 0, sp.frame_width, sp.frame_height),
                normal: sp.normal.as_ref().map(|n| {
                    let h = n.height.min(sp.frame_height);
                    n.crop(0, 0, n.width.min(sp.frame_width), h)
                }),
                specular: sp.specular.as_ref().map(|s| {
                    let h = s.height.min(sp.frame_height);
                    s.crop(0, 0, s.width.min(sp.frame_width), h)
                }),
                overlay: sp.overlay.as_ref().map(|o| {
                    let h = o.height.min(sp.frame_height);
                    o.crop(0, 0, o.width.min(sp.frame_width), h)
                }),
                emissive: sp.emissive.as_ref().map(|e| {
                    let h = e.height.min(sp.frame_height);
                    e.crop(0, 0, e.width.min(sp.frame_width), h)
                }),
                normal_gl: sp.normal_gl.as_ref().map(|n| {
                    let h = n.height.min(sp.frame_height);
                    n.crop(0, 0, n.width.min(sp.frame_width), h)
                }),
                orm: sp.orm.as_ref().map(|o| {
                    let h = o.height.min(sp.frame_height);
                    o.crop(0, 0, o.width.min(sp.frame_width), h)
                }),
                aux: sp.aux.as_ref().map(|a| {
                    let h = a.height.min(sp.frame_height);
                    a.crop(0, 0, a.width.min(sp.frame_width), h)
                }),
                frame_width: sp.frame_width,
                frame_height: sp.frame_height,
                frame_count: 1,
                metadata: None,
            }
        } else {
            sp.clone()
        };
        static_sprites.push(static_sp);

        if sp.frame_count > 1 {
            anim_sprites.push(sp);
        }
    }

    (static_sprites, anim_sprites)
}

/// Helper to blit and apply edge-clamping padding to an optional layer buffer.
pub(crate) fn blit_and_pad_optional_layer(
    dest: &mut Option<crate::image::buffer::RgbaBuffer>,
    src: &Option<crate::image::buffer::RgbaBuffer>,
    inner_x: u32,
    inner_y: u32,
    width: u32,
    height: u32,
    padding: u32,
) {
    if let (Some(dest_buf), Some(src_buf)) = (dest.as_mut(), src.as_ref()) {
        dest_buf.blit(src_buf, 0, 0, inner_x, inner_y, width, height);
        if padding > 0 {
            crate::image::padding::apply_edge_clamping_padding(
                dest_buf, inner_x, inner_y, width, height, padding,
            );
        }
    }
}
