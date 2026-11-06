use std::collections::HashMap;

use mtk_resource::ResourceLocation;

use super::types::{AtlasBuilderConfig, BakedAtlasChunk};
use crate::atlas::address_map::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, SpriteKind};
use crate::error::TextureError;
use crate::image::apply_edge_clamping_padding;
use crate::stitcher::Stitcher;
use crate::{DecodedSprite, RgbaBuffer};

/// Builds and bakes static atlas chunks with PBR multi-channel buffers.
pub(crate) fn build_static_atlas_chunks(
    static_sprites: Vec<DecodedSprite>,
    config: &AtlasBuilderConfig,
    category_name: &str,
    global_chunk_counter: &mut u16,
    texture_id_counter: &mut u32,
    baked_chunks: &mut Vec<BakedAtlasChunk>,
    address_map: &mut AtlasAddressMap,
) -> Result<(), TextureError> {
    let fallback_id = mtk_resource::ResourceLocation::new("mozi", "fallback");
    let is_main_blocks = (category_name == "blocks" || category_name == "scene")
        && !address_map.sprites.contains_key(&fallback_id);
    if static_sprites.is_empty() && !is_main_blocks {
        return Ok(());
    }

    let mut stitcher = Stitcher::new(config.max_width, config.max_height, config.mip_level);
    let mut sprite_map: HashMap<ResourceLocation, DecodedSprite> = HashMap::new();

    // Ensure canonical fallback checkerboard sprite is pinned at chunk 0, slot 0 (x=0, y=0)
    if is_main_blocks {
        let fallback_sp = crate::atlas::create_fallback_sprite();
        let fw = fallback_sp.frame_width + config.padding * 2;
        let fh = fallback_sp.frame_height + config.padding * 2;
        stitcher.register_pinned_sprite(
            fallback_sp.sprite_id.clone(),
            fw,
            fh,
            fallback_sp.sprite_id.as_string(),
        );
        sprite_map.insert(fallback_sp.sprite_id.clone(), fallback_sp);
    }

    for sprite in static_sprites {
        let fw = sprite.frame_width + config.padding * 2;
        let fh = sprite.frame_height + config.padding * 2;
        stitcher.register_sprite(
            sprite.sprite_id.clone(),
            fw,
            fh,
            sprite.sprite_id.as_string(),
        );
        sprite_map.insert(sprite.sprite_id.clone(), sprite);
    }

    let stitched = stitcher.stitch()?;

    for (idx, chunk) in stitched.chunks.into_iter().enumerate() {
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
            if let Some(sp) = sprite_map.get(&slot.entry) {
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
        let mut pbr_layers = super::PbrLayerBuffers::new(
            chunk.width,
            chunk.height,
            has_normal,
            has_specular,
            has_overlay,
            has_normal_gl,
            has_orm,
            has_aux,
        );

        for slot in chunk.slots {
            if let Some(sp) = sprite_map.get(&slot.entry) {
                let inner_x = slot.x + config.padding;
                let inner_y = slot.y + config.padding;
                let fw = sp.frame_width;
                let fh = sp.frame_height;

                albedo_buf.blit(&sp.albedo, 0, 0, inner_x, inner_y, fw, fh);
                if config.padding > 0 {
                    apply_edge_clamping_padding(
                        &mut albedo_buf,
                        inner_x,
                        inner_y,
                        fw,
                        fh,
                        config.padding,
                    );
                }

                let slot_has_normal = sp.normal.is_some();
                let slot_has_specular = sp.specular.is_some();
                let slot_has_overlay = sp.overlay.is_some();
                let slot_has_normal_gl = sp.normal_gl.is_some();

                pbr_layers.blit_sprite_layers(sp, inner_x, inner_y, fw, fh, config.padding);

                let u_min = (inner_x as f32) / (chunk.width as f32);
                let u_max = ((inner_x + fw) as f32) / (chunk.width as f32);
                let v_min = 1.0 - ((inner_y + fh) as f32) / (chunk.height as f32);
                let v_max = 1.0 - (inner_y as f32) / (chunk.height as f32);

                let sprite_loc = AtlasSpriteLocation {
                    chunk_id,
                    category: category_name.to_string(),
                    is_animated: false,
                    sprite_kind: SpriteKind::StaticAtlas,
                    texture_id: *texture_id_counter,
                    uv_bounds: [u_min, v_min, u_max, v_max],
                    frame_0_uv_bounds: [u_min, v_min, u_max, v_max],
                    local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
                    frame_uv_step: [0.0, 0.0],
                    pixel_rect: [inner_x, inner_y, fw, fh],
                    strip_pixel_rect: [inner_x, inner_y, fw, fh],
                    frame_size: [fw, fh],
                    frame_count: 1,
                    animation: None,
                    has_normal: slot_has_normal || slot_has_normal_gl,
                    has_specular: slot_has_specular,
                    has_overlay: slot_has_overlay,
                    average_color: Some(sp.albedo.average_rgb_hex()),
                    slot_index: None,
                };

                address_map
                    .sprites
                    .insert(sp.sprite_id.clone(), sprite_loc.clone());

                if sp.sprite_id.namespace == "mozi" && sp.sprite_id.path == "fallback" {
                    address_map
                        .sprites
                        .entry(ResourceLocation::new("minecraft", "missingno"))
                        .or_insert_with(|| sprite_loc.clone());
                    address_map
                        .sprites
                        .entry(ResourceLocation::new("minecraft", "missing"))
                        .or_insert_with(|| sprite_loc.clone());
                }

                if slot_has_overlay {
                    let overlay_id = sp.sprite_id.with_suffix("_overlay");
                    address_map.sprites.insert(overlay_id, sprite_loc.clone());
                    if sp.sprite_id.path.ends_with("side") {
                        let alt_path = sp.sprite_id.path.replace("side", "side_overlay");
                        address_map.sprites.insert(
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
            is_animated: false,
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
            is_animated: false,
            category_chunk_index,
            width: chunk.width,
            height: chunk.height,
            albedo: albedo_buf,
            normal: pbr_layers.normal,
            specular: pbr_layers.specular,
            overlay: pbr_layers.overlay,
            normal_gl: pbr_layers.normal_gl,
            orm: pbr_layers.orm,
            aux: pbr_layers.aux,
        });
    }

    Ok(())
}
