//! # Atlas Builder and PBR Stitching Pipeline
//!
//! Provides multi-sheet packing, static/animated separation, PBR companion
//! layer alignment, and paletted permutations baking.

mod anim_atlas;
mod static_atlas;
mod types;

pub(crate) use anim_atlas::build_anim_atlas_chunks;
pub(crate) use static_atlas::build_static_atlas_chunks;
pub use types::*;

use crate::error::TextureError;
use crate::{AtlasAddressMap, DecodedSprite, RgbaBuffer};
use mtk_resource::{AnimationFrame, AtlasDefinition, ResourcePackStack};
use std::collections::{HashMap, HashSet};

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
        let definition = stack.load_atlas_category(category)?;
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
        let mut skipped_animations = Vec::new();
        let mut global_chunk_counter = 0u16;
        let mut texture_id_counter = 0u32;

        for (category_name, definition) in definitions {
            // 1. Discover raw sprites (including CTM tiles for blocks)
            let discovered = if category_name == "blocks" {
                let ctm_rules = stack.load_ctm_rules()?;
                if !ctm_rules.is_empty() {
                    address_map.ctm_rules = ctm_rules.clone();
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

            if decoded.is_empty() {
                continue;
            }

            // Dedicated independent atlases (e.g. banner_patterns, decorated_pot, chests, etc.)
            // and block-entity sprites in `blocks` must never be duplicated into the synthetic
            // catch-all "entities" sheet. Standard Minecraft atlases (items, chests, shulker_boxes,
            // etc.) remain isolated per AtlasManager.java and are never suppressed by `blocks`.
            if category_name == "entities" {
                decoded.retain(|sp| {
                    if address_map.sprites.contains_key(&sp.sprite_id) {
                        return false;
                    }
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
                &mut skipped_animations,
            )?;
        }

        skipped_animations.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.sprite_id.cmp(&b.sprite_id))
        });

        Ok(BakedAtlas {
            chunks: baked_chunks,
            address_map,
            skipped_animations,
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
        let mut skipped_animations = Vec::new();
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
            &mut skipped_animations,
        )?;

        skipped_animations.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.sprite_id.cmp(&b.sprite_id))
        });

        Ok(BakedAtlas {
            chunks: baked_chunks,
            address_map,
            skipped_animations,
        })
    }
}

/// Compacts animated sprite strips to only unique referenced frames (matching `SpriteContents.uniqueFrames`),
/// ordered by first appearance in `.mcmeta` `frames` so Frame 0 in the strip is always the first played frame.
///
/// Repeated frames (e.g. ping-pong animations) are NOT duplicated into the atlas image strip;
/// instead `metadata.frames` preserves the remapped playback schedule when it differs from `0..N-1`.
fn compact_animation_unique_frames(sprites: Vec<DecodedSprite>) -> Vec<DecodedSprite> {
    sprites
        .into_iter()
        .map(|mut sprite| {
            let Some((unique_order, remapped_frames)) = compute_unique_frame_plan(&sprite) else {
                return sprite;
            };
            let frame_height = sprite.frame_height;
            let source_frames = sprite.frame_count;
            if frame_height == 0 {
                return sprite;
            }

            let is_identity = unique_order.len() == source_frames as usize
                && unique_order
                    .iter()
                    .enumerate()
                    .all(|(i, &index)| index == i as u32);
            if !is_identity {
                sprite.albedo = reorder_strip(&sprite.albedo, &unique_order, frame_height);
                sprite.normal = sprite
                    .normal
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.specular = sprite
                    .specular
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.overlay = sprite
                    .overlay
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.emissive = sprite
                    .emissive
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.normal_gl = sprite
                    .normal_gl
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.orm = sprite
                    .orm
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.aux = sprite
                    .aux
                    .as_ref()
                    .map(|buf| reorder_companion(buf, &unique_order, frame_height, source_frames));
                sprite.frame_count = unique_order.len() as u32;
            }

            if let Some(metadata) = sprite.metadata.as_mut() {
                metadata.frames = remapped_frames;
            }
            sprite
        })
        .collect()
}

/// Computes `(unique_source_indices, remapped_schedule)` matching `SpriteContents.uniqueFrames`.
///
/// Returns `None` when the sprite has no `frames` override or no valid frame entries.
fn compute_unique_frame_plan(
    sprite: &DecodedSprite,
) -> Option<(Vec<u32>, Option<Vec<AnimationFrame>>)> {
    let frames = sprite.metadata.as_ref()?.frames.as_ref()?;
    if frames.is_empty() || sprite.frame_count == 0 {
        return None;
    }

    let valid: Vec<AnimationFrame> = frames
        .iter()
        .filter(|frame| {
            let valid_idx = frame.index() < sprite.frame_count;
            let valid_time = match frame {
                AnimationFrame::Index(_) => true,
                AnimationFrame::Detailed { time, .. } => *time > 0,
            };
            valid_idx && valid_time
        })
        .cloned()
        .collect();

    if valid.is_empty() {
        return None;
    }

    let mut seen = HashSet::new();
    let mut unique_indices = Vec::new();
    let mut index_remap = HashMap::new();
    for frame in &valid {
        let orig = frame.index();
        if seen.insert(orig) {
            let new_idx = unique_indices.len() as u32;
            unique_indices.push(orig);
            index_remap.insert(orig, new_idx);
        }
    }

    let remapped: Vec<AnimationFrame> = valid
        .into_iter()
        .map(|frame| match frame {
            AnimationFrame::Index(idx) => AnimationFrame::Index(index_remap[&idx]),
            AnimationFrame::Detailed { index, time } => AnimationFrame::Detailed {
                index: index_remap[&index],
                time,
            },
        })
        .collect();

    let is_sequential = remapped.len() == unique_indices.len()
        && remapped
            .iter()
            .enumerate()
            .all(|(i, f)| matches!(f, AnimationFrame::Index(idx) if *idx == i as u32));

    let final_frames = if is_sequential { None } else { Some(remapped) };
    Some((unique_indices, final_frames))
}

fn reorder_strip(source: &RgbaBuffer, order: &[u32], frame_height: u32) -> RgbaBuffer {
    let mut out = RgbaBuffer::new(source.width, frame_height * order.len() as u32);
    for (destination, &frame) in order.iter().enumerate() {
        out.blit(
            source,
            0,
            frame * frame_height,
            0,
            destination as u32 * frame_height,
            source.width,
            frame_height,
        );
    }
    out
}

/// Reorders a companion layer when it is a full strip; single-frame companions are tiled later.
fn reorder_companion(
    source: &RgbaBuffer,
    order: &[u32],
    frame_height: u32,
    source_frames: u32,
) -> RgbaBuffer {
    if source.height == source_frames * frame_height {
        reorder_strip(source, order, frame_height)
    } else {
        source.clone()
    }
}

fn separate_static_and_anim_sprites(
    sprites: Vec<DecodedSprite>,
) -> (Vec<DecodedSprite>, Vec<DecodedSprite>) {
    let sprites = compact_animation_unique_frames(sprites);
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
    dest: &mut Option<crate::RgbaBuffer>,
    src: &Option<crate::RgbaBuffer>,
    inner_x: u32,
    inner_y: u32,
    width: u32,
    height: u32,
    padding: u32,
) {
    if let (Some(dest_buf), Some(src_buf)) = (dest.as_mut(), src.as_ref()) {
        dest_buf.blit(src_buf, 0, 0, inner_x, inner_y, width, height);
        if padding > 0 {
            crate::image::apply_edge_clamping_padding(
                dest_buf, inner_x, inner_y, width, height, padding,
            );
        }
    }
}
