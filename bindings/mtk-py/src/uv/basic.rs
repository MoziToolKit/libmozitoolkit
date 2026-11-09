use super::helpers::{tuples_to_vec2, vec2_to_tuples};
use mtk_core as uv;
use pyo3::prelude::*;

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
