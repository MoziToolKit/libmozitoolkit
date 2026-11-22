use glam::Vec2;
use mtk_material::MaterialResolver;
use mtk_model::{BakedFace, BakedObjFace};

use crate::mesher::shading::anim::{extract_anim_metrics, lookup_animated_from_cache};
use crate::mesher::shading::types::{PreResolvedFace, PreResolvedModelFace, PreResolvedObjFace};
use crate::mesher::tint::compute_face_tint;
use crate::types::MesherConfig;

struct ResolvedAtlasBinding {
    final_tex_key: String,
    remapped_uvs: Option<Vec<Vec2>>,
    mat_slot: u16,
    chunk_id: i32,
    anim_timing: [f32; 3],
    anim_frame_size: [f32; 3],
    average_color: [f32; 4],
}

#[inline]
fn remap_uv_slice(uvs: &[Vec2], bounds: [f32; 4], clamp_uv: bool) -> Vec<Vec2> {
    let u_min = bounds[0];
    let v_min = bounds[1];
    let u_span = bounds[2] - u_min;
    let v_span = bounds[3] - v_min;
    uvs.iter()
        .map(|uv| {
            let (u, v) = if clamp_uv {
                (uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0))
            } else {
                (uv.x, uv.y)
            };
            Vec2::new(u_min + u * u_span, v_min + (1.0 - v) * v_span)
        })
        .collect()
}

fn resolve_atlas_binding(
    texture: &str,
    raw_uvs: &[Vec2],
    pre_atlas_uvs: Option<&[Vec2]>,
    pre_atlas_chunk_id: Option<u16>,
    clamp_uv: bool,
    config: &MesherConfig,
) -> ResolvedAtlasBinding {
    let Some(atlas) = &config.atlas_address_map else {
        return ResolvedAtlasBinding {
            final_tex_key: texture.to_string(),
            remapped_uvs: None,
            mat_slot: pre_atlas_chunk_id.unwrap_or(0),
            chunk_id: pre_atlas_chunk_id.unwrap_or(0) as i32,
            anim_timing: [1.0, 1.0, 0.0],
            anim_frame_size: [16.0, 16.0, 0.0],
            average_color: [1.0, 1.0, 1.0, 1.0],
        };
    };

    let anim_target = if config.enable_animated_textures {
        lookup_animated_from_cache(texture, config.custom_aliases.as_deref(), atlas)
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
        return ResolvedAtlasBinding {
            final_tex_key: texture.to_string(),
            remapped_uvs: Some(remap_uv_slice(
                raw_uvs,
                anim_loc.frame_0_uv_bounds,
                clamp_uv,
            )),
            mat_slot: anim_loc.chunk_id,
            chunk_id: anim_loc.chunk_id as i32,
            anim_timing: timing,
            anim_frame_size: fsize,
            average_color: avg_col,
        };
    }

    if let Some(atlas_uvs) = pre_atlas_uvs {
        let base_loc = (!texture.is_empty())
            .then(|| mtk_resource::ResourceLocation::parse(texture).ok())
            .flatten();
        let sp = base_loc
            .as_ref()
            .and_then(|loc| {
                atlas.lookup(loc).or_else(|| {
                    MaterialResolver::resolve(
                        &loc.as_string(),
                        config.custom_aliases.as_deref(),
                        atlas,
                    )
                    .map(|(_, s)| s)
                })
            })
            .or_else(|| atlas.lookup_str(texture))
            .or_else(|| {
                MaterialResolver::resolve(texture, config.custom_aliases.as_deref(), atlas)
                    .map(|(_, s)| s)
            })
            .or_else(|| atlas.lookup_fallback());
        let avg_col = sp
            .and_then(|s| s.average_color.as_deref())
            .map(mtk_material::hex_to_linear_rgba)
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let cid = pre_atlas_chunk_id.unwrap_or(0);
        return ResolvedAtlasBinding {
            final_tex_key: texture.to_string(),
            remapped_uvs: Some(atlas_uvs.to_vec()),
            mat_slot: cid,
            chunk_id: cid as i32,
            anim_timing: [1.0, 1.0, 0.0],
            anim_frame_size: [16.0, 16.0, 0.0],
            average_color: avg_col,
        };
    }

    let base_loc = (!texture.is_empty())
        .then(|| mtk_resource::ResourceLocation::parse(texture).ok())
        .flatten();
    let resolved = base_loc
        .as_ref()
        .and_then(|loc| {
            MaterialResolver::resolve(&loc.as_string(), config.custom_aliases.as_deref(), atlas)
                .or_else(|| atlas.lookup(loc).map(|sp| (loc.clone(), sp)))
        })
        .or_else(|| MaterialResolver::resolve(texture, config.custom_aliases.as_deref(), atlas));

    if let Some((res_loc, mut atlas_loc)) = resolved {
        if config.enable_animated_textures {
            if let Some(anim_loc) = lookup_animated_from_cache(
                &res_loc.as_string(),
                config.custom_aliases.as_deref(),
                atlas,
            )
            .or_else(|| {
                lookup_animated_from_cache(texture, config.custom_aliases.as_deref(), atlas)
            }) {
                atlas_loc = anim_loc;
            }
        } else if atlas_loc.is_animated {
            if let Some(static_loc) = atlas
                .lookup_static(&res_loc)
                .or_else(|| atlas.lookup_static_str(texture))
            {
                atlas_loc = static_loc;
            }
        }
        let (timing, fsize) = extract_anim_metrics(Some(atlas_loc));
        let avg_col = atlas_loc
            .average_color
            .as_deref()
            .map(mtk_material::hex_to_linear_rgba)
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        return ResolvedAtlasBinding {
            final_tex_key: res_loc.as_string(),
            remapped_uvs: Some(remap_uv_slice(
                raw_uvs,
                atlas_loc.frame_0_uv_bounds,
                clamp_uv,
            )),
            mat_slot: atlas_loc.chunk_id,
            chunk_id: atlas_loc.chunk_id as i32,
            anim_timing: timing,
            anim_frame_size: fsize,
            average_color: avg_col,
        };
    }

    let fallback_loc = if texture.starts_with("minecraft:skin/")
        || texture.starts_with("skin/")
        || texture.starts_with("minecraft:player/")
        || texture.starts_with("player/")
    {
        atlas
            .lookup_str("minecraft:entity/player/wide/steve")
            .or_else(|| atlas.lookup_fallback())
    } else {
        atlas.lookup_fallback()
    };

    if let Some(fallback_loc) = fallback_loc {
        let (timing, fsize) = extract_anim_metrics(Some(fallback_loc));
        let avg_col = fallback_loc
            .average_color
            .as_deref()
            .map(mtk_material::hex_to_linear_rgba)
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        return ResolvedAtlasBinding {
            final_tex_key: texture.to_string(),
            remapped_uvs: Some(remap_uv_slice(
                raw_uvs,
                fallback_loc.frame_0_uv_bounds,
                clamp_uv,
            )),
            mat_slot: fallback_loc.chunk_id,
            chunk_id: fallback_loc.chunk_id as i32,
            anim_timing: timing,
            anim_frame_size: fsize,
            average_color: avg_col,
        };
    }

    let default_chunk = atlas.default_chunk_id();
    let (chunk_w, chunk_h) = atlas
        .get_chunk_meta(default_chunk)
        .map(|c| (c.width as f32, c.height as f32))
        .unwrap_or((4096.0, 4096.0));
    let bounds = [0.0, 0.0, 16.0 / chunk_w.max(16.0), 16.0 / chunk_h.max(16.0)];
    let avg_col = atlas
        .lookup_fallback()
        .and_then(|s| s.average_color.as_deref())
        .map(mtk_material::hex_to_linear_rgba)
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    ResolvedAtlasBinding {
        final_tex_key: texture.to_string(),
        remapped_uvs: Some(remap_uv_slice(raw_uvs, bounds, clamp_uv)),
        mat_slot: default_chunk,
        chunk_id: default_chunk as i32,
        anim_timing: [1.0, 1.0, 0.0],
        anim_frame_size: [16.0, 16.0, 0.0],
        average_color: avg_col,
    }
}

pub(crate) fn resolve_model_face(
    face: &BakedFace,
    state_str: &str,
    config: &MesherConfig,
) -> PreResolvedModelFace {
    let binding = resolve_atlas_binding(
        &face.texture,
        &face.uvs,
        face.atlas_uvs.as_ref().map(|a| a.as_slice()),
        face.atlas_chunk_id,
        false,
        config,
    );
    let override_uvs = binding
        .remapped_uvs
        .and_then(|v| <[Vec2; 4]>::try_from(v).ok());

    let (tint_data, tint_color) = compute_face_tint(
        &binding.final_tex_key,
        state_str,
        face.tint_index,
        config.biome_resolver.as_deref(),
        config.colormaps.as_deref(),
    );

    PreResolvedModelFace {
        face: face.clone(),
        pre: PreResolvedFace {
            source_texture_key: binding.final_tex_key,
            override_uvs,
            mat_slot: binding.mat_slot,
            chunk_id: binding.chunk_id,
            tint_data,
            tint_color,
            anim_timing: binding.anim_timing,
            anim_frame_size: binding.anim_frame_size,
            average_color: binding.average_color,
        },
    }
}

pub(crate) fn resolve_obj_face(
    face: &BakedObjFace,
    state_str: &str,
    config: &MesherConfig,
) -> PreResolvedObjFace {
    let binding = resolve_atlas_binding(
        &face.texture,
        &face.uvs,
        face.atlas_uvs.as_deref(),
        face.atlas_chunk_id,
        true,
        config,
    );
    let (tint_data, tint_color) = compute_face_tint(
        &binding.final_tex_key,
        state_str,
        face.tint_index,
        config.biome_resolver.as_deref(),
        config.colormaps.as_deref(),
    );

    PreResolvedObjFace {
        face: face.clone(),
        override_uvs: binding.remapped_uvs,
        pre: PreResolvedFace {
            source_texture_key: binding.final_tex_key,
            override_uvs: None,
            mat_slot: binding.mat_slot,
            chunk_id: binding.chunk_id,
            tint_data,
            tint_color,
            anim_timing: binding.anim_timing,
            anim_frame_size: binding.anim_frame_size,
            average_color: binding.average_color,
        },
    }
}
