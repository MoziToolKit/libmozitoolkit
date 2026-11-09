use super::helpers::vec2_to_tuples;
use glam::Vec2;
use pyo3::prelude::*;

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

    let repaired =
        mtk_voxel::repair_quad_fluid_uv(&v_arr, &mut uv_arr, norm, force, min_slope_threshold);
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
            let count = mtk_voxel::batch_repair_fluid_uv(
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
            let count = mtk_voxel::batch_repair_fluid_uv(
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
    let count = mtk_voxel::batch_repair_fluid_uv(
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
    let arr = mtk_voxel::get_fluid_top_uvs_vec2(is_flowing, rotation);
    vec2_to_tuples(&arr)
}

#[pyfunction]
pub fn get_fluid_side_uvs(h_left_top: f32, h_right_top: f32) -> Vec<(f32, f32)> {
    let arr = mtk_voxel::get_fluid_side_uvs_vec2(h_left_top, h_right_top);
    vec2_to_tuples(&arr)
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

    if positions.is(uvs) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "positions and uvs must not be the same object",
        ));
    }
    if loop_verts.is(uvs) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "loop_verts and uvs must not be the same object",
        ));
    }
    if face_loop_starts.is(uvs) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "face_loop_starts and uvs must not be the same object",
        ));
    }
    if face_loop_totals.is(uvs) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "face_loop_totals and uvs must not be the same object",
        ));
    }
    if let Some(faces) = face_indices {
        if faces.is(uvs) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "face_indices and uvs must not be the same object",
            ));
        }
    }

    let pos_slice = get_cow_slice_f32(positions)?;
    let loop_slice = get_cow_slice_u32(loop_verts)?;
    let starts_slice = get_cow_slice_u32(face_loop_starts)?;
    let totals_slice = get_cow_slice_u32(face_loop_totals)?;

    let faces_slice = if let Some(faces) = face_indices {
        Some(get_cow_slice_u32(faces)?)
    } else {
        None
    };

    let check_overlap = |uvs_start: usize, uvs_len_bytes: usize| -> PyResult<()> {
        let uvs_end = uvs_start + uvs_len_bytes;

        let check_buf = |name: &str, slice_start: usize, slice_len_bytes: usize| -> PyResult<()> {
            let slice_end = slice_start + slice_len_bytes;
            if (slice_start < uvs_end) && (uvs_start < slice_end) {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "{} and uvs buffers must not overlap in memory",
                    name
                )));
            }
            Ok(())
        };

        check_buf(
            "positions",
            pos_slice.as_ptr() as usize,
            pos_slice.len() * std::mem::size_of::<f32>(),
        )?;
        check_buf(
            "loop_verts",
            loop_slice.as_ptr() as usize,
            loop_slice.len() * std::mem::size_of::<u32>(),
        )?;
        check_buf(
            "face_loop_starts",
            starts_slice.as_ptr() as usize,
            starts_slice.len() * std::mem::size_of::<u32>(),
        )?;
        check_buf(
            "face_loop_totals",
            totals_slice.as_ptr() as usize,
            totals_slice.len() * std::mem::size_of::<u32>(),
        )?;
        if let Some(ref faces) = faces_slice {
            check_buf(
                "face_indices",
                faces.as_ptr() as usize,
                faces.len() * std::mem::size_of::<u32>(),
            )?;
        }
        Ok(())
    };

    // Obtain mutable slice to uvs buffer
    if let Ok(buf) = PyBuffer::<f32>::get(uvs) {
        if buf.readonly() {
            return Err(pyo3::exceptions::PyBufferError::new_err(
                "uvs buffer must be writable",
            ));
        }
        if buf.is_c_contiguous() {
            check_overlap(buf.buf_ptr() as usize, buf.len_bytes())?;
            let uvs_mut: &mut [f32] = unsafe {
                std::slice::from_raw_parts_mut(buf.buf_ptr() as *mut f32, buf.item_count())
            };
            let count = py.detach(|| {
                mtk_voxel::repair_mesh_fluid_uv_inplace(
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
        if buf.readonly() {
            return Err(pyo3::exceptions::PyBufferError::new_err(
                "uvs buffer must be writable",
            ));
        }
        let ptr = buf.buf_ptr();
        let len_bytes = buf.len_bytes();
        let is_aligned = (ptr as usize) % std::mem::align_of::<f32>() == 0;
        let is_len_valid = len_bytes % std::mem::size_of::<f32>() == 0;
        if is_aligned && is_len_valid && buf.is_c_contiguous() {
            check_overlap(ptr as usize, len_bytes)?;
            let uvs_mut: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, len_bytes / 4) };
            let count = py.detach(|| {
                mtk_voxel::repair_mesh_fluid_uv_inplace(
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
