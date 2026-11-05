//! # `mtk-py` Subdivision Bindings
//!
//! Exposes adaptive pixel grid subdivision and polygon slicing algorithms to Python.

use pyo3::prelude::*;

use crate::mesh::PyMeshData;

/// Calculates adaptive (cols, rows) subdivisions based on UV bounds and texture size.
#[pyfunction]
#[pyo3(signature = (uvs, tex_w, tex_h, pixels_per_face=1.0, max_subdivisions=64))]
pub fn calculate_face_target_grid(
    uvs: Vec<[f32; 2]>,
    tex_w: u32,
    tex_h: u32,
    pixels_per_face: f32,
    max_subdivisions: u32,
) -> (u32, u32) {
    mtk_core::calculate_face_target_grid(&uvs, tex_w, tex_h, pixels_per_face, max_subdivisions)
}

/// Calculates non-uniform [0, 1] parameter cut factors along U and V axes of a quad face,
/// directly snapping interior cuts to the integer pixel grid lines of the texture image.
#[pyfunction]
#[pyo3(signature = (uvs, tex_w, tex_h, pixels_per_face=1.0, max_subdivisions=64))]
pub fn calculate_pixel_grid_cut_factors(
    uvs: Vec<[f32; 2]>,
    tex_w: u32,
    tex_h: u32,
    pixels_per_face: f32,
    max_subdivisions: u32,
) -> (Vec<f32>, Vec<f32>) {
    mtk_core::calculate_pixel_grid_cut_factors(
        &uvs,
        tex_w,
        tex_h,
        pixels_per_face,
        max_subdivisions,
    )
}

/// Slices a 2D/3D polygon strictly along the 2D texture pixel grid lines (X = 1, 2... and Y = 1, 2...).
///
/// Returns `(positions, uvs, faces, param_coords)` where faces contains vertex index lists for sub-faces.
#[pyfunction]
#[pyo3(signature = (positions, uvs, tex_w, tex_h, pixels_per_face=1.0, max_subdivisions=64))]
pub fn slice_face_by_pixel_grid(
    py: Python<'_>,
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tex_w: u32,
    tex_h: u32,
    pixels_per_face: f32,
    max_subdivisions: u32,
) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<Vec<u32>>, Vec<[f32; 2]>) {
    let res = py.detach(|| {
        mtk_core::slice_face_by_pixel_grid(
            &positions,
            &uvs,
            tex_w,
            tex_h,
            pixels_per_face,
            max_subdivisions,
        )
    });
    (res.positions, res.uvs, res.faces, res.param_coords)
}

/// Performs adaptive pixel grid subdivision on a `PyMeshData` buffer.
#[pyfunction]
#[pyo3(signature = (mesh, face_resolutions=None, default_resolution=(16, 16), pixels_per_face=1.0, max_subdivisions=64, weld_dist=1e-4))]
pub fn adaptive_pixel_split_mesh(
    py: Python<'_>,
    mesh: &PyMeshData,
    face_resolutions: Option<Vec<Option<(u32, u32)>>>,
    default_resolution: (u32, u32),
    pixels_per_face: f32,
    max_subdivisions: u32,
    weld_dist: f32,
) -> PyMeshData {
    let empty_vec = Vec::new();
    let res_slice = face_resolutions.as_deref().unwrap_or(&empty_vec);
    let mut profiler = mtk_core::ExecutionProfiler::start();
    let output_mesh = py.detach(|| {
        let face_count = mesh.inner.indices.len() / 6;
        let mut split =
            profiler.measure_with_count("split_faces_to_pixel_grid", face_count, || {
                mtk_core::adaptive_pixel_split_mesh(
                    &mesh.inner,
                    res_slice,
                    default_resolution,
                    pixels_per_face,
                    max_subdivisions,
                    0.0,
                )
            });

        if weld_dist > 0.0 {
            profiler.measure_with_count("weld_subdivided_vertices", split.positions.len(), || {
                split.uv_layout = mtk_core::UvLayout::PerLoop;
                split.weld_vertices(weld_dist);
            });
        }

        split
    });
    PyMeshData::from_mesh(output_mesh).with_profile(profiler.finish())
}
