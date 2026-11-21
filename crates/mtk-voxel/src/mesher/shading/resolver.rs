use glam::{IVec3, Vec2};
use mtk_core::Direction;
use std::borrow::Cow;

use crate::biome::SmoothedBiomeColumn;
use crate::mesher::tint::compute_face_tint;
use crate::types::MesherConfig;

use super::anim::{extract_anim_metrics, lookup_animated_from_cache};
use super::types::{PreResolvedFace, PreResolvedModelFace, ResolvedFaceShading};

/// Dynamically resolves CTM overrides for a BakedModel face, falling back to pre-resolved shading.
pub fn resolve_model_face_shading<'a, 'b, F>(
    mf: &'a PreResolvedModelFace,
    state_str: &str,
    block_pos: IVec3,
    config: &MesherConfig,
    ctm_active: bool,
    get_neighbor_state: F,
) -> ResolvedFaceShading<'a>
where
    F: Fn(IVec3) -> Option<&'b str>,
{
    let face = &mf.face;
    let pre_face = &mf.pre;
    let face_dir = face.direction;

    if let Some(solver) = config.ctm_solver.as_ref().filter(|_| ctm_active) {
        let base_loc = if !face.texture.is_empty() {
            mtk_resource::ResourceLocation::parse(&face.texture).ok()
        } else {
            None
        };
        let tangents =
            mtk_core::derive_quad_face_tangents(&face.vertices, &face.uvs, face_dir, true);
        if let Some(resolved_loc) = solver.resolve_face_with_tangents(
            state_str,
            face_dir,
            tangents,
            block_pos,
            base_loc.as_ref(),
            None,
            get_neighbor_state,
        ) {
            if let Some(atlas) = &config.atlas_address_map {
                if let Some(mut atlas_loc) = atlas.lookup(&resolved_loc) {
                    if config.enable_animated_textures {
                        if let Some(anim_loc) = lookup_animated_from_cache(
                            &resolved_loc.as_string(),
                            config.custom_aliases.as_deref(),
                            atlas,
                        ) {
                            atlas_loc = anim_loc;
                        }
                    } else if atlas_loc.is_animated {
                        if let Some(static_loc) = atlas.lookup_static(&resolved_loc) {
                            atlas_loc = static_loc;
                        }
                    }
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
                    let ctm_key = resolved_loc.as_string();
                    let (td, tc) = compute_face_tint(
                        &ctm_key,
                        state_str,
                        face.tint_index,
                        config.biome_resolver.as_deref(),
                        config.colormaps.as_deref(),
                    );
                    let (timing, fsize) = extract_anim_metrics(Some(atlas_loc));
                    let avg_col = atlas_loc
                        .average_color
                        .as_deref()
                        .map(mtk_material::hex_to_linear_rgba)
                        .unwrap_or(pre_face.average_color);
                    return ResolvedFaceShading {
                        texture_key: Cow::Owned(ctm_key),
                        override_uvs: Some(remapped),
                        mat_slot: atlas_loc.chunk_id,
                        chunk_id: atlas_loc.chunk_id as i32,
                        tint_data: td,
                        tint_color: tc,
                        anim_timing: timing,
                        anim_frame_size: fsize,
                        average_color: avg_col,
                    };
                }
            } else {
                let ctm_key = resolved_loc.as_string();
                let (td, tc) = compute_face_tint(
                    &ctm_key,
                    state_str,
                    face.tint_index,
                    config.biome_resolver.as_deref(),
                    config.colormaps.as_deref(),
                );
                return ResolvedFaceShading {
                    texture_key: Cow::Owned(ctm_key),
                    override_uvs: None,
                    mat_slot: pre_face.mat_slot,
                    chunk_id: 0,
                    tint_data: td,
                    tint_color: tc,
                    anim_timing: pre_face.anim_timing,
                    anim_frame_size: pre_face.anim_frame_size,
                    average_color: pre_face.average_color,
                };
            }
        }
    }

    ResolvedFaceShading {
        texture_key: Cow::Borrowed(pre_face.source_texture_key.as_str()),
        override_uvs: pre_face.override_uvs,
        mat_slot: pre_face.mat_slot,
        chunk_id: pre_face.chunk_id,
        tint_data: pre_face.tint_data,
        tint_color: pre_face.tint_color,
        anim_timing: pre_face.anim_timing,
        anim_frame_size: pre_face.anim_frame_size,
        average_color: pre_face.average_color,
    }
}

/// Dynamically resolves CTM overrides for a unit cube face, falling back to pre-resolved shading.
pub fn resolve_unit_cube_face_shading<'a, 'b, F>(
    pre_face: &'a PreResolvedFace,
    dir: Direction,
    state_str: &str,
    block_pos: IVec3,
    config: &MesherConfig,
    ctm_active: bool,
    get_neighbor_state: F,
) -> ResolvedFaceShading<'a>
where
    F: Fn(IVec3) -> Option<&'b str>,
{
    if let Some(solver) = config.ctm_solver.as_ref().filter(|_| ctm_active) {
        let block_name = mtk_resource::extract_block_name(state_str);
        let clean_block = block_name.strip_prefix("minecraft:").unwrap_or(block_name);
        let mut path = String::with_capacity(6 + clean_block.len());
        path.push_str("block/");
        path.push_str(clean_block);
        let base_loc = mtk_resource::ResourceLocation::new("minecraft", path);
        if let Some(resolved_loc) = solver.resolve_face(
            state_str,
            dir,
            block_pos,
            Some(&base_loc),
            None,
            get_neighbor_state,
        ) {
            if let Some(atlas) = &config.atlas_address_map {
                if let Some(mut atlas_loc) = atlas.lookup(&resolved_loc) {
                    if config.enable_animated_textures {
                        if let Some(anim_loc) = lookup_animated_from_cache(
                            &resolved_loc.as_string(),
                            config.custom_aliases.as_deref(),
                            atlas,
                        ) {
                            atlas_loc = anim_loc;
                        }
                    } else if atlas_loc.is_animated {
                        if let Some(static_loc) = atlas.lookup_static(&resolved_loc) {
                            atlas_loc = static_loc;
                        }
                    }
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
                    let ctm_key = resolved_loc.as_string();
                    let (td, tc) = compute_face_tint(
                        &ctm_key,
                        state_str,
                        -1,
                        config.biome_resolver.as_deref(),
                        config.colormaps.as_deref(),
                    );
                    let (timing, fsize) = extract_anim_metrics(Some(atlas_loc));
                    let avg_col = atlas_loc
                        .average_color
                        .as_deref()
                        .map(mtk_material::hex_to_linear_rgba)
                        .unwrap_or(pre_face.average_color);
                    return ResolvedFaceShading {
                        texture_key: Cow::Owned(ctm_key),
                        override_uvs: Some(remapped),
                        mat_slot: atlas_loc.chunk_id,
                        chunk_id: atlas_loc.chunk_id as i32,
                        tint_data: td,
                        tint_color: tc,
                        anim_timing: timing,
                        anim_frame_size: fsize,
                        average_color: avg_col,
                    };
                }
            } else {
                let ctm_key = resolved_loc.as_string();
                let (td, tc) = compute_face_tint(
                    &ctm_key,
                    state_str,
                    -1,
                    config.biome_resolver.as_deref(),
                    config.colormaps.as_deref(),
                );
                return ResolvedFaceShading {
                    texture_key: Cow::Owned(ctm_key),
                    override_uvs: None,
                    mat_slot: pre_face.mat_slot,
                    chunk_id: 0,
                    tint_data: td,
                    tint_color: tc,
                    anim_timing: pre_face.anim_timing,
                    anim_frame_size: pre_face.anim_frame_size,
                    average_color: pre_face.average_color,
                };
            }
        }
    }

    ResolvedFaceShading {
        texture_key: Cow::Borrowed(pre_face.source_texture_key.as_str()),
        override_uvs: pre_face.override_uvs,
        mat_slot: pre_face.mat_slot,
        chunk_id: pre_face.chunk_id,
        tint_data: pre_face.tint_data,
        tint_color: pre_face.tint_color,
        anim_timing: pre_face.anim_timing,
        anim_frame_size: pre_face.anim_frame_size,
        average_color: pre_face.average_color,
    }
}

/// Samples tint color from a single `SmoothedBiomeColumn` based on the tint type.
#[inline]
pub fn sample_column_tint_color(
    col: &SmoothedBiomeColumn,
    tint_type: u8,
    colormaps: Option<&mtk_material::VanillaColormaps>,
    default_tint_color: [f32; 4],
) -> ([f32; 4], bool) {
    match tint_type {
        mtk_material::TINT_TYPE_GRASS => {
            // Swamp replaces the colour entirely (position-dependent noise), `dark_forest` samples
            // and then transforms. Both are exact colours the host cannot reproduce - it has no
            // modifier logic - so they are reported as overrides and shipped as TINT_TYPE_HARDCODED.
            let modifier = col.grass_color_modifier;
            let is_modifier = modifier != mtk_material::GrassColorModifier::None;
            if modifier.is_swamp() || col.has_custom_grass {
                (col.grass_color, true)
            } else if let Some(cms) = colormaps {
                let sampled = cms.sample_grass_linear_uv(col.colormap_uv[0], col.colormap_uv[1]);
                (modifier.apply_linear(sampled), is_modifier)
            } else {
                (col.grass_color, is_modifier)
            }
        }
        mtk_material::TINT_TYPE_FOLIAGE => {
            if col.has_custom_foliage {
                (col.foliage_color, true)
            } else if let Some(cms) = colormaps {
                (
                    cms.sample_foliage_linear_uv(col.colormap_uv[0], col.colormap_uv[1]),
                    false,
                )
            } else {
                (col.foliage_color, false)
            }
        }
        mtk_material::TINT_TYPE_DRY_FOLIAGE => {
            if col.has_custom_dry_foliage {
                (col.dry_foliage_color, true)
            } else if let Some(cms) = colormaps {
                (
                    cms.sample_dry_foliage_linear_uv(col.colormap_uv[0], col.colormap_uv[1]),
                    false,
                )
            } else {
                (col.dry_foliage_color, false)
            }
        }
        mtk_material::TINT_TYPE_WATER => (col.water_color, false),
        mtk_material::TINT_TYPE_HARDCODED => (default_tint_color, true),
        _ => (default_tint_color, false),
    }
}

/// Samples per-column/3D quart biome tint colors based on face tint configuration.
pub fn sample_biome_tint(
    biome_data: Option<&[SmoothedBiomeColumn]>,
    lx: usize,
    ly: usize,
    lz: usize,
    mut tint_data: [f32; 4],
    default_tint_color: [f32; 4],
    colormaps: Option<&mtk_material::VanillaColormaps>,
) -> ([f32; 4], [f32; 4]) {
    if tint_data[2] <= 0.0 {
        return (tint_data, default_tint_color);
    }
    let effective_cms: Option<&mtk_material::VanillaColormaps> = colormaps;

    if let Some(biome_cols) = biome_data {
        let col_idx = if biome_cols.len() == 1024 {
            let qy = (ly >> 2) & 3;
            (qy << 8) | ((lx & 15) << 4) | (lz & 15)
        } else {
            (lx & 15) * 16 + (lz & 15)
        };
        if col_idx < biome_cols.len() && tint_data[2] > 0.0 {
            let col = &biome_cols[col_idx];
            let tt = tint_data[3] as u8;
            let (c, has_custom) =
                sample_column_tint_color(col, tt, effective_cms, default_tint_color);
            if has_custom {
                tint_data[3] = mtk_material::TINT_TYPE_HARDCODED as f32;
            }
            (tint_data, c)
        } else {
            (tint_data, default_tint_color)
        }
    } else {
        (tint_data, default_tint_color)
    }
}

/// Samples 4-corner smoothed biome tint colors for a quad face.
///
/// When `smooth_blend` is true, each corner averages the dual-grid neighbor columns
/// around that vertex corner, producing C0 continuous, seamless color transitions across adjacent faces.
/// When `smooth_blend` is false, each corner receives the block's uniform column tint.
pub fn sample_quad_corner_biome_tints(
    biome_data: Option<&[SmoothedBiomeColumn]>,
    lx: usize,
    ly: usize,
    lz: usize,
    corner_offsets: &[[usize; 3]; 4],
    mut tint_data: [f32; 4],
    default_tint_color: [f32; 4],
    colormaps: Option<&mtk_material::VanillaColormaps>,
    smooth_blend: bool,
) -> ([f32; 4], [[f32; 4]; 4]) {
    if tint_data[2] <= 0.0 {
        return (tint_data, [default_tint_color; 4]);
    }
    let Some(biome_cols) = biome_data else {
        return (tint_data, [default_tint_color; 4]);
    };
    let tt = tint_data[3] as u8;
    if tt == mtk_material::TINT_TYPE_HARDCODED {
        return (tint_data, [default_tint_color; 4]);
    }

    if !smooth_blend {
        let (td, single_col) = sample_biome_tint(
            Some(biome_cols),
            lx,
            ly,
            lz,
            tint_data,
            default_tint_color,
            colormaps,
        );
        return (td, [single_col; 4]);
    }

    let is_3d = biome_cols.len() == 1024;
    let mut any_custom = false;
    let mut corners = [[0.0f32; 4]; 4];

    for (k, offset) in corner_offsets.iter().enumerate() {
        let cx = lx + offset[0];
        let cy = ly + offset[1];
        let cz = lz + offset[2];

        // `cy` is a local corner coordinate and can be 16 (ly = 15 plus the +1 corner
        // offset), so this must clamp rather than wrap modulo four.
        let qy = if is_3d { (cy >> 2).min(3) } else { 0 };

        // 4 dual-grid neighbor columns around this vertex corner
        let x0 = cx.saturating_sub(1).min(15);
        let x1 = cx.min(15);
        let z0 = cz.saturating_sub(1).min(15);
        let z1 = cz.min(15);

        let sample_indices = if x0 == x1 && z0 == z1 {
            [(x0, z0), (x0, z0), (x0, z0), (x0, z0)]
        } else if x0 == x1 {
            [(x0, z0), (x0, z1), (x0, z0), (x0, z1)]
        } else if z0 == z1 {
            [(x0, z0), (x1, z0), (x0, z0), (x1, z0)]
        } else {
            [(x0, z0), (x1, z0), (x0, z1), (x1, z1)]
        };

        let num_samples = if x0 == x1 && z0 == z1 {
            1
        } else if x0 == x1 || z0 == z1 {
            2
        } else {
            4
        };

        let mut sum_r = 0.0f32;
        let mut sum_g = 0.0f32;
        let mut sum_b = 0.0f32;
        let mut sum_a = 0.0f32;

        for &(sx, sz) in &sample_indices[..num_samples] {
            let col_idx = if is_3d {
                (qy << 8) | (sx << 4) | sz
            } else {
                sx * 16 + sz
            };
            if col_idx < biome_cols.len() {
                let col = &biome_cols[col_idx];
                let (c, has_cust) =
                    sample_column_tint_color(col, tt, colormaps, default_tint_color);
                if has_cust {
                    any_custom = true;
                }
                sum_r += c[0];
                sum_g += c[1];
                sum_b += c[2];
                sum_a += c[3];
            } else {
                sum_r += default_tint_color[0];
                sum_g += default_tint_color[1];
                sum_b += default_tint_color[2];
                sum_a += default_tint_color[3];
            }
        }

        let inv = 1.0 / (num_samples as f32);
        corners[k] = [sum_r * inv, sum_g * inv, sum_b * inv, sum_a * inv];
    }

    if any_custom {
        tint_data[3] = mtk_material::TINT_TYPE_HARDCODED as f32;
    }

    (tint_data, corners)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_corner_samples_the_highest_quart_layer() {
        use crate::biome::SmoothedBiomeColumn;

        // 3D biome mode: 64 quart layers, each expanded to a 16x16 column grid.
        let mut cols = vec![SmoothedBiomeColumn::default(); 1024];
        for (index, col) in cols.iter_mut().enumerate() {
            let layer = index >> 8;
            col.grass_color = if layer == 3 {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            };
            col.has_custom_grass = false;
        }

        // Top face of the topmost block: ly = 15, corner offsets reach cy = 16.
        let offsets = [[0, 0, 0], [0, 1, 0], [1, 0, 0], [1, 1, 0]];
        let tint_data = [0.0, 0.0, 1.0, mtk_material::TINT_TYPE_GRASS as f32];
        let (_, corners) = sample_quad_corner_biome_tints(
            Some(&cols),
            0,
            15,
            0,
            &offsets,
            tint_data,
            [0.0, 0.0, 0.0, 1.0],
            None,
            true,
        );

        for (index, corner) in corners.iter().enumerate() {
            assert!(
                (corner[1] - 1.0).abs() < 1e-5 && corner[0].abs() < 1e-5,
                "corner {index} at ly=15 must sample quart layer 3 (green), got {corner:?}"
            );
        }
    }

    #[test]
    fn test_sample_quad_corner_biome_tints_none_biome_data_does_not_panic() {
        let offsets = [[0, 0, 0]; 4];
        let default_color = [0.2, 0.8, 0.2, 1.0];

        // 1. None biome_data with positive tint weight, smooth_blend = false
        let tint_data = [0.0, 0.0, 1.0, 0.0];
        let (td, corners) = sample_quad_corner_biome_tints(
            None,
            0,
            0,
            0,
            &offsets,
            tint_data,
            default_color,
            None,
            false,
        );
        assert_eq!(td, tint_data);
        assert_eq!(corners, [default_color; 4]);

        // 2. None biome_data with positive tint weight, smooth_blend = true
        let (td2, corners2) = sample_quad_corner_biome_tints(
            None,
            0,
            0,
            0,
            &offsets,
            tint_data,
            default_color,
            None,
            true,
        );
        assert_eq!(td2, tint_data);
        assert_eq!(corners2, [default_color; 4]);

        // 3. Negative tint weight
        let tint_data_neg = [0.0, 0.0, 0.0, 0.0];
        let (td3, corners3) = sample_quad_corner_biome_tints(
            None,
            0,
            0,
            0,
            &offsets,
            tint_data_neg,
            default_color,
            None,
            true,
        );
        assert_eq!(td3, tint_data_neg);
        assert_eq!(corners3, [default_color; 4]);

        // 4. Hardcoded tint type
        let tint_data_hardcoded = [0.0, 0.0, 1.0, mtk_material::TINT_TYPE_HARDCODED as f32];
        let (td4, corners4) = sample_quad_corner_biome_tints(
            None,
            0,
            0,
            0,
            &offsets,
            tint_data_hardcoded,
            default_color,
            None,
            true,
        );
        assert_eq!(td4, tint_data_hardcoded);
        assert_eq!(corners4, [default_color; 4]);
    }

    /// `GrassColorModifier.DARK_FOREST` darkens the colormap sample; sampling alone is too bright.
    fn dark_forest_packed(sample: [f32; 3]) -> [f32; 3] {
        let bytes = sample.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u32);
        let packed = 0xFF00_0000 | (bytes[0] << 16) | (bytes[1] << 8) | bytes[2];
        let packed = ((packed & 0x00FE_FEFE) + 0x0028_340A) >> 1;
        [
            ((packed >> 16) & 0xFF) as f32 / 255.0,
            ((packed >> 8) & 0xFF) as f32 / 255.0,
            (packed & 0xFF) as f32 / 255.0,
        ]
    }

    fn sampled_grass(
        colormaps: &mtk_material::VanillaColormaps,
        column: &SmoothedBiomeColumn,
    ) -> [f32; 3] {
        colormaps.sample_grass_srgb_uv(column.colormap_uv[0], column.colormap_uv[1])
    }

    /// The sRGB transfer function, spelled out so the expectation does not reuse the code under test.
    fn to_linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    #[test]
    fn dark_forest_grass_is_darkened_after_sampling() {
        let colormaps = mtk_material::VanillaColormaps::load_embedded();
        let column = crate::biome::get_point_column_biome("minecraft:dark_forest", 0, 0);
        assert!(
            !column.has_custom_grass,
            "dark_forest has no explicit grass_color; it uses a modifier"
        );

        let (color, is_override) = sample_column_tint_color(
            &column,
            mtk_material::TINT_TYPE_GRASS,
            Some(&colormaps),
            [1.0, 1.0, 1.0, 1.0],
        );
        assert!(
            is_override,
            "the host cannot apply the modifier, so the colour must be shipped as an override"
        );

        let expected_srgb = dark_forest_packed(sampled_grass(&colormaps, &column));
        let expected: Vec<f32> = expected_srgb
            .iter()
            .map(|channel| to_linear(*channel))
            .collect();
        for (channel, want) in color[..3].iter().zip(expected.iter()) {
            assert!(
                (channel - want).abs() < 1.0 / 255.0,
                "dark_forest grass {channel} should be the darkened sample {want}"
            );
        }
    }

    #[test]
    fn swamp_grass_ignores_the_colormap_and_keeps_its_noise_colour() {
        let colormaps = mtk_material::VanillaColormaps::load_embedded();
        let column = crate::biome::get_point_column_biome("minecraft:swamp", 0, 0);
        let (color, _) = sample_column_tint_color(
            &column,
            mtk_material::TINT_TYPE_GRASS,
            Some(&colormaps),
            [1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(
            color, column.grass_color,
            "swamp grass must stay the position-dependent colour"
        );
    }
}
