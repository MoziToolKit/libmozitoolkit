//! # Conversion from FlatPolygonMesh to MeshData
//!
//! Reconstructs a full `MeshData` buffer from a shared-vertex polygon mesh with per-loop UVs.

use super::{MeshData, UvLayout};
use crate::polygon::FlatPolygonMesh;

impl MeshData {
    /// Builds a `MeshData` from a shared-vertex polygon mesh with per-loop UVs.
    ///
    /// All-quad and all-triangle inputs retain their exact topology. Mixed or n-gon
    /// inputs are fan-triangulated while preserving per-loop UV seams.
    pub fn from_flat_polygons(
        flat: &FlatPolygonMesh,
        normals: Option<&[[f32; 3]]>,
    ) -> Result<Self, String> {
        let vertex_count = flat.positions.len();
        let face_count = flat.face_loop_starts.len();
        if flat.face_loop_totals.len() != face_count {
            return Err("face_loop_starts and face_loop_totals must have equal length".to_string());
        }
        if flat.loop_vertices.len() != flat.loop_uvs.len() {
            return Err("loop_vertices and loop_uvs must have equal length".to_string());
        }
        if !flat.face_materials.is_empty() && flat.face_materials.len() != face_count {
            return Err(format!(
                "face_materials length ({}) must match face count ({})",
                flat.face_materials.len(),
                face_count
            ));
        }
        for &idx in &flat.loop_vertices {
            if idx as usize >= vertex_count {
                return Err(format!(
                    "loop vertex index {} out of bounds for vertex count {}",
                    idx, vertex_count
                ));
            }
        }
        for (face, (&start, &total)) in flat
            .face_loop_starts
            .iter()
            .zip(flat.face_loop_totals.iter())
            .enumerate()
        {
            if total < 3 {
                return Err(format!("face {} has fewer than 3 loops", face));
            }
            let end = start as usize + total as usize;
            if end > flat.loop_vertices.len() {
                return Err(format!("face {} loop range exceeds loop buffer", face));
            }
        }

        let mut mesh = Self::with_capacity(vertex_count, 0, face_count);
        mesh.positions = flat.positions.clone();
        mesh.normals = match normals {
            Some(values) if values.len() == vertex_count => values.to_vec(),
            _ => vec![[0.0, 1.0, 0.0]; vertex_count],
        };
        mesh.uv_layout = UvLayout::PerLoop;

        let face_materials: Vec<u16> = if flat.face_materials.is_empty() {
            vec![0; face_count]
        } else {
            flat.face_materials
                .iter()
                .map(|&slot| {
                    u16::try_from(slot).map_err(|_| {
                        format!("face material slot {slot} exceeds u16 material limit")
                    })
                })
                .collect::<Result<_, _>>()?
        };

        let all_quads = face_count > 0 && flat.face_loop_totals.iter().all(|&total| total == 4);
        let all_tris = face_count > 0 && flat.face_loop_totals.iter().all(|&total| total == 3);

        if all_quads {
            mesh.uvs = flat.loop_uvs.clone();
            mesh.quad_indices = Some(flat.loop_vertices.clone());
            mesh.face_materials = face_materials.clone();
            mesh.indices.reserve(face_count * 6);
            for face in 0..face_count {
                let base = face * 4;
                let v0 = flat.loop_vertices[base];
                let v1 = flat.loop_vertices[base + 1];
                let v2 = flat.loop_vertices[base + 2];
                let v3 = flat.loop_vertices[base + 3];
                mesh.indices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
            }
        } else if all_tris {
            mesh.uvs = flat.loop_uvs.clone();
            mesh.indices = flat.loop_vertices.clone();
            mesh.face_materials = face_materials.clone();
        } else {
            for (face, (&start, &total)) in flat
                .face_loop_starts
                .iter()
                .zip(flat.face_loop_totals.iter())
                .enumerate()
            {
                let start = start as usize;
                let total = total as usize;
                let mat = face_materials.get(face).copied().unwrap_or(0);
                let v0 = flat.loop_vertices[start];
                let uv0 = flat.loop_uvs[start];
                for i in 1..total - 1 {
                    mesh.indices.extend_from_slice(&[
                        v0,
                        flat.loop_vertices[start + i],
                        flat.loop_vertices[start + i + 1],
                    ]);
                    mesh.uvs.extend_from_slice(&[
                        uv0,
                        flat.loop_uvs[start + i],
                        flat.loop_uvs[start + i + 1],
                    ]);
                    mesh.face_materials.push(mat);
                }
            }
        }

        mesh.face_tint_indices = vec![-1; mesh.face_materials.len()];
        Ok(mesh)
    }
}
