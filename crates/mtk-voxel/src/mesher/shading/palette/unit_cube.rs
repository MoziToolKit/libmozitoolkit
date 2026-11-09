use glam::Vec2;
use mtk_core::Direction;
use mtk_cull::is_non_mesh_placeholder_block;
use mtk_material::MaterialResolver;

use crate::mesher::heuristic::get_unit_cube_texture_candidates;
use crate::mesher::shading::anim::{extract_anim_metrics, lookup_animated_from_cache};
use crate::mesher::shading::types::{PaletteMeshingData, PreResolvedFace};
use crate::mesher::tint::compute_face_tint;
use crate::types::MesherConfig;

pub(crate) fn resolve_unit_cube_palette_data(
    state_str: &str,
    clean_block: &str,
    config: &MesherConfig,
) -> PaletteMeshingData {
    if is_non_mesh_placeholder_block(state_str) {
        return PaletteMeshingData::Empty;
    }

    let (ns, path) = match clean_block.split_once(':') {
        Some((n, p)) => (n, p),
        None => ("minecraft", clean_block),
    };
    let canonical_tex_key = format!("{}:block/{}", ns, path);

    let has_candidate = if let Some(atlas) = &config.atlas_address_map {
        Direction::ALL.iter().any(|&dir| {
            let candidates = get_unit_cube_texture_candidates(clean_block, dir);
            candidates.iter().any(|cand| {
                MaterialResolver::resolve(cand, config.custom_aliases.as_deref(), atlas).is_some()
            })
        })
    } else {
        false
    };

    if !has_candidate
        && config.unknown_block_strategy == crate::types::UnknownBlockStrategy::IgnoreAsAir
    {
        return PaletteMeshingData::Empty;
    }

    let mut faces: [Option<PreResolvedFace>; 6] = Default::default();
    // Classify the block once for all six directions instead of
    // re-running the (allocation-heavy) tint classifier per face.
    let tint_indices = mtk_material::get_unit_cube_tint_indices(path);
    for dir in Direction::ALL {
        let (
            final_tex_key,
            override_uvs,
            mat_slot,
            chunk_id,
            anim_timing,
            anim_frame_size,
            average_color,
        ) = if let Some(atlas) = &config.atlas_address_map {
            let candidates = get_unit_cube_texture_candidates(clean_block, dir);
            let mut resolved = None;
            for cand in &candidates {
                if let Some((res, sp)) =
                    MaterialResolver::resolve(cand, config.custom_aliases.as_deref(), atlas)
                {
                    resolved = Some((res, sp));
                    break;
                }
            }
            if let Some((res_loc, mut atlas_loc)) = resolved {
                if config.enable_animated_textures {
                    if let Some(anim_loc) = lookup_animated_from_cache(
                        &res_loc.as_string(),
                        config.custom_aliases.as_deref(),
                        atlas,
                    ) {
                        atlas_loc = anim_loc;
                    }
                } else if atlas_loc.is_animated {
                    if let Some(static_loc) = atlas.lookup_static(&res_loc) {
                        atlas_loc = static_loc;
                    }
                }
                let (timing, fsize) = extract_anim_metrics(Some(atlas_loc));
                let avg_col = atlas_loc
                    .average_color
                    .as_deref()
                    .map(mtk_material::hex_to_linear_rgba)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                let u_min = atlas_loc.frame_0_uv_bounds[0];
                let v_min = atlas_loc.frame_0_uv_bounds[1];
                let u_max = atlas_loc.frame_0_uv_bounds[2];
                let v_max = atlas_loc.frame_0_uv_bounds[3];
                let remapped = [
                    Vec2::new(u_min, v_max),
                    Vec2::new(u_min, v_min),
                    Vec2::new(u_max, v_min),
                    Vec2::new(u_max, v_max),
                ];
                (
                    res_loc.as_string(),
                    Some(remapped),
                    atlas_loc.chunk_id,
                    atlas_loc.chunk_id as i32,
                    timing,
                    fsize,
                    avg_col,
                )
            } else if let Some(fb) = atlas.lookup_fallback() {
                let (timing, fsize) = extract_anim_metrics(Some(fb));
                let avg_col = fb
                    .average_color
                    .as_deref()
                    .map(mtk_material::hex_to_linear_rgba)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                let u_min = fb.frame_0_uv_bounds[0];
                let v_min = fb.frame_0_uv_bounds[1];
                let u_max = fb.frame_0_uv_bounds[2];
                let v_max = fb.frame_0_uv_bounds[3];
                let remapped = [
                    Vec2::new(u_min, v_max),
                    Vec2::new(u_min, v_min),
                    Vec2::new(u_max, v_min),
                    Vec2::new(u_max, v_max),
                ];
                (
                    canonical_tex_key.clone(),
                    Some(remapped),
                    fb.chunk_id,
                    fb.chunk_id as i32,
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
                    Vec2::new(0.0, v_span),
                    Vec2::new(0.0, 0.0),
                    Vec2::new(u_span, 0.0),
                    Vec2::new(u_span, v_span),
                ];
                let avg_col = [1.0, 0.0, 1.0, 1.0];
                (
                    canonical_tex_key.clone(),
                    Some(remapped),
                    default_chunk,
                    default_chunk as i32,
                    [1.0, 1.0, 0.0],
                    [16.0, 16.0, 0.0],
                    avg_col,
                )
            }
        } else {
            (
                canonical_tex_key.clone(),
                None,
                0,
                0,
                [1.0, 1.0, 0.0],
                [16.0, 16.0, 0.0],
                [1.0, 1.0, 1.0, 1.0],
            )
        };

        let tint_idx = tint_indices[dir.to_index()];
        let (tint_data, tint_color) = compute_face_tint(
            &final_tex_key,
            state_str,
            tint_idx,
            config.biome_resolver.as_deref(),
            config.colormaps.as_deref(),
        );

        faces[dir.to_index()] = Some(PreResolvedFace {
            source_texture_key: final_tex_key,
            override_uvs,
            mat_slot,
            chunk_id,
            tint_data,
            tint_color,
            anim_timing,
            anim_frame_size,
            average_color,
        });
    }

    PaletteMeshingData::UnitCube {
        faces: [
            faces[0].take().unwrap(),
            faces[1].take().unwrap(),
            faces[2].take().unwrap(),
            faces[3].take().unwrap(),
            faces[4].take().unwrap(),
            faces[5].take().unwrap(),
        ],
    }
}
