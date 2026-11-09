use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use mtk_core::{AttributeData, MeshAttribute, MeshData};

/// Copies a single element from src AttributeData to dst AttributeData at index.
pub(crate) fn copy_attribute_element(src: &AttributeData, dst: &mut AttributeData, idx: usize) {
    match (src, dst) {
        (AttributeData::Float(s), AttributeData::Float(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Float2(s), AttributeData::Float2(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Float3(s), AttributeData::Float3(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Float4(s), AttributeData::Float4(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Int8(s), AttributeData::Int8(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Int16(s), AttributeData::Int16(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::Int32(s), AttributeData::Int32(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::UInt8(s), AttributeData::UInt8(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::UInt16(s), AttributeData::UInt16(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::UInt32(s), AttributeData::UInt32(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        (AttributeData::String(s), AttributeData::String(d)) => {
            if let Some(v) = s.get(idx) {
                d.push(v.clone());
            }
        }
        (AttributeData::Bool(s), AttributeData::Bool(d)) => {
            if let Some(&v) = s.get(idx) {
                d.push(v);
            }
        }
        _ => {}
    }
}

/// Builds the output `MeshData` filtering out faces in `faces_to_cull`.
pub(crate) fn rebuild_culled_mesh(
    mesh: &MeshData,
    face_count: usize,
    poly_step: usize,
    is_quad_mesh: bool,
    faces_to_cull: &BTreeSet<usize>,
) -> (MeshData, usize) {
    let mut out = MeshData::new();
    if mesh.secondary_uvs.is_some() {
        out.secondary_uvs = Some(Vec::new());
    }
    if mesh.colors.is_some() {
        out.colors = Some(Vec::new());
    }

    for (name, attr) in &mesh.custom_attributes {
        out.add_custom_attribute(MeshAttribute {
            name: name.clone(),
            domain: attr.domain,
            data: attr.data.empty_with_capacity(0),
        });
    }

    // Copy Mesh-domain attributes directly
    for (name, attr) in &mesh.custom_attributes {
        if attr.domain == mtk_core::AttributeDomain::Mesh {
            if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                out_attr.data = attr.data.clone();
            }
        }
    }

    let mut remaining_faces = 0;
    let mut out_quad_indices = if is_quad_mesh || mesh.quad_indices.is_some() {
        Some(Vec::new())
    } else {
        None
    };

    for face_idx in 0..face_count {
        if faces_to_cull.contains(&face_idx) {
            continue;
        }

        remaining_faces += 1;
        let base = face_idx * poly_step;
        let quad_start_v = out.positions.len() as u32;

        if is_quad_mesh {
            let vert_indices = [
                mesh.indices[base] as usize,
                mesh.indices[base + 1] as usize,
                mesh.indices[base + 2] as usize,
                mesh.indices[base + 5] as usize,
            ];

            for (step, &orig_idx) in vert_indices.iter().enumerate() {
                let new_v_idx = out.positions.len() as u32;

                out.positions.push(mesh.positions[orig_idx]);
                out.normals.push(
                    mesh.normals
                        .get(orig_idx)
                        .copied()
                        .unwrap_or([0.0, 0.0, 1.0]),
                );

                let uv = match mesh.uv_layout {
                    mtk_core::UvLayout::PerLoop => {
                        if mesh.uvs.len() == face_count * 4 {
                            mesh.uvs.get(face_idx * 4 + step).copied()
                        } else if mesh.uvs.len() == face_count * 6 {
                            let tri_step = if step == 3 { 5 } else { step };
                            mesh.uvs.get(base + tri_step).copied()
                        } else {
                            mesh.uvs.get(orig_idx).copied()
                        }
                    }
                    mtk_core::UvLayout::PerVertex | mtk_core::UvLayout::None => {
                        mesh.uvs.get(orig_idx).copied()
                    }
                };
                out.uvs.push(uv.unwrap_or([0.0, 0.0]));

                if let (Some(sec_in), Some(sec_out)) = (&mesh.secondary_uvs, &mut out.secondary_uvs)
                {
                    sec_out.push(sec_in.get(orig_idx).copied().unwrap_or([0.0, 0.0]));
                }
                if let (Some(col_in), Some(col_out)) = (&mesh.colors, &mut out.colors) {
                    col_out.push(
                        col_in
                            .get(orig_idx)
                            .copied()
                            .unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    );
                }

                // Copy Point and Corner domain attributes
                for (name, in_attr) in &mesh.custom_attributes {
                    if in_attr.domain == mtk_core::AttributeDomain::Point {
                        if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                            copy_attribute_element(&in_attr.data, &mut out_attr.data, orig_idx);
                        }
                    } else if in_attr.domain == mtk_core::AttributeDomain::Corner {
                        if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                            let c_idx = if in_attr.data.len() == face_count * 4 {
                                face_idx * 4 + step
                            } else {
                                base + if step == 3 { 5 } else { step }
                            };
                            copy_attribute_element(&in_attr.data, &mut out_attr.data, c_idx);
                        }
                    }
                }

                let _ = new_v_idx;
            }

            // Quad indices maintenance: 4 corner vertex indices
            if let Some(ref mut q_indices) = out_quad_indices {
                q_indices.push(quad_start_v);
                q_indices.push(quad_start_v + 1);
                q_indices.push(quad_start_v + 2);
                q_indices.push(quad_start_v + 3);
            }

            // Triangle indices for quads: 2 triangles (0, 1, 2) and (0, 2, 3)
            out.indices.extend_from_slice(&[
                quad_start_v,
                quad_start_v + 1,
                quad_start_v + 2,
                quad_start_v,
                quad_start_v + 2,
                quad_start_v + 3,
            ]);
        } else {
            // Triangle mesh (poly_step == 3)
            for step in 0..3 {
                let orig_idx = mesh.indices[base + step] as usize;
                let new_v_idx = out.positions.len() as u32;

                out.positions.push(mesh.positions[orig_idx]);
                out.normals.push(
                    mesh.normals
                        .get(orig_idx)
                        .copied()
                        .unwrap_or([0.0, 0.0, 1.0]),
                );

                let uv = match mesh.uv_layout {
                    mtk_core::UvLayout::PerLoop => mesh.uvs.get(base + step).copied(),
                    mtk_core::UvLayout::PerVertex | mtk_core::UvLayout::None => {
                        mesh.uvs.get(orig_idx).copied()
                    }
                };
                out.uvs.push(uv.unwrap_or([0.0, 0.0]));

                if let (Some(sec_in), Some(sec_out)) = (&mesh.secondary_uvs, &mut out.secondary_uvs)
                {
                    sec_out.push(sec_in.get(orig_idx).copied().unwrap_or([0.0, 0.0]));
                }
                if let (Some(col_in), Some(col_out)) = (&mesh.colors, &mut out.colors) {
                    col_out.push(
                        col_in
                            .get(orig_idx)
                            .copied()
                            .unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    );
                }

                for (name, in_attr) in &mesh.custom_attributes {
                    if in_attr.domain == mtk_core::AttributeDomain::Point {
                        if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                            copy_attribute_element(&in_attr.data, &mut out_attr.data, orig_idx);
                        }
                    } else if in_attr.domain == mtk_core::AttributeDomain::Corner {
                        if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                            copy_attribute_element(&in_attr.data, &mut out_attr.data, base + step);
                        }
                    }
                }

                out.indices.push(new_v_idx);
            }
        }

        if let Some(&mat_id) = mesh.face_materials.get(face_idx) {
            out.face_materials.push(mat_id);
        }
        if let Some(&tint_idx) = mesh.face_tint_indices.get(face_idx) {
            out.face_tint_indices.push(tint_idx);
        }

        // Copy Face domain attributes
        for (name, in_attr) in &mesh.custom_attributes {
            if in_attr.domain == mtk_core::AttributeDomain::Face {
                if let Some(out_attr) = out.custom_attributes.get_mut(name) {
                    copy_attribute_element(&in_attr.data, &mut out_attr.data, face_idx);
                }
            }
        }
    }

    out.uv_layout = mesh.uv_layout;
    out.quad_indices = out_quad_indices;
    (out, remaining_faces)
}
