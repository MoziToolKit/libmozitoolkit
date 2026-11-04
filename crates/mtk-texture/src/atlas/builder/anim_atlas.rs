use std::collections::HashMap;

use mtk_resource::ResourceLocation;

use super::types::{AtlasBuilderConfig, BakedAtlasChunk};
use crate::atlas::address_map::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, SpriteKind};
use crate::error::TextureError;
use crate::image::buffer::RgbaBuffer;
use crate::image::loader::DecodedSprite;
use crate::image::padding::apply_edge_clamping_padding;
use crate::stitcher::stitcher::Stitcher;

/// Builds and bakes animated atlas chunks with full vertical animation strips and PBR alignment.
pub fn build_anim_atlas_chunks(
    anim_sprites: Vec<DecodedSprite>,
    config: &AtlasBuilderConfig,
    category_name: &str,
    global_chunk_counter: &mut u16,
    texture_id_counter: &mut u32,
    baked_chunks: &mut Vec<BakedAtlasChunk>,
    address_map: &mut AtlasAddressMap,
) -> Result<(), TextureError> {
    if anim_sprites.is_empty() {
        return Ok(());
    }

    let mut anim_stitcher = Stitcher::new(config.max_width, config.max_height, config.mip_level);
    let mut anim_sprite_map: HashMap<ResourceLocation, DecodedSprite> = HashMap::new();

    for sprite in anim_sprites {
        let strip_w = sprite.frame_width + config.padding * 2;
        let strip_h = sprite.albedo.height + config.padding * 2;
        anim_stitcher.register_sprite(
            sprite.sprite_id.clone(),
            strip_w,
            strip_h,
            sprite.sprite_id.as_string(),
        );
        anim_sprite_map.insert(sprite.sprite_id.clone(), sprite);
    }

    let anim_stitched = anim_stitcher.stitch()?;

    for (idx, chunk) in anim_stitched.chunks.into_iter().enumerate() {
        let chunk_id = *global_chunk_counter;
        *global_chunk_counter += 1;
        let category_chunk_index = idx + 1;
        let mut has_normal = false;
        let mut has_specular = false;
        let mut has_overlay = false;
        let mut has_normal_gl = false;
        let mut has_orm = false;
        let mut has_aux = false;

        for slot in &chunk.slots {
            if let Some(sp) = anim_sprite_map.get(&slot.entry) {
                if sp.normal.is_some() {
                    has_normal = true;
                }
                if sp.specular.is_some() {
                    has_specular = true;
                }
                if sp.overlay.is_some() {
                    has_overlay = true;
                }
                if sp.normal_gl.is_some() {
                    has_normal_gl = true;
                }
                if sp.orm.is_some() {
                    has_orm = true;
                }
                if sp.aux.is_some() {
                    has_aux = true;
                }
            }
        }

        let mut albedo_buf = RgbaBuffer::new(chunk.width, chunk.height);
        let mut normal_buf = if has_normal {
            Some(RgbaBuffer::solid(
                chunk.width,
                chunk.height,
                128,
                128,
                255,
                255,
            ))
        } else {
            None
        };
        let mut specular_buf = if has_specular {
            Some(RgbaBuffer::new(chunk.width, chunk.height))
        } else {
            None
        };
        let mut overlay_buf = if has_overlay {
            Some(RgbaBuffer::new(chunk.width, chunk.height))
        } else {
            None
        };
        let mut normal_gl_buf = if has_normal_gl {
            Some(RgbaBuffer::solid(
                chunk.width,
                chunk.height,
                128,
                128,
                255,
                255,
            ))
        } else {
            None
        };
        let mut orm_buf = if has_orm {
            Some(RgbaBuffer::solid(chunk.width, chunk.height, 255, 255, 0, 0))
        } else {
            None
        };
        let mut aux_buf = if has_aux {
            Some(RgbaBuffer::new(chunk.width, chunk.height))
        } else {
            None
        };

        for slot in chunk.slots {
            if let Some(sp) = anim_sprite_map.get(&slot.entry) {
                let inner_x = slot.x + config.padding;
                let inner_y = slot.y + config.padding;
                let fw = sp.frame_width;
                let strip_h = sp.albedo.height;

                albedo_buf.blit(&sp.albedo, 0, 0, inner_x, inner_y, fw, strip_h);
                if config.padding > 0 {
                    apply_edge_clamping_padding(
                        &mut albedo_buf,
                        inner_x,
                        inner_y,
                        fw,
                        strip_h,
                        config.padding,
                    );
                }

                let slot_has_normal = sp.normal.is_some();
                super::blit_and_pad_optional_layer(
                    &mut normal_buf,
                    &sp.normal,
                    inner_x,
                    inner_y,
                    fw,
                    strip_h,
                    config.padding,
                );

                let slot_has_specular = sp.specular.is_some();
                super::blit_and_pad_optional_layer(
                    &mut specular_buf,
                    &sp.specular,
                    inner_x,
                    inner_y,
                    fw,
                    strip_h,
                    config.padding,
                );

                let slot_has_overlay = sp.overlay.is_some();
                super::blit_and_pad_optional_layer(
                    &mut overlay_buf,
                    &sp.overlay,
                    inner_x,
                    inner_y,
                    fw,
                    strip_h,
                    config.padding,
                );

                let slot_has_normal_gl = sp.normal_gl.is_some();
                super::blit_and_pad_optional_layer(
                    &mut normal_gl_buf,
                    &sp.normal_gl,
                    inner_x,
                    inner_y,
                    fw,
                    strip_h,
                    config.padding,
                );

                let _slot_has_orm = sp.orm.is_some();
                super::blit_and_pad_optional_layer(
                    &mut orm_buf,
                    &sp.orm,
                    inner_x,
                    inner_y,
                    fw,
                    strip_h,
                    config.padding,
                );

                let _slot_has_aux = sp.aux.is_some();
                super::blit_and_pad_optional_layer(
                    &mut aux_buf,
                    &sp.aux,
                    inner_x,
                    inner_y,
                    fw,
                    strip_h,
                    config.padding,
                );

                let u_min = (inner_x as f32) / (chunk.width as f32);
                let u_max = ((inner_x + fw) as f32) / (chunk.width as f32);
                let v_min = 1.0 - ((inner_y + strip_h) as f32) / (chunk.height as f32);
                let v_max = 1.0 - (inner_y as f32) / (chunk.height as f32);

                let v_frame_step = (sp.frame_height as f32) / (chunk.height as f32);
                let frame_0_v_min = v_max - v_frame_step;

                let sprite_loc = AtlasSpriteLocation {
                    chunk_id,
                    category: category_name.to_string(),
                    is_animated: true,
                    sprite_kind: SpriteKind::AnimatedAtlas,
                    texture_id: *texture_id_counter,
                    uv_bounds: [u_min, v_min, u_max, v_max],
                    frame_0_uv_bounds: [u_min, frame_0_v_min, u_max, v_max],
                    local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                    frame_uv_step: [0.0, v_frame_step],
                    pixel_rect: [inner_x, inner_y, fw, sp.frame_height],
                    strip_pixel_rect: [inner_x, inner_y, fw, strip_h],
                    frame_size: [fw, sp.frame_height],
                    frame_count: sp.frame_count,
                    animation: sp.metadata.clone(),
                    has_normal: slot_has_normal || slot_has_normal_gl,
                    has_specular: slot_has_specular,
                    has_overlay: slot_has_overlay,
                    average_color: Some(sp.albedo.average_rgb_hex()),
                };

                address_map
                    .anim_sprites
                    .insert(sp.sprite_id.clone(), sprite_loc.clone());

                if slot_has_overlay {
                    let overlay_id = sp.sprite_id.with_suffix("_overlay");
                    address_map
                        .anim_sprites
                        .insert(overlay_id, sprite_loc.clone());
                    if sp.sprite_id.path.ends_with("side") {
                        let alt_path = sp.sprite_id.path.replace("side", "side_overlay");
                        address_map.anim_sprites.insert(
                            ResourceLocation::new(&sp.sprite_id.namespace, alt_path),
                            sprite_loc,
                        );
                    }
                }

                *texture_id_counter += 1;
            }
        }

        let mut chunk_meta = AtlasChunkMeta {
            chunk_id,
            category: category_name.to_string(),
            is_animated: true,
            category_chunk_index,
            width: chunk.width,
            height: chunk.height,
            has_normal: has_normal || has_normal_gl,
            has_specular,
            has_overlay,
            has_orm,
            has_aux,
            has_tint: category_name == "blocks" || category_name == "scene",
            pbr: if has_normal_gl || has_orm || has_aux {
                Some(crate::atlas::pbr::PbrMeta::labpbr13())
            } else {
                None
            },
            textures: Vec::new(),
        };
        chunk_meta.refresh_textures();
        address_map.chunks.push(chunk_meta);

        baked_chunks.push(BakedAtlasChunk {
            chunk_id,
            category: category_name.to_string(),
            is_animated: true,
            category_chunk_index,
            width: chunk.width,
            height: chunk.height,
            albedo: albedo_buf,
            normal: normal_buf,
            specular: specular_buf,
            overlay: overlay_buf,
            normal_gl: normal_gl_buf,
            orm: orm_buf,
            aux: aux_buf,
        });
    }

    Ok(())
}
