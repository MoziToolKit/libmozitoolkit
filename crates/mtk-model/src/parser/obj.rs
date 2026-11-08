use glam::{Vec2, Vec3};
use mtk_core::Direction;
use mtk_core::MeshData;
use serde::{Deserialize, Serialize};

/// A normalized and baked polygon face from an OBJ model ready for mesh construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedObjFace {
    pub direction: Direction,
    pub texture: String,
    pub tint_index: i16,
    pub vertices: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub normal: Vec3,
}

/// Formats a `MeshData` buffer as a standard Wavefront OBJ string.
pub fn mesh_to_obj_string(mesh: &MeshData, object_name: &str, material_names: &[String]) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(out, "# Exported by libmozitoolkit mtk-model");
    let _ = writeln!(out, "o {}", object_name);

    for pos in &mesh.positions {
        let _ = writeln!(out, "v {:.6} {:.6} {:.6}", pos[0], pos[1], pos[2]);
    }
    for uv in &mesh.uvs {
        let _ = writeln!(out, "vt {:.6} {:.6}", uv[0], uv[1]);
    }
    for norm in &mesh.normals {
        let _ = writeln!(out, "vn {:.6} {:.6} {:.6}", norm[0], norm[1], norm[2]);
    }

    let mut curr_mat: Option<u16> = None;
    let tri_count = mesh.triangle_count();

    for tri_idx in 0..tri_count {
        let face_idx = tri_idx / 2;
        let mat_slot = mesh.face_materials.get(face_idx).copied();
        if mat_slot != curr_mat {
            curr_mat = mat_slot;
            if let Some(slot) = mat_slot {
                let mat_name = material_names
                    .get(slot as usize)
                    .map(|s| s.as_str())
                    .unwrap_or("default");
                let _ = writeln!(out, "usemtl {}", mat_name);
            }
        }

        let i0 = mesh.indices[tri_idx * 3] + 1;
        let i1 = mesh.indices[tri_idx * 3 + 1] + 1;
        let i2 = mesh.indices[tri_idx * 3 + 2] + 1;

        let _ = writeln!(
            out,
            "f {}/{}/{} {}/{}/{} {}/{}/{}",
            i0, i0, i0, i1, i1, i1, i2, i2, i2
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_to_obj_string_empty() {
        let mesh = MeshData::new();
        let obj = mesh_to_obj_string(&mesh, "empty", &[]);
        assert!(obj.contains("o empty"));
    }
}
