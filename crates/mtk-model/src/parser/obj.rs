use glam::{Vec2, Vec3};
use mtk_core::Direction;
use mtk_core::MeshData;

use crate::baker::math::rotate_point;

/// A raw polygon face parsed from a Wavefront OBJ.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObjRawFace {
    pub verts: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub normals: Vec<Vec3>,
    pub material: String,
    pub tint_index: i16,
    pub object_name: String,
}

/// Lightweight and fast Wavefront OBJ parser supporting Minecraft/Forge/MiEx extensions.
pub(crate) struct WavefrontObjParser;

impl WavefrontObjParser {
    /// Parses an OBJ text string into a list of faces.
    ///
    /// If `object_filter` is provided, only faces matching one of the specified object/group names are retained.
    pub fn parse_str(text: &str, object_filter: Option<&[&str]>) -> Vec<ObjRawFace> {
        let mut verts: Vec<Vec3> = Vec::new();
        let mut uvs: Vec<Vec2> = Vec::new();
        let mut normals: Vec<Vec3> = Vec::new();

        let mut curr_obj = "default".to_string();
        let mut curr_mtl = String::new();
        let mut curr_tint: i16 = -1;

        let mut faces = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut parts = line.split_whitespace();
            let tag = match parts.next() {
                Some(t) => t,
                None => continue,
            };

            match tag {
                "o" | "g" => {
                    if let Some(name) = parts.next() {
                        curr_obj = name.to_string();
                    }
                }
                "v" => {
                    let x = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let y = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let z = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    verts.push(Vec3::new(x, y, z));
                }
                "vt" => {
                    let u = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let v = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    uvs.push(Vec2::new(u, v));
                }
                "vn" => {
                    let nx = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let ny = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let nz = parts
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    normals.push(Vec3::new(nx, ny, nz));
                }
                "usemtl" => {
                    let raw_mtl = parts.next().unwrap_or("");
                    curr_tint = -1;
                    let mtl = if raw_mtl.starts_with('[') {
                        if let Some(end_idx) = raw_mtl.find(']') {
                            let tint_str = &raw_mtl[1..end_idx];
                            if let Ok(tint) = tint_str.parse::<i16>() {
                                curr_tint = tint;
                            }
                            &raw_mtl[end_idx + 1..]
                        } else {
                            raw_mtl
                        }
                    } else {
                        raw_mtl
                    };
                    curr_mtl = mtl.to_string();
                }
                "f" => {
                    if let Some(filter) = object_filter {
                        if !filter.iter().any(|&f| f == curr_obj) {
                            continue;
                        }
                    }

                    let mut f_verts = Vec::new();
                    let mut f_uvs = Vec::new();
                    let mut f_norms = Vec::new();

                    let num_verts = verts.len();
                    let num_uvs = uvs.len();
                    let num_norms = normals.len();

                    for tok in parts {
                        let mut sub = tok.split('/');
                        // Vertex index
                        if let Some(vi_str) = sub.next() {
                            if let Ok(raw_vi) = vi_str.parse::<i32>() {
                                let vi = if raw_vi > 0 {
                                    (raw_vi - 1) as usize
                                } else {
                                    (num_verts as i32 + raw_vi) as usize
                                };
                                if vi < num_verts {
                                    f_verts.push(verts[vi]);
                                }
                            }
                        }

                        // Texture coordinate index
                        let mut u_val = Vec2::ZERO;
                        if let Some(ui_str) = sub.next() {
                            if !ui_str.is_empty() {
                                if let Ok(raw_ui) = ui_str.parse::<i32>() {
                                    let ui = if raw_ui > 0 {
                                        (raw_ui - 1) as usize
                                    } else {
                                        (num_uvs as i32 + raw_ui) as usize
                                    };
                                    if ui < num_uvs {
                                        u_val = uvs[ui];
                                    }
                                }
                            }
                        }
                        f_uvs.push(u_val);

                        // Normal index
                        if let Some(ni_str) = sub.next() {
                            if !ni_str.is_empty() {
                                if let Ok(raw_ni) = ni_str.parse::<i32>() {
                                    let ni = if raw_ni > 0 {
                                        (raw_ni - 1) as usize
                                    } else {
                                        (num_norms as i32 + raw_ni) as usize
                                    };
                                    if ni < num_norms {
                                        f_norms.push(normals[ni]);
                                    }
                                }
                            }
                        }
                    }

                    if f_verts.len() < 3 {
                        continue;
                    }

                    // Convex fan triangulation for n-gons (> 4 verts) or direct triangle / quad
                    if f_verts.len() == 3 || f_verts.len() == 4 {
                        faces.push(ObjRawFace {
                            verts: f_verts,
                            uvs: f_uvs,
                            normals: f_norms,
                            material: curr_mtl.clone(),
                            tint_index: curr_tint,
                            object_name: curr_obj.clone(),
                        });
                    } else {
                        for i in 1..f_verts.len() - 1 {
                            let tri_v = vec![f_verts[0], f_verts[i], f_verts[i + 1]];
                            let tri_u = vec![f_uvs[0], f_uvs[i], f_uvs[i + 1]];
                            let tri_n = if f_norms.len() == f_verts.len() {
                                vec![f_norms[0], f_norms[i], f_norms[i + 1]]
                            } else {
                                Vec::new()
                            };
                            faces.push(ObjRawFace {
                                verts: tri_v,
                                uvs: tri_u,
                                normals: tri_n,
                                material: curr_mtl.clone(),
                                tint_index: curr_tint,
                                object_name: curr_obj.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        faces
    }
}

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

/// Generic loader and normalizer for OBJ models in Minecraft modding (Create, Immersive Engineering, etc.).
pub(crate) struct ModObjLoader;

impl ModObjLoader {
    /// Loads, scales, centers, and classifies OBJ faces into standardized Minecraft block geometry.
    pub fn process_raw_faces(
        raw_faces: &[ObjRawFace],
        texture_mappings: &std::collections::HashMap<String, String>,
        fallback_texture: &str,
        rot_x: f32,
        rot_y: f32,
        rot_z: f32,
        offset: Vec3,
    ) -> Vec<BakedObjFace> {
        if raw_faces.is_empty() {
            return Vec::new();
        }

        // 1. Calculate coordinate extents for auto-scaling
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;

        let mut max_uv = 1.0f32;

        for f in raw_faces {
            for v in &f.verts {
                min_x = min_x.min(v.x);
                max_x = max_x.max(v.x);
                min_y = min_y.min(v.y);
                max_y = max_y.max(v.y);
                min_z = min_z.min(v.z);
                max_z = max_z.max(v.z);
            }
            for u in &f.uvs {
                max_uv = max_uv.max(u.x.abs()).max(u.y.abs());
            }
        }

        let span_x = max_x - min_x;
        let span_y = max_y - min_y;
        let span_z = max_z - min_z;
        let max_span = span_x.max(span_y).max(span_z);

        // Auto-scale: Blockbench 16-pixel units vs normalized [0..1]
        let auto_scale = if max_span > 2.0 { 1.0 / 16.0 } else { 1.0 };

        // Center detection: If vertices are centered [-0.5..0.5] or [-8..8], shift into [0..1]
        let is_centered =
            min_x * auto_scale < -0.1 || min_y * auto_scale < -0.1 || min_z * auto_scale < -0.1;
        let shift = if is_centered {
            Vec3::splat(0.5)
        } else {
            Vec3::ZERO
        };

        let uv_scale = if max_uv > 2.0 { 1.0 / 16.0 } else { 1.0 };

        let mut baked_faces = Vec::with_capacity(raw_faces.len());

        for f in raw_faces {
            let tex = if !f.material.is_empty() {
                let clean = f.material.trim_start_matches('#');
                texture_mappings
                    .get(&f.material)
                    .or_else(|| texture_mappings.get(clean))
                    .cloned()
                    .unwrap_or_else(|| {
                        if f.material.contains(':') {
                            f.material.clone()
                        } else {
                            fallback_texture.to_string()
                        }
                    })
            } else {
                fallback_texture.to_string()
            };

            // Transform vertices into [0..1] Minecraft block space
            let mut transformed_verts = Vec::with_capacity(f.verts.len());
            for v in &f.verts {
                let mut pos = *v * auto_scale + shift + offset;
                if rot_x != 0.0 || rot_y != 0.0 {
                    pos = rotate_point(pos, rot_x, rot_y);
                }
                if rot_z != 0.0 {
                    // Optional Z rotation around block center
                    let center = Vec3::splat(0.5);
                    let diff = pos - center;
                    let rad = rot_z.to_radians();
                    let (s, c) = rad.sin_cos();
                    pos = Vec3::new(diff.x * c - diff.y * s, diff.x * s + diff.y * c, diff.z)
                        + center;
                }
                transformed_verts.push(pos);
            }

            // Transform UVs: Flip V axis (Minecraft top=0, bottom=1 vs OBJ bottom=0, top=1)
            let mut transformed_uvs = Vec::with_capacity(f.uvs.len());
            for u in &f.uvs {
                transformed_uvs.push(Vec2::new(u.x * uv_scale, 1.0 - u.y * uv_scale));
            }

            // Calculate polygon normal
            let normal = if transformed_verts.len() >= 3 {
                let e1 = transformed_verts[1] - transformed_verts[0];
                let e2 = transformed_verts[2] - transformed_verts[0];
                e1.cross(e2).normalize_or_zero()
            } else {
                Vec3::Y
            };

            // Find closest Minecraft 6-direction
            let mut best_dir = Direction::Up;
            let mut best_dot = -999.0f32;
            for d in Direction::ALL {
                let dot = normal.dot(d.normal());
                if dot > best_dot {
                    best_dot = dot;
                    best_dir = d;
                }
            }

            baked_faces.push(BakedObjFace {
                direction: best_dir,
                texture: tex,
                tint_index: f.tint_index,
                vertices: transformed_verts,
                uvs: transformed_uvs,
                normal,
            });
        }

        baked_faces
    }
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
    fn test_parse_obj_cube() {
        let obj_text = r#"
# Simple quad
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.0 1.0
usemtl [0]minecraft:block/stone
f 1/1 2/2 3/3 4/4
"#;

        let faces = WavefrontObjParser::parse_str(obj_text, None);
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].verts.len(), 4);
        assert_eq!(faces[0].material, "minecraft:block/stone");
        assert_eq!(faces[0].tint_index, 0);

        let baked = ModObjLoader::process_raw_faces(
            &faces,
            &std::collections::HashMap::new(),
            "minecraft:block/stone",
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
        );
        assert_eq!(baked.len(), 1);
        assert_eq!(baked[0].direction, Direction::South);
        // V axis flipped: 0 -> 1.0, 1.0 -> 0.0
        assert_eq!(baked[0].uvs[0], Vec2::new(0.0, 1.0));
        assert_eq!(baked[0].uvs[2], Vec2::new(1.0, 0.0));
    }

    #[test]
    fn test_obj_polygon_fan_and_negative_indices() {
        let obj_text = r#"
# 5-vertex polygon and negative indices
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.5 1.5 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.5 1.0
vt 0.0 1.0
usemtl [1]minecraft:block/gold_block
f -5/-5 -4/-4 -3/-3 -2/-2 -1/-1
"#;

        let faces = WavefrontObjParser::parse_str(obj_text, None);
        // 5-vertex polygon fanned into 3 triangles: (0,1,2), (0,2,3), (0,3,4)
        assert_eq!(faces.len(), 3);
        for f in &faces {
            assert_eq!(f.verts.len(), 3);
            assert_eq!(f.material, "minecraft:block/gold_block");
            assert_eq!(f.tint_index, 1);
        }

        let baked = ModObjLoader::process_raw_faces(
            &faces,
            &std::collections::HashMap::new(),
            "minecraft:block/dirt",
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
        );
        assert_eq!(baked.len(), 3);
    }
}
