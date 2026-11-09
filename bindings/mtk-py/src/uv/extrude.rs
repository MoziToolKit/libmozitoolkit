use glam::Vec2;
use pyo3::prelude::*;

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
