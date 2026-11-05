//! # `mtk-py` Extrusion & Side UV Repair Bindings
//!
//! Exposes batch discrete face extrusion, UV reconstruction, and random noise displacement to Python.

use pyo3::prelude::*;

/// Performs high-performance batch Data In, Data Out UV repair across an entire mesh
/// using flat contiguous 1D buffers for zero-copy memory ingestion.
#[pyfunction]
#[pyo3(signature = (
    positions,
    loop_vertices,
    loop_uvs,
    face_loop_starts,
    face_loop_totals,
    face_materials,
    selected_faces,
    pixel_steps,
    uv_mode="SMART",
    repair_uv=true,
    add_crease=false,
    crease_val=1.0,
    only_collapsed=false,
    smart_side_faces=None
))]
pub fn process_flat_mesh_extrude_repair(
    py: Python<'_>,
    positions: Vec<f32>,
    loop_vertices: Vec<u32>,
    loop_uvs: Vec<f32>,
    face_loop_starts: Vec<u32>,
    face_loop_totals: Vec<u32>,
    face_materials: Vec<u32>,
    selected_faces: Vec<u32>,
    pixel_steps: Vec<[f32; 2]>,
    uv_mode: &str,
    repair_uv: bool,
    add_crease: bool,
    crease_val: f32,
    only_collapsed: bool,
    smart_side_faces: Option<Vec<u32>>,
) -> (
    Vec<(u32, Vec<[f32; 2]>)>,
    Vec<(u32, u32)>,
    Vec<((u32, u32), f32)>,
    usize,
) {
    let mode = match uv_mode.to_uppercase().as_str() {
        "INWARD" => mtk_core::ExtrudeUvMode::Inward,
        "OUTWARD" => mtk_core::ExtrudeUvMode::Outward,
        _ => mtk_core::ExtrudeUvMode::Smart,
    };

    let flat_mesh = mtk_core::FlatPolygonMesh::from_flat_buffers(
        &positions,
        loop_vertices,
        &loop_uvs,
        face_loop_starts,
        face_loop_totals,
        face_materials,
    );

    let config = mtk_core::MeshExtrudeRepairConfig {
        uv_mode: mode,
        repair_uv,
        add_crease,
        crease_val,
        only_collapsed,
    };

    let out = py.detach(|| {
        mtk_core::process_flat_mesh_extrude_repair(
            &flat_mesh,
            &selected_faces,
            &pixel_steps,
            &config,
            smart_side_faces.as_deref(),
        )
    });

    (
        out.modified_face_uvs,
        out.modified_face_materials,
        out.modified_edge_creases,
        out.repaired_count,
    )
}
