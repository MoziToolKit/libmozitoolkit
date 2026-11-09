use glam::Vec2;
use mtk_material::MaterialResolver;
use mtk_model::BakedFace;

use crate::mesher::shading::anim::{extract_anim_metrics, lookup_animated_from_cache};
use crate::mesher::shading::types::{PreResolvedFace, PreResolvedModelFace};
use crate::mesher::tint::compute_face_tint;
use crate::types::MesherConfig;

pub(crate) fn resolve_model_face(
    face: &BakedFace,
    state_str: &str,
    config: &MesherConfig,
) -> PreResolvedModelFace {
    let (
        final_tex_key,
        override_uvs,
        mat_slot,
        chunk_id,
        anim_timing,
        anim_frame_size,
        average_color,
    ) = if let Some(atlas) = &config.atlas_address_map {
        // Cache-driven animated texture gate:
        // When enabled, probe the prebaked animated sprite cache.
        let anim_target = if config.enable_animated_textures {
            lookup_animated_from_cache(&face.texture, config.custom_aliases.as_deref(), atlas)
        } else {
            None
        };

        if let Some(anim_loc) = anim_target {
            let (timing, fsize) = extract_anim_metrics(Some(anim_loc));
            let avg_col = anim_loc
                .average_color
                .as_deref()
                .map(mtk_material::hex_to_linear_rgba)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let u_min = anim_loc.frame_0_uv_bounds[0];
            let v_min = anim_loc.frame_0_uv_bounds[1];
            let u_span = anim_loc.frame_0_uv_bounds[2] - u_min;
            let v_span = anim_loc.frame_0_uv_bounds[3] - v_min;
            let remapped = [
                Vec2::new(
                    u_min + face.uvs[0].x * u_span,
                    v_min + (1.0 - face.uvs[0].y) * v_span,
                ),
                Vec2::new(
                    u_min + face.uvs[1].x * u_span,
                    v_min + (1.0 - face.uvs[1].y) * v_span,
                ),
                Vec2::new(
                    u_min + face.uvs[2].x * u_span,
                    v_min + (1.0 - face.uvs[2].y) * v_span,
                ),
                Vec2::new(
                    u_min + face.uvs[3].x * u_span,
                    v_min + (1.0 - face.uvs[3].y) * v_span,
                ),
            ];
            (
                face.texture.clone(),
                Some(remapped),
                anim_loc.chunk_id,
                anim_loc.chunk_id as i32,
                timing,
                fsize,
                avg_col,
            )
        } else if let Some(ref atlas_uvs) = face.atlas_uvs {
            let base_loc = if !face.texture.is_empty() {
                mtk_resource::ResourceLocation::parse(&face.texture).ok()
            } else {
                None
            };
            let sp = if let Some(ref loc) = base_loc {
                atlas.lookup(loc).or_else(|| {
                    MaterialResolver::resolve(
                        &loc.as_string(),
                        config.custom_aliases.as_deref(),
                        atlas,
                    )
                    .map(|(_, s)| s)
                })
            } else {
                None
            }
            .or_else(|| atlas.lookup_str(&face.texture))
            .or_else(|| {
                MaterialResolver::resolve(&face.texture, config.custom_aliases.as_deref(), atlas)
                    .map(|(_, s)| s)
            })
            .or_else(|| atlas.lookup_fallback());

            let avg_col = sp
                .and_then(|s| s.average_color.as_deref())
                .map(mtk_material::hex_to_linear_rgba)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);

            (
                face.texture.clone(),
                Some(*atlas_uvs),
                face.atlas_chunk_id.unwrap_or(0),
                face.atlas_chunk_id.unwrap_or(0) as i32,
                [1.0, 1.0, 0.0],
                [16.0, 16.0, 0.0],
                avg_col,
            )
        } else {
            let base_loc = if !face.texture.is_empty() {
                mtk_resource::ResourceLocation::parse(&face.texture).ok()
            } else {
                None
            };
            let resolved = if let Some(ref loc) = base_loc {
                MaterialResolver::resolve(&loc.as_string(), config.custom_aliases.as_deref(), atlas)
                    .or_else(|| atlas.lookup(loc).map(|sp| ((*loc).clone(), sp)))
            } else {
                None
            }
            .or_else(|| {
                MaterialResolver::resolve(&face.texture, config.custom_aliases.as_deref(), atlas)
            });

            if let Some((res_loc, mut atlas_loc)) = resolved {
                if config.enable_animated_textures {
                    if let Some(anim_loc) = lookup_animated_from_cache(
                        &res_loc.as_string(),
                        config.custom_aliases.as_deref(),
                        atlas,
                    )
                    .or_else(|| {
                        lookup_animated_from_cache(
                            &face.texture,
                            config.custom_aliases.as_deref(),
                            atlas,
                        )
                    }) {
                        atlas_loc = anim_loc;
                    }
                } else if atlas_loc.is_animated {
                    if let Some(static_loc) = atlas
                        .lookup_static(&res_loc)
                        .or_else(|| atlas.lookup_static_str(&face.texture))
                    {
                        atlas_loc = static_loc;
                    }
                }
                let (timing, fsize) = extract_anim_metrics(Some(atlas_loc));
                let u_min = atlas_loc.frame_0_uv_bounds[0];
                let v_min = atlas_loc.frame_0_uv_bounds[1];
                let u_span = atlas_loc.frame_0_uv_bounds[2] - u_min;
                let v_span = atlas_loc.frame_0_uv_bounds[3] - v_min;
                let remapped = [
                    Vec2::new(
                        u_min + face.uvs[0].x * u_span,
                        v_min + (1.0 - face.uvs[0].y) * v_span,
                    ),
                    Vec2::new(
                        u_min + face.uvs[1].x * u_span,
                        v_min + (1.0 - face.uvs[1].y) * v_span,
                    ),
                    Vec2::new(
                        u_min + face.uvs[2].x * u_span,
                        v_min + (1.0 - face.uvs[2].y) * v_span,
                    ),
                    Vec2::new(
                        u_min + face.uvs[3].x * u_span,
                        v_min + (1.0 - face.uvs[3].y) * v_span,
                    ),
                ];
                let avg_col = atlas_loc
                    .average_color
                    .as_deref()
                    .map(mtk_material::hex_to_linear_rgba)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                (
                    res_loc.as_string(),
                    Some(remapped),
                    atlas_loc.chunk_id,
                    atlas_loc.chunk_id as i32,
                    timing,
                    fsize,
                    avg_col,
                )
            } else if let Some(fallback_loc) = if face.texture.starts_with("minecraft:skin/")
                || face.texture.starts_with("skin/")
                || face.texture.starts_with("minecraft:player/")
                || face.texture.starts_with("player/")
            {
                atlas
                    .lookup_str("minecraft:entity/player/wide/steve")
                    .or_else(|| atlas.lookup_fallback())
            } else {
                atlas.lookup_fallback()
            } {
                let (timing, fsize) = extract_anim_metrics(Some(fallback_loc));
                let avg_col = fallback_loc
                    .average_color
                    .as_deref()
                    .map(mtk_material::hex_to_linear_rgba)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                let u_min = fallback_loc.frame_0_uv_bounds[0];
                let v_min = fallback_loc.frame_0_uv_bounds[1];
                let u_max = fallback_loc.frame_0_uv_bounds[2];
                let v_max = fallback_loc.frame_0_uv_bounds[3];
                let u_span = u_max - u_min;
                let v_span = v_max - v_min;
                let remapped = [
                    Vec2::new(
                        u_min + face.uvs[0].x * u_span,
                        v_min + (1.0 - face.uvs[0].y) * v_span,
                    ),
                    Vec2::new(
                        u_min + face.uvs[1].x * u_span,
                        v_min + (1.0 - face.uvs[1].y) * v_span,
                    ),
                    Vec2::new(
                        u_min + face.uvs[2].x * u_span,
                        v_min + (1.0 - face.uvs[2].y) * v_span,
                    ),
                    Vec2::new(
                        u_min + face.uvs[3].x * u_span,
                        v_min + (1.0 - face.uvs[3].y) * v_span,
                    ),
                ];
                (
                    face.texture.clone(),
                    Some(remapped),
                    fallback_loc.chunk_id,
                    fallback_loc.chunk_id as i32,
                    timing,
                    fsize,
                    avg_col,
                )
            } else {
                let default_chunk = atlas.default_chunk_id();
                let (chunk_w, chunk_h) = atlas
                    .get_chunk_meta(default_chunk)
                    .map(|c| (c.width as f32, c.height as f32))
                    .unwrap_or((4096.0, 4096.0));
                let u_span = 16.0 / chunk_w.max(16.0);
                let v_span = 16.0 / chunk_h.max(16.0);
                let remapped = [
                    Vec2::new(face.uvs[0].x * u_span, (1.0 - face.uvs[0].y) * v_span),
                    Vec2::new(face.uvs[1].x * u_span, (1.0 - face.uvs[1].y) * v_span),
                    Vec2::new(face.uvs[2].x * u_span, (1.0 - face.uvs[2].y) * v_span),
                    Vec2::new(face.uvs[3].x * u_span, (1.0 - face.uvs[3].y) * v_span),
                ];
                let avg_col = atlas
                    .lookup_fallback()
                    .and_then(|s| s.average_color.as_deref())
                    .map(mtk_material::hex_to_linear_rgba)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                (
                    face.texture.clone(),
                    Some(remapped),
                    default_chunk,
                    default_chunk as i32,
                    [1.0, 1.0, 0.0],
                    [16.0, 16.0, 0.0],
                    avg_col,
                )
            }
        }
    } else {
        (
            face.texture.clone(),
            None,
            face.atlas_chunk_id.unwrap_or(0),
            face.atlas_chunk_id.unwrap_or(0) as i32,
            [1.0, 1.0, 0.0],
            [16.0, 16.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
        )
    };

    let (tint_data, tint_color) = compute_face_tint(
        &final_tex_key,
        state_str,
        face.tint_index,
        config.biome_resolver.as_deref(),
        config.colormaps.as_deref(),
    );

    PreResolvedModelFace {
        face: face.clone(),
        pre: PreResolvedFace {
            source_texture_key: final_tex_key,
            override_uvs,
            mat_slot,
            chunk_id,
            tint_data,
            tint_color,
            anim_timing,
            anim_frame_size,
            average_color,
        },
    }
}
