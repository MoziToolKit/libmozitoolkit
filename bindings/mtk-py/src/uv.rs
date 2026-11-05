//! Python bindings for UV geometric algorithms.

use glam::Vec2;
use mtk_core as uv;
use pyo3::prelude::*;

#[inline]
fn tuples_to_vec2(uvs: &[(f32, f32)]) -> Vec<Vec2> {
    uvs.iter().map(|(u, v)| Vec2::new(*u, *v)).collect()
}

#[inline]
fn vec2_to_tuples(vecs: &[Vec2]) -> Vec<(f32, f32)> {
    vecs.iter().map(|v| (v.x, v.y)).collect()
}

#[pyfunction]
pub fn calculate_uv_area(uvs: Vec<(f32, f32)>) -> f32 {
    let vecs = tuples_to_vec2(&uvs);
    uv::calculate_uv_area(&vecs)
}

#[pyfunction]
pub fn get_uv_bounds(uvs: Vec<(f32, f32)>) -> (f32, f32, f32, f32, f32, f32) {
    let vecs = tuples_to_vec2(&uvs);
    let b = uv::get_uv_bounds(&vecs);
    (b.min.x, b.min.y, b.max.x, b.max.y, b.width(), b.height())
}

#[pyfunction]
pub fn get_uv_center(uvs: Vec<(f32, f32)>) -> (f32, f32) {
    let vecs = tuples_to_vec2(&uvs);
    let c = uv::get_uv_center(&vecs);
    (c.x, c.y)
}

#[pyfunction]
#[pyo3(signature = (uvs, area_threshold=None, dist_threshold=None, pixel_step=None))]
pub fn is_uv_collapsed(
    uvs: Vec<(f32, f32)>,
    area_threshold: Option<f32>,
    dist_threshold: Option<f32>,
    pixel_step: Option<(f32, f32)>,
) -> bool {
    let vecs = tuples_to_vec2(&uvs);
    uv::is_uv_collapsed(&vecs, area_threshold, dist_threshold, pixel_step)
}

#[pyfunction]
#[pyo3(signature = (angle_rad, tolerance=1e-3))]
pub fn is_orthogonal_angle(angle_rad: f32, tolerance: f32) -> bool {
    uv::is_orthogonal_angle(angle_rad, tolerance)
}

#[pyfunction]
#[pyo3(signature = (uvs, tolerance=1e-3))]
pub fn detect_uv_rotation(uvs: Vec<(f32, f32)>, tolerance: f32) -> f32 {
    let vecs = tuples_to_vec2(&uvs);
    uv::detect_uv_rotation(&vecs, tolerance)
}

#[pyfunction]
#[pyo3(signature = (uvs, angle=None))]
pub fn straighten_uv(uvs: Vec<(f32, f32)>, angle: Option<f32>) -> (f32, bool, Vec<(f32, f32)>) {
    let vecs = tuples_to_vec2(&uvs);
    let (ang, was_straightened, new_vecs) = uv::straighten_uv(&vecs, angle);
    (ang, was_straightened, vec2_to_tuples(&new_vecs))
}

#[pyfunction]
pub fn scale_uv(uvs: Vec<(f32, f32)>, scale_factor: f32) -> Vec<(f32, f32)> {
    let vecs = tuples_to_vec2(&uvs);
    let scaled = uv::scale_uv(&vecs, scale_factor);
    vec2_to_tuples(&scaled)
}

#[pyfunction]
#[pyo3(signature = (uvs, epsilon=1e-6))]
pub fn normalize_uv_for_atlas_tiling(
    uvs: Vec<(f32, f32)>,
    epsilon: f32,
) -> (Vec<(f32, f32)>, (f32, f32, f32), (f32, f32, f32)) {
    let vecs = tuples_to_vec2(&uvs);
    let (norm_vecs, scale, loc) = uv::normalize_uv_for_atlas_tiling(&vecs, epsilon);
    (
        vec2_to_tuples(&norm_vecs),
        (scale[0], scale[1], scale[2]),
        (loc[0], loc[1], loc[2]),
    )
}

#[pyfunction]
#[pyo3(signature = (uvs, epsilon=1e-4))]
pub fn uv_requires_atlas_tiling(uvs: Vec<(f32, f32)>, epsilon: f32) -> bool {
    let vecs = tuples_to_vec2(&uvs);
    uv::uv_requires_atlas_tiling(&vecs, epsilon)
}

#[pyfunction]
#[pyo3(signature = (u, v, scale=(1.0, 1.0, 1.0), location=(0.0, 0.0, 0.0), rotation=0.0))]
pub fn restore_atlas_tiling_uv(
    u: f32,
    v: f32,
    scale: (f32, f32, f32),
    location: (f32, f32, f32),
    rotation: f32,
) -> (f32, f32) {
    uv::restore_atlas_tiling_uv(
        u,
        v,
        [scale.0, scale.1, scale.2],
        [location.0, location.1, location.2],
        rotation,
    )
}

#[pyfunction]
#[pyo3(signature = (verts, uvs, normal=None, force=false, min_slope_threshold=0.005))]
pub fn repair_quad_fluid_uv(
    verts: Vec<(f32, f32, f32)>,
    uvs: Vec<(f32, f32)>,
    normal: Option<(f32, f32, f32)>,
    force: bool,
    min_slope_threshold: f32,
) -> PyResult<(bool, Vec<(f32, f32)>)> {
    if verts.len() != 4 || uvs.len() != 4 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "repair_quad_fluid_uv requires exactly 4 vertices and 4 UV coordinates",
        ));
    }

    let v_arr = [
        glam::Vec3::new(verts[0].0, verts[0].1, verts[0].2),
        glam::Vec3::new(verts[1].0, verts[1].1, verts[1].2),
        glam::Vec3::new(verts[2].0, verts[2].1, verts[2].2),
        glam::Vec3::new(verts[3].0, verts[3].1, verts[3].2),
    ];
    let mut uv_arr = [
        Vec2::new(uvs[0].0, uvs[0].1),
        Vec2::new(uvs[1].0, uvs[1].1),
        Vec2::new(uvs[2].0, uvs[2].1),
        Vec2::new(uvs[3].0, uvs[3].1),
    ];
    let norm = normal.map(|(x, y, z)| glam::Vec3::new(x, y, z));

    let repaired = mtk_voxel::fluid::uv::repair_quad_fluid_uv(
        &v_arr,
        &mut uv_arr,
        norm,
        force,
        min_slope_threshold,
    );
    let out_uvs = vec2_to_tuples(&uv_arr);
    Ok((repaired, out_uvs))
}

#[pyfunction]
#[pyo3(signature = (verts_flat, uvs_flat, normals_flat=None, force=false, min_slope_threshold=0.005))]
pub fn batch_repair_fluid_uv<'py>(
    py: Python<'py>,
    verts_flat: &Bound<'py, PyAny>,
    uvs_flat: &Bound<'py, PyAny>,
    normals_flat: Option<&Bound<'py, PyAny>>,
    force: bool,
    min_slope_threshold: f32,
) -> PyResult<(usize, Bound<'py, PyAny>)> {
    use pyo3::buffer::PyBuffer;
    use pyo3::types::PyList;

    // 1. Resolve verts_flat buffer or Vec
    let py_buf_verts_f32 = PyBuffer::<f32>::get(verts_flat).ok();
    let py_buf_verts_u8 = if py_buf_verts_f32.is_none() {
        PyBuffer::<u8>::get(verts_flat).ok()
    } else {
        None
    };
    let verts_vec: Vec<f32>;
    let verts_slice: &[f32] = if let Some(ref buf) = py_buf_verts_f32 {
        if buf.is_c_contiguous() {
            unsafe { std::slice::from_raw_parts(buf.buf_ptr() as *const f32, buf.item_count()) }
        } else {
            verts_vec = verts_flat.extract()?;
            &verts_vec
        }
    } else if let Some(ref buf) = py_buf_verts_u8 {
        let ptr = buf.buf_ptr();
        let len_bytes = buf.len_bytes();
        let is_aligned = (ptr as usize) % std::mem::align_of::<f32>() == 0;
        let is_len_valid = len_bytes % std::mem::size_of::<f32>() == 0;
        if is_aligned && is_len_valid && buf.is_c_contiguous() {
            unsafe { std::slice::from_raw_parts(ptr as *const f32, len_bytes / 4) }
        } else {
            verts_vec = verts_flat.extract()?;
            &verts_vec
        }
    } else {
        verts_vec = verts_flat.extract()?;
        &verts_vec
    };

    // 2. Resolve normals_flat buffer or Vec
    let py_buf_normals_f32 = normals_flat.and_then(|n| PyBuffer::<f32>::get(n).ok());
    let py_buf_normals_u8 = if py_buf_normals_f32.is_none() {
        normals_flat.and_then(|n| PyBuffer::<u8>::get(n).ok())
    } else {
        None
    };
    let normals_vec: Option<Vec<f32>>;
    let normals_slice: Option<&[f32]> = if let Some(ref buf) = py_buf_normals_f32 {
        if buf.is_c_contiguous() {
            normals_vec = None;
            Some(unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const f32, buf.item_count())
            })
        } else {
            normals_vec = normals_flat.map(|n| n.extract()).transpose()?;
            normals_vec.as_deref()
        }
    } else if let Some(ref buf) = py_buf_normals_u8 {
        let ptr = buf.buf_ptr();
        let len_bytes = buf.len_bytes();
        let is_aligned = (ptr as usize) % std::mem::align_of::<f32>() == 0;
        let is_len_valid = len_bytes % std::mem::size_of::<f32>() == 0;
        if is_aligned && is_len_valid && buf.is_c_contiguous() {
            normals_vec = None;
            Some(unsafe { std::slice::from_raw_parts(ptr as *const f32, len_bytes / 4) })
        } else {
            normals_vec = normals_flat.map(|n| n.extract()).transpose()?;
            normals_vec.as_deref()
        }
    } else if let Some(n) = normals_flat {
        normals_vec = Some(n.extract()?);
        normals_vec.as_deref()
    } else {
        normals_vec = None;
        None
    };
    let _ = &normals_vec;

    // 3. Resolve uvs_flat buffer (mutable in-place) or fallback to Vec
    let py_buf_uvs_f32 = PyBuffer::<f32>::get(uvs_flat).ok();
    if let Some(ref buf) = py_buf_uvs_f32 {
        if !buf.readonly() && buf.is_c_contiguous() {
            let uvs_mut: &mut [f32] = unsafe {
                std::slice::from_raw_parts_mut(buf.buf_ptr() as *mut f32, buf.item_count())
            };
            let count = mtk_voxel::fluid::uv::batch_repair_fluid_uv(
                verts_slice,
                uvs_mut,
                normals_slice,
                force,
                min_slope_threshold,
            );
            return Ok((count, uvs_flat.clone()));
        }
    }
    let py_buf_uvs_u8 = PyBuffer::<u8>::get(uvs_flat).ok();
    if let Some(ref buf) = py_buf_uvs_u8 {
        let ptr = buf.buf_ptr();
        let len_bytes = buf.len_bytes();
        let is_aligned = (ptr as usize) % std::mem::align_of::<f32>() == 0;
        let is_len_valid = len_bytes % std::mem::size_of::<f32>() == 0;
        if !buf.readonly() && is_aligned && is_len_valid && buf.is_c_contiguous() {
            let uvs_mut: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, len_bytes / 4) };
            let count = mtk_voxel::fluid::uv::batch_repair_fluid_uv(
                verts_slice,
                uvs_mut,
                normals_slice,
                force,
                min_slope_threshold,
            );
            return Ok((count, uvs_flat.clone()));
        }
    }

    // Fallback path: copy into Vec<f32> and construct PyList
    let mut uvs_vec: Vec<f32> = uvs_flat.extract()?;
    let count = mtk_voxel::fluid::uv::batch_repair_fluid_uv(
        verts_slice,
        &mut uvs_vec,
        normals_slice,
        force,
        min_slope_threshold,
    );
    let py_list = PyList::new(py, &uvs_vec)?;
    Ok((count, py_list.into_any()))
}

#[pyfunction]
#[pyo3(signature = (is_flowing=true, rotation=0.0))]
pub fn get_fluid_top_uvs(is_flowing: bool, rotation: f32) -> Vec<(f32, f32)> {
    let arr = mtk_voxel::fluid::uv::get_fluid_top_uvs_vec2(is_flowing, rotation);
    vec2_to_tuples(&arr)
}

#[pyfunction]
pub fn get_fluid_side_uvs(h_left_top: f32, h_right_top: f32) -> Vec<(f32, f32)> {
    let arr = mtk_voxel::fluid::uv::get_fluid_side_uvs_vec2(h_left_top, h_right_top);
    vec2_to_tuples(&arr)
}

/// Reconstructs 4 UV corner coordinates for an extruded side quad polygon.
#[pyfunction]
#[pyo3(signature = (
    uv_base_a,
    uv_base_b,
    top_normal,
    extrude_vec,
    mode="SMART",
    step_u=1.0/16.0,
    step_v=1.0/16.0,
    top_uv_bounds=None,
    adjacent_uv_strip=None
))]
pub fn repair_extruded_side_uv(
    uv_base_a: (f32, f32),
    uv_base_b: (f32, f32),
    top_normal: (f32, f32, f32),
    extrude_vec: (f32, f32, f32),
    mode: &str,
    step_u: f32,
    step_v: f32,
    top_uv_bounds: Option<(f32, f32, f32, f32)>,
    adjacent_uv_strip: Option<Vec<(f32, f32)>>,
) -> Vec<(f32, f32)> {
    let m = match mode.to_uppercase().as_str() {
        "INWARD" => mtk_core::ExtrudeUvMode::Inward,
        "OUTWARD" => mtk_core::ExtrudeUvMode::Outward,
        _ => mtk_core::ExtrudeUvMode::Smart,
    };

    let bounds = if let Some((min_u, min_v, max_u, max_v)) = top_uv_bounds {
        mtk_core::Aabb2d::new(Vec2::new(min_u, min_v), Vec2::new(max_u, max_v))
    } else {
        mtk_core::Aabb2d::new(
            Vec2::new(uv_base_a.0.min(uv_base_b.0), uv_base_a.1.min(uv_base_b.1)),
            Vec2::new(uv_base_a.0.max(uv_base_b.0), uv_base_a.1.max(uv_base_b.1)),
        )
    };

    let strip_arr = adjacent_uv_strip.and_then(|s| {
        if s.len() == 4 {
            Some([
                [s[0].0, s[0].1],
                [s[1].0, s[1].1],
                [s[2].0, s[2].1],
                [s[3].0, s[3].1],
            ])
        } else {
            None
        }
    });

    let res = mtk_core::repair_extruded_side_uv(
        [uv_base_a.0, uv_base_a.1],
        [uv_base_b.0, uv_base_b.1],
        [top_normal.0, top_normal.1, top_normal.2],
        [extrude_vec.0, extrude_vec.1, extrude_vec.2],
        m,
        step_u,
        step_v,
        bounds,
        strip_arr,
    );

    vec![
        (res[0][0], res[0][1]),
        (res[1][0], res[1][1]),
        (res[2][0], res[2][1]),
        (res[3][0], res[3][1]),
    ]
}

/// Generates random extrusion height displacement values for face center coordinates.
#[pyfunction]
#[pyo3(signature = (
    centers,
    noise_type="RANDOM",
    min_height=0.0,
    max_height=1.0,
    noise_scale=1.0,
    seed=1234,
    discrete_steps=None
))]
pub fn generate_random_extrude_heights(
    centers: Vec<(f32, f32, f32)>,
    noise_type: &str,
    min_height: f32,
    max_height: f32,
    noise_scale: f32,
    seed: u32,
    discrete_steps: Option<u32>,
) -> Vec<f32> {
    let n_type = match noise_type.to_uppercase().as_str() {
        "PERLIN" => mtk_core::ExtrudeNoiseType::Perlin,
        "CELL" | "CELLULAR" | "VORONOI" => mtk_core::ExtrudeNoiseType::Cellular,
        _ => mtk_core::ExtrudeNoiseType::UniformRandom,
    };

    let pts: Vec<[f32; 3]> = centers.iter().map(|&(x, y, z)| [x, y, z]).collect();

    mtk_core::generate_extrude_heights(
        &pts,
        n_type,
        min_height,
        max_height,
        noise_scale,
        seed,
        discrete_steps,
    )
}

/// In-place batch repair for inverted fluid UVs directly on indexed mesh geometry.
///
/// Modifies the `uvs` buffer directly in-place if it is a writable numpy ndarray / buffer.
/// Returns the number of quad faces whose UVs were repaired.
#[pyfunction]
#[pyo3(signature = (positions, loop_verts, face_loop_starts, face_loop_totals, uvs, face_indices=None, force=false, min_slope_threshold=0.005))]
pub fn repair_mesh_fluid_uv_inplace<'py>(
    py: Python<'py>,
    positions: &Bound<'py, PyAny>,
    loop_verts: &Bound<'py, PyAny>,
    face_loop_starts: &Bound<'py, PyAny>,
    face_loop_totals: &Bound<'py, PyAny>,
    uvs: &Bound<'py, PyAny>,
    face_indices: Option<&Bound<'py, PyAny>>,
    force: bool,
    min_slope_threshold: f32,
) -> PyResult<usize> {
    use crate::mesh::topology::{get_cow_slice_f32, get_cow_slice_u32};
    use pyo3::buffer::PyBuffer;

    let pos_slice = get_cow_slice_f32(positions)?;
    let loop_slice = get_cow_slice_u32(loop_verts)?;
    let starts_slice = get_cow_slice_u32(face_loop_starts)?;
    let totals_slice = get_cow_slice_u32(face_loop_totals)?;

    let faces_slice = if let Some(faces) = face_indices {
        Some(get_cow_slice_u32(faces)?)
    } else {
        None
    };

    // Obtain mutable slice to uvs buffer
    if let Ok(buf) = PyBuffer::<f32>::get(uvs) {
        if !buf.readonly() && buf.is_c_contiguous() {
            let uvs_mut: &mut [f32] = unsafe {
                std::slice::from_raw_parts_mut(buf.buf_ptr() as *mut f32, buf.item_count())
            };
            let count = py.detach(|| {
                mtk_voxel::fluid::uv::repair_mesh_fluid_uv_inplace(
                    &pos_slice,
                    &loop_slice,
                    &starts_slice,
                    &totals_slice,
                    uvs_mut,
                    faces_slice.as_deref(),
                    force,
                    min_slope_threshold,
                )
            });
            return Ok(count);
        }
    }

    if let Ok(buf) = PyBuffer::<u8>::get(uvs) {
        let ptr = buf.buf_ptr();
        let len_bytes = buf.len_bytes();
        let is_aligned = (ptr as usize) % std::mem::align_of::<f32>() == 0;
        let is_len_valid = len_bytes % std::mem::size_of::<f32>() == 0;
        if !buf.readonly() && is_aligned && is_len_valid && buf.is_c_contiguous() {
            let uvs_mut: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, len_bytes / 4) };
            let count = py.detach(|| {
                mtk_voxel::fluid::uv::repair_mesh_fluid_uv_inplace(
                    &pos_slice,
                    &loop_slice,
                    &starts_slice,
                    &totals_slice,
                    uvs_mut,
                    faces_slice.as_deref(),
                    force,
                    min_slope_threshold,
                )
            });
            return Ok(count);
        }
    }

    Err(pyo3::exceptions::PyBufferError::new_err(
        "uvs parameter must be a writable C-contiguous f32 buffer (e.g. numpy.ndarray[float32])",
    ))
}
