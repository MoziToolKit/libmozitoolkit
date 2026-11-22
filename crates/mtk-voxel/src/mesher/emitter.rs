//! # Mesh Face Emitter
//!
//! Handles emitting baked faces and unit cube faces into `MeshData` and
//! `FaceAttributesCollector`, calculating vertex positions, coordinate
//! transforms, AO brightness interpolation, and anisotropy diagonal flips.

use glam::{IVec3, Vec3};
use mtk_core::Direction;
use mtk_core::{MeshData, UvLayout};
use mtk_model::BakedFace;
use std::borrow::Cow;

use crate::mesher::ao::should_flip_quad_diagonal;
use crate::mesher::collector::FaceAttributesCollector;
use crate::mesher::shading::{PreResolvedObjFace, ResolvedFaceShading};
use crate::types::MesherConfig;

#[inline]
fn push_corner_colors(
    mesh: &mut MeshData,
    average_color: [f32; 4],
    final_tint_data: [f32; 4],
    corner_tint_colors: &[[f32; 4]],
) {
    let colors_vec = mesh.colors.get_or_insert_with(Vec::new);
    if final_tint_data[2] > 0.0 {
        let is_grayscale = (average_color[0] - average_color[1]).abs() < 0.03
            && (average_color[1] - average_color[2]).abs() < 0.03;
        for &c in corner_tint_colors {
            let col = if is_grayscale {
                c
            } else {
                [
                    average_color[0] * c[0],
                    average_color[1] * c[1],
                    average_color[2] * c[2],
                    1.0,
                ]
            };
            colors_vec.push(col);
        }
    } else {
        for _ in 0..corner_tint_colors.len() {
            colors_vec.push(average_color);
        }
    }
}

/// Emits a baked model face into `MeshData` with AO shading and anisotropy diagonal flip.
#[inline]
pub fn emit_baked_face(
    mesh: &mut MeshData,
    face: &BakedFace,
    wx: f32,
    wy: f32,
    wz: f32,
    ao_brightness: [f32; 4],
    config: &MesherConfig,
    collector: &mut FaceAttributesCollector,
    shading: &ResolvedFaceShading<'_>,
    final_tint_data: [f32; 4],
    corner_tint_colors: [[f32; 4]; 4],
    block_pos: IVec3,
) {
    mesh.uv_layout = UvLayout::PerLoop;
    let base_idx = mesh.positions.len() as u32;
    let norm = config.transform_direction(face.normal);
    let n = [norm.x, norm.y, norm.z];

    for i in 0..4 {
        let v = face.vertices[i];
        let p = config.transform_position(Vec3::new(wx + v.x, wy + v.y, wz + v.z));
        mesh.positions.push([p.x, p.y, p.z]);
        mesh.normals.push(n);
        if let Some(ref uvs) = shading.override_uvs {
            mesh.uvs.push([uvs[i].x, uvs[i].y]);
        } else {
            mesh.uvs.push([face.uvs[i].x, 1.0 - face.uvs[i].y]);
        }

        collector.ao_values.push(ao_brightness[i]);
    }

    emit_quad_shading_and_topology(
        mesh,
        collector,
        [base_idx, base_idx + 1, base_idx + 2, base_idx + 3],
        ao_brightness,
        shading,
        final_tint_data,
        corner_tint_colors,
        block_pos,
        face.tint_index,
        face.uv_rot,
    );
}

#[inline]
pub(crate) fn emit_quad_shading_and_topology(
    mesh: &mut MeshData,
    collector: &mut FaceAttributesCollector,
    quad: [u32; 4],
    ao_brightness: [f32; 4],
    shading: &ResolvedFaceShading<'_>,
    final_tint_data: [f32; 4],
    corner_tint_colors: [[f32; 4]; 4],
    block_pos: IVec3,
    tint_index: i16,
    uv_rot: f32,
) {
    push_corner_colors(
        mesh,
        shading.average_color,
        final_tint_data,
        &corner_tint_colors,
    );

    mesh.quad_indices
        .get_or_insert_with(Vec::new)
        .extend_from_slice(&quad);

    let [v0, v1, v2, v3] = quad;
    // Triangulate with anisotropy diagonal flip
    if should_flip_quad_diagonal(ao_brightness) {
        mesh.indices.push(v0);
        mesh.indices.push(v1);
        mesh.indices.push(v2);

        mesh.indices.push(v0);
        mesh.indices.push(v2);
        mesh.indices.push(v3);
    } else {
        mesh.indices.push(v1);
        mesh.indices.push(v2);
        mesh.indices.push(v3);

        mesh.indices.push(v0);
        mesh.indices.push(v1);
        mesh.indices.push(v3);
    }

    mesh.face_materials.push(shading.mat_slot);
    mesh.face_tint_indices.push(tint_index);

    collector.push_face(
        &shading.texture_key,
        shading.chunk_id,
        [1.0, 1.0, 0.0, 0.0],
        uv_rot,
        final_tint_data,
        corner_tint_colors,
        block_pos,
        shading.anim_timing,
        shading.anim_frame_size,
    );
}

use super::grid_pool::{GridVertexPool, UNIT_CUBE_CORNERS};

/// Emits a unit cube face into `MeshData` with AO shading and anisotropy diagonal flip.
#[inline]
pub fn emit_unit_cube_face(
    mesh: &mut MeshData,
    dir: Direction,
    wx: f32,
    wy: f32,
    wz: f32,
    ao_brightness: [f32; 4],
    config: &MesherConfig,
    collector: &mut FaceAttributesCollector,
    shading: &ResolvedFaceShading<'_>,
    final_tint_data: [f32; 4],
    corner_tint_colors: [[f32; 4]; 4],
    block_pos: IVec3,
    mut grid_context: Option<(&mut GridVertexPool, [usize; 3], Vec3)>,
) {
    mesh.uv_layout = UvLayout::PerLoop;
    let norm = config.transform_direction(dir.normal());
    let n = [norm.x, norm.y, norm.z];

    let (v0_idx, v1_idx, v2_idx, v3_idx) =
        if let Some((ref mut pool, local_pos, origin)) = grid_context {
            let dir_idx = dir.to_index();
            let corners = UNIT_CUBE_CORNERS[dir_idx];
            let idx0 = pool.get_or_create(
                mesh,
                config,
                origin,
                local_pos[0] + corners[0][0],
                local_pos[1] + corners[0][1],
                local_pos[2] + corners[0][2],
                n,
            );
            let idx1 = pool.get_or_create(
                mesh,
                config,
                origin,
                local_pos[0] + corners[1][0],
                local_pos[1] + corners[1][1],
                local_pos[2] + corners[1][2],
                n,
            );
            let idx2 = pool.get_or_create(
                mesh,
                config,
                origin,
                local_pos[0] + corners[2][0],
                local_pos[1] + corners[2][1],
                local_pos[2] + corners[2][2],
                n,
            );
            let idx3 = pool.get_or_create(
                mesh,
                config,
                origin,
                local_pos[0] + corners[3][0],
                local_pos[1] + corners[3][1],
                local_pos[2] + corners[3][2],
                n,
            );
            (idx0, idx1, idx2, idx3)
        } else {
            let base_idx = mesh.positions.len() as u32;
            let (v0, v1, v2, v3) = match dir {
                Direction::East => (
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ),
                Direction::West => (
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(0.0, 1.0, 1.0),
                ),
                Direction::Up => (
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ),
                Direction::Down => (
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 1.0),
                ),
                Direction::South => (
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ),
                Direction::North => (
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ),
            };

            for v in [v0, v1, v2, v3] {
                let p = config.transform_position(Vec3::new(wx + v.x, wy + v.y, wz + v.z));
                mesh.positions.push([p.x, p.y, p.z]);
                mesh.normals.push(n);
            }
            (base_idx, base_idx + 1, base_idx + 2, base_idx + 3)
        };

    for i in 0..4 {
        collector.ao_values.push(ao_brightness[i]);
    }

    if let Some(ref uvs) = shading.override_uvs {
        for uv in uvs {
            mesh.uvs.push([uv.x, uv.y]);
        }
    } else {
        mesh.uvs.push([0.0, 1.0]);
        mesh.uvs.push([0.0, 0.0]);
        mesh.uvs.push([1.0, 0.0]);
        mesh.uvs.push([1.0, 1.0]);
    }

    emit_quad_shading_and_topology(
        mesh,
        collector,
        [v0_idx, v1_idx, v2_idx, v3_idx],
        ao_brightness,
        shading,
        final_tint_data,
        corner_tint_colors,
        block_pos,
        -1,
        0.0,
    );
}

/// Emits a 4-vertex Wavefront OBJ polygon face into a quad `MeshData` buffer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_obj_quad_face(
    mesh: &mut MeshData,
    pre_obj: &PreResolvedObjFace,
    wx: f32,
    wy: f32,
    wz: f32,
    ao_brightness: [f32; 4],
    config: &MesherConfig,
    collector: &mut FaceAttributesCollector,
    final_tint_data: [f32; 4],
    corner_tint_colors: [[f32; 4]; 4],
    block_pos: IVec3,
) {
    let face = &pre_obj.face;
    let pre = &pre_obj.pre;
    mesh.uv_layout = UvLayout::PerLoop;
    let base_idx = mesh.positions.len() as u32;
    let norm = config.transform_direction(face.normal);
    let n = [norm.x, norm.y, norm.z];

    for i in 0..4 {
        let v = face.vertices[i];
        let p = config.transform_position(Vec3::new(wx + v.x, wy + v.y, wz + v.z));
        mesh.positions.push([p.x, p.y, p.z]);
        mesh.normals.push(n);
        if let Some(uv) = pre_obj.override_uvs.as_ref().and_then(|u| u.get(i)) {
            mesh.uvs.push([uv.x, uv.y]);
        } else {
            let uv = face.uvs[i];
            mesh.uvs.push([uv.x, 1.0 - uv.y]);
        }
        collector.ao_values.push(ao_brightness[i]);
    }

    let shading = ResolvedFaceShading {
        texture_key: Cow::Borrowed(pre.source_texture_key.as_str()),
        override_uvs: None,
        mat_slot: pre.mat_slot,
        chunk_id: pre.chunk_id,
        tint_data: pre.tint_data,
        tint_color: pre.tint_color,
        anim_timing: pre.anim_timing,
        anim_frame_size: pre.anim_frame_size,
        average_color: pre.average_color,
    };

    emit_quad_shading_and_topology(
        mesh,
        collector,
        [base_idx, base_idx + 1, base_idx + 2, base_idx + 3],
        ao_brightness,
        &shading,
        final_tint_data,
        corner_tint_colors,
        block_pos,
        face.tint_index,
        0.0,
    );
}

/// Emits a 3-vertex or N-gon Wavefront OBJ polygon face as fan-triangulated triangles into a
/// pure-triangle `MeshData` buffer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_obj_tri_faces(
    mesh: &mut MeshData,
    pre_obj: &PreResolvedObjFace,
    wx: f32,
    wy: f32,
    wz: f32,
    ao_brightness: f32,
    config: &MesherConfig,
    collector: &mut FaceAttributesCollector,
    final_tint_data: [f32; 4],
    corner_tint_colors: [[f32; 4]; 3],
    block_pos: IVec3,
) {
    let face = &pre_obj.face;
    let pre = &pre_obj.pre;
    let n_verts = face.vertices.len();
    if n_verts < 3 {
        return;
    }

    mesh.uv_layout = UvLayout::PerLoop;
    mesh.quad_indices = None;
    let norm = config.transform_direction(face.normal);
    let n = [norm.x, norm.y, norm.z];

    for i in 1..(n_verts - 1) {
        let base_idx = mesh.positions.len() as u32;
        for corner in [0, i, i + 1] {
            let v = face.vertices[corner];
            let p = config.transform_position(Vec3::new(wx + v.x, wy + v.y, wz + v.z));
            mesh.positions.push([p.x, p.y, p.z]);
            mesh.normals.push(n);
            if let Some(uv) = pre_obj.override_uvs.as_ref().and_then(|u| u.get(corner)) {
                mesh.uvs.push([uv.x, uv.y]);
            } else {
                let uv = face.uvs[corner];
                mesh.uvs.push([uv.x, 1.0 - uv.y]);
            }
            collector.ao_values.push(ao_brightness);
        }

        push_corner_colors(
            mesh,
            pre.average_color,
            final_tint_data,
            &corner_tint_colors,
        );
        mesh.indices
            .extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        mesh.face_materials.push(pre.mat_slot);
        mesh.face_tint_indices.push(face.tint_index);

        collector.push_tri_face(
            &pre.source_texture_key,
            pre.chunk_id,
            [1.0, 1.0, 0.0, 0.0],
            0.0,
            final_tint_data,
            corner_tint_colors,
            block_pos,
            pre.anim_timing,
            pre.anim_frame_size,
        );
    }
}
