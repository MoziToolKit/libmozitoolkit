//! Fluid mesh geometry emission (top, bottom, and 4 sides).

use glam::Vec3;
use mtk_core::Direction;
use mtk_core::MeshData;
use mtk_cull::parse_block_name_and_props;
use mtk_cull::FaceCuller;

use crate::fluid::uv::{get_fluid_side_uvs, get_fluid_top_uvs};
use crate::types::MesherConfig;

pub use super::height::*;

/// Emits complete fluid geometry (top, bottom, and 4 sides) for a block into `MeshData`.
pub fn emit_fluid_geometry<F>(
    mesh: &mut MeshData,
    x: i32,
    y: i32,
    z: i32,
    wx: f32,
    wy: f32,
    wz: f32,
    state_str: &str,
    mut get_state: F,
    culler: &FaceCuller,
    config: &MesherConfig,
    material_slot: u16,
    mut collector: Option<&mut crate::mesher::FaceAttributesCollector>,
    tint_color: Option<[f32; 4]>,
) -> usize
where
    F: FnMut(i32, i32, i32) -> String,
{
    let (_, props) = parse_block_name_and_props(state_str);
    let is_waterlogged = props.get("waterlogged").map(|v| v.as_str()) == Some("true");

    let fluid_type = match FluidType::from_name(state_str) {
        Some(ft) => ft,
        None => {
            if is_waterlogged {
                FluidType::Water
            } else {
                return 0;
            }
        }
    };

    let (c_nw, c_ne, c_se, c_sw) =
        calculate_fluid_corner_heights(&mut get_state, x, y, z, fluid_type);
    let own_height = get_fluid_base_height(state_str);
    let (flow_vx, flow_vz, flow_angle) =
        calculate_fluid_flow_vector(&mut get_state, x, y, z, fluid_type, own_height);
    let level_int: u32 = props.get("level").and_then(|l| l.parse().ok()).unwrap_or(0);
    let is_flowing = (level_int > 0 && !is_waterlogged)
        || flow_vx.abs() > 1e-4
        || flow_vz.abs() > 1e-4
        || state_str.contains("flowing_");

    let fluid_state_str = match fluid_type {
        FluidType::Water => "minecraft:water[level=0]",
        FluidType::Lava => "minecraft:lava[level=0]",
    };
    let fluid_meta = culler.get_meta(fluid_state_str, None, None);
    // Vanilla self-occlusion only matters for the solid host state of waterlogged blocks.
    let self_meta = is_waterlogged.then(|| culler.get_meta(state_str, None, None));

    // Vanilla corner heights used for per-face occlusion:
    // top = min of all four corners, each side = max of its two corners.
    let up_height = c_nw.min(c_sw).min(c_se).min(c_ne);

    let (still_sprite, flow_sprite) = if let Some(atlas) = &config.atlas_address_map {
        let (still_name, flow_name) = match fluid_type {
            FluidType::Water => ("block/water_still", "block/water_flow"),
            FluidType::Lava => ("block/lava_still", "block/lava_flow"),
        };
        let still = mtk_material::MaterialResolver::resolve(
            still_name,
            config.custom_aliases.as_deref(),
            atlas,
        )
        .map(|(res, mut sp)| {
            if config.enable_animated_textures {
                if let Some(anim_sp) = crate::mesher::shading::lookup_animated_from_cache(
                    &res.as_string(),
                    config.custom_aliases.as_deref(),
                    atlas,
                )
                .or_else(|| {
                    crate::mesher::shading::lookup_animated_from_cache(
                        still_name,
                        config.custom_aliases.as_deref(),
                        atlas,
                    )
                }) {
                    sp = anim_sp;
                }
            } else if sp.is_animated {
                if let Some(static_sp) = atlas
                    .lookup_static(&res)
                    .or_else(|| atlas.lookup_static_str(still_name))
                {
                    sp = static_sp;
                }
            }
            let (timing, fsize) = crate::mesher::shading::extract_anim_metrics(Some(sp));
            (
                res.as_string(),
                sp.chunk_id,
                sp.texture_id,
                sp.frame_0_uv_bounds,
                timing,
                fsize,
            )
        });
        let flow = mtk_material::MaterialResolver::resolve(
            flow_name,
            config.custom_aliases.as_deref(),
            atlas,
        )
        .map(|(res, mut sp)| {
            if config.enable_animated_textures {
                if let Some(anim_sp) = crate::mesher::shading::lookup_animated_from_cache(
                    &res.as_string(),
                    config.custom_aliases.as_deref(),
                    atlas,
                )
                .or_else(|| {
                    crate::mesher::shading::lookup_animated_from_cache(
                        flow_name,
                        config.custom_aliases.as_deref(),
                        atlas,
                    )
                }) {
                    sp = anim_sp;
                }
            } else if sp.is_animated {
                if let Some(static_sp) = atlas
                    .lookup_static(&res)
                    .or_else(|| atlas.lookup_static_str(flow_name))
                {
                    sp = static_sp;
                }
            }
            let (timing, fsize) = crate::mesher::shading::extract_anim_metrics(Some(sp));
            (
                res.as_string(),
                sp.chunk_id,
                sp.texture_id,
                sp.frame_0_uv_bounds,
                timing,
                fsize,
            )
        });
        (still, flow)
    } else {
        (None, None)
    };

    let mut faces_emitted = 0;

    // Helper to emit quad face
    let mut emit_quad =
        |verts: [Vec3; 4], raw_uvs: [[f32; 2]; 4], norm: Vec3, face_flowing: bool| {
            let base_idx = mesh.positions.len() as u32;
            let norm_transformed = config.transform_direction(norm);
            let n = [norm_transformed.x, norm_transformed.y, norm_transformed.z];

            let target_sprite = if face_flowing {
                flow_sprite.as_ref().or(still_sprite.as_ref())
            } else {
                still_sprite.as_ref().or(flow_sprite.as_ref())
            };

            let (final_mat_slot, uvs, chunk_id, source_key, anim_timing, anim_frame_size) =
                if let Some((res_name, cid, _tid, bounds, timing, fsize)) = target_sprite {
                    let u_min = bounds[0];
                    let v_min = bounds[1];
                    let u_span = bounds[2] - u_min;
                    let v_span = bounds[3] - v_min;
                    let mapped = [
                        [
                            u_min + raw_uvs[0][0] * u_span,
                            v_min + (1.0 - raw_uvs[0][1]) * v_span,
                        ],
                        [
                            u_min + raw_uvs[1][0] * u_span,
                            v_min + (1.0 - raw_uvs[1][1]) * v_span,
                        ],
                        [
                            u_min + raw_uvs[2][0] * u_span,
                            v_min + (1.0 - raw_uvs[2][1]) * v_span,
                        ],
                        [
                            u_min + raw_uvs[3][0] * u_span,
                            v_min + (1.0 - raw_uvs[3][1]) * v_span,
                        ],
                    ];
                    (*cid, mapped, *cid as i32, res_name.clone(), *timing, *fsize)
                } else {
                    let default_name = match fluid_type {
                        FluidType::Water => {
                            if face_flowing {
                                "minecraft:block/water_flow"
                            } else {
                                "minecraft:block/water_still"
                            }
                        }
                        FluidType::Lava => {
                            if face_flowing {
                                "minecraft:block/lava_flow"
                            } else {
                                "minecraft:block/lava_still"
                            }
                        }
                    };
                    let mapped_fallback = [
                        [raw_uvs[0][0], 1.0 - raw_uvs[0][1]],
                        [raw_uvs[1][0], 1.0 - raw_uvs[1][1]],
                        [raw_uvs[2][0], 1.0 - raw_uvs[2][1]],
                        [raw_uvs[3][0], 1.0 - raw_uvs[3][1]],
                    ];
                    (
                        material_slot,
                        mapped_fallback,
                        0,
                        default_name.to_string(),
                        [1.0, 1.0, 0.0],
                        [16.0, 16.0, 0.0],
                    )
                };

            for i in 0..4 {
                let p = config.transform_position(verts[i]);
                mesh.positions.push([p.x, p.y, p.z]);
                mesh.normals.push(n);
                mesh.uvs.push(uvs[i]);
            }

            mesh.quad_indices
                .get_or_insert_with(Vec::new)
                .extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx + 3]);

            mesh.indices.push(base_idx);
            mesh.indices.push(base_idx + 1);
            mesh.indices.push(base_idx + 2);

            mesh.indices.push(base_idx);
            mesh.indices.push(base_idx + 2);
            mesh.indices.push(base_idx + 3);

            mesh.face_materials.push(final_mat_slot);
            mesh.face_tint_indices
                .push(if fluid_type == FluidType::Water {
                    0
                } else {
                    -1
                });

            let fluid_color = if fluid_type == FluidType::Water {
                tint_color.unwrap_or([0.24, 0.44, 0.99, 1.0])
            } else {
                [0.95, 0.45, 0.05, 1.0]
            };
            mesh.colors
                .get_or_insert_with(Vec::new)
                .extend_from_slice(&[fluid_color; 4]);

            if let Some(ref mut col) = collector {
                col.ao_values.extend_from_slice(&[1.0, 1.0, 1.0, 1.0]);
                let (f_tint_data, f_tint_color) = if fluid_type == FluidType::Water {
                    (
                        [1.0, 1.0, 1.0, 3.0],
                        tint_color.unwrap_or([0.24, 0.44, 0.99, 1.0]),
                    )
                } else {
                    ([1.0, 1.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0])
                };
                col.push_face(
                    &source_key,
                    chunk_id,
                    [1.0, 1.0, 0.0, 0.0],
                    0.0,
                    f_tint_data,
                    f_tint_color,
                    glam::IVec3::new(x, y, z),
                    anim_timing,
                    anim_frame_size,
                );
            }

            faces_emitted += 1;
        };

    // 1. TOP FACE (UP)
    let up_state = get_state(x, y + 1, z);
    let up_meta = if !up_state.is_empty() && up_state != "minecraft:air" {
        Some(culler.get_meta(&up_state, None, None))
    } else {
        None
    };

    if culler.should_render_fluid_face(
        &fluid_meta,
        self_meta.as_deref(),
        up_meta.as_deref(),
        Direction::Up,
        up_height,
    ) {
        let v_nw = Vec3::new(wx, wy + c_nw, wz);
        let v_sw = Vec3::new(wx, wy + c_sw, wz + 1.0);
        let v_se = Vec3::new(wx + 1.0, wy + c_se, wz + 1.0);
        let v_ne = Vec3::new(wx + 1.0, wy + c_ne, wz);

        let top_uvs = get_fluid_top_uvs(is_flowing, if is_flowing { flow_angle } else { 0.0 });
        emit_quad([v_nw, v_sw, v_se, v_ne], top_uvs, Vec3::Y, is_flowing);
    }

    // 2. BOTTOM FACE (DOWN)
    let down_state = get_state(x, y - 1, z);
    let down_meta = if !down_state.is_empty() && down_state != "minecraft:air" {
        Some(culler.get_meta(&down_state, None, None))
    } else {
        None
    };

    if culler.should_render_fluid_face(
        &fluid_meta,
        self_meta.as_deref(),
        down_meta.as_deref(),
        Direction::Down,
        MAX_FLUID_HEIGHT,
    ) {
        let v_sw = Vec3::new(wx, wy, wz + 1.0);
        let v_nw = Vec3::new(wx, wy, wz);
        let v_ne = Vec3::new(wx + 1.0, wy, wz);
        let v_se = Vec3::new(wx + 1.0, wy, wz + 1.0);

        let bot_uvs = get_fluid_top_uvs(false, 0.0);
        emit_quad([v_sw, v_nw, v_ne, v_se], bot_uvs, -Vec3::Y, false);
    }

    // 3. SIDE FACES (North, South, West, East)
    // North (-Z)
    let n_state = get_state(x, y, z - 1);
    let n_meta = if !n_state.is_empty() && n_state != "minecraft:air" {
        Some(culler.get_meta(&n_state, None, None))
    } else {
        None
    };
    if culler.should_render_fluid_face(
        &fluid_meta,
        self_meta.as_deref(),
        n_meta.as_deref(),
        Direction::North,
        c_ne.max(c_nw),
    ) {
        let v_ne_top = Vec3::new(wx + 1.0, wy + c_ne, wz);
        let v_ne_bot = Vec3::new(wx + 1.0, wy, wz);
        let v_nw_bot = Vec3::new(wx, wy, wz);
        let v_nw_top = Vec3::new(wx, wy + c_nw, wz);
        let uvs = get_fluid_side_uvs(c_ne, c_nw);
        emit_quad(
            [v_ne_top, v_ne_bot, v_nw_bot, v_nw_top],
            uvs,
            -Vec3::Z,
            true,
        );
    }

    // South (+Z)
    let s_state = get_state(x, y, z + 1);
    let s_meta = if !s_state.is_empty() && s_state != "minecraft:air" {
        Some(culler.get_meta(&s_state, None, None))
    } else {
        None
    };
    if culler.should_render_fluid_face(
        &fluid_meta,
        self_meta.as_deref(),
        s_meta.as_deref(),
        Direction::South,
        c_sw.max(c_se),
    ) {
        let v_sw_top = Vec3::new(wx, wy + c_sw, wz + 1.0);
        let v_sw_bot = Vec3::new(wx, wy, wz + 1.0);
        let v_se_bot = Vec3::new(wx + 1.0, wy, wz + 1.0);
        let v_se_top = Vec3::new(wx + 1.0, wy + c_se, wz + 1.0);
        let uvs = get_fluid_side_uvs(c_sw, c_se);
        emit_quad([v_sw_top, v_sw_bot, v_se_bot, v_se_top], uvs, Vec3::Z, true);
    }

    // West (-X)
    let w_state = get_state(x - 1, y, z);
    let w_meta = if !w_state.is_empty() && w_state != "minecraft:air" {
        Some(culler.get_meta(&w_state, None, None))
    } else {
        None
    };
    if culler.should_render_fluid_face(
        &fluid_meta,
        self_meta.as_deref(),
        w_meta.as_deref(),
        Direction::West,
        c_nw.max(c_sw),
    ) {
        let v_nw_top = Vec3::new(wx, wy + c_nw, wz);
        let v_nw_bot = Vec3::new(wx, wy, wz);
        let v_sw_bot = Vec3::new(wx, wy, wz + 1.0);
        let v_sw_top = Vec3::new(wx, wy + c_sw, wz + 1.0);
        let uvs = get_fluid_side_uvs(c_nw, c_sw);
        emit_quad(
            [v_nw_top, v_nw_bot, v_sw_bot, v_sw_top],
            uvs,
            -Vec3::X,
            true,
        );
    }

    // East (+X)
    let e_state = get_state(x + 1, y, z);
    let e_meta = if !e_state.is_empty() && e_state != "minecraft:air" {
        Some(culler.get_meta(&e_state, None, None))
    } else {
        None
    };
    if culler.should_render_fluid_face(
        &fluid_meta,
        self_meta.as_deref(),
        e_meta.as_deref(),
        Direction::East,
        c_se.max(c_ne),
    ) {
        let v_se_top = Vec3::new(wx + 1.0, wy + c_se, wz + 1.0);
        let v_se_bot = Vec3::new(wx + 1.0, wy, wz + 1.0);
        let v_ne_bot = Vec3::new(wx + 1.0, wy, wz);
        let v_ne_top = Vec3::new(wx + 1.0, wy + c_ne, wz);
        let uvs = get_fluid_side_uvs(c_se, c_ne);
        emit_quad([v_se_top, v_se_bot, v_ne_bot, v_ne_top], uvs, Vec3::X, true);
    }

    faces_emitted
}
