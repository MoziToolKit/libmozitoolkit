//! Wavefront `.obj` / `.mtl` parser, asset store, and OBJ mesh export utilities.

mod mtl;

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::{Direction, MeshData};
use serde::{Deserialize, Serialize};

pub use mtl::{MtlMaterial, ParsedMtlLibrary};

/// A normalized and baked polygon face from an OBJ model ready for mesh construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedObjFace {
    pub direction: Direction,
    pub texture: String,
    pub tint_index: i16,
    pub vertices: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub normal: Vec3,
    #[serde(default)]
    pub atlas_uvs: Option<Vec<Vec2>>,
    #[serde(default)]
    pub atlas_chunk_id: Option<u16>,
    #[serde(default)]
    pub atlas_texture_id: Option<u32>,
}

impl BakedObjFace {
    /// Remaps this face's local Minecraft UVs to absolute Atlas UV coordinates given frame 0 bounds.
    pub fn remap_to_atlas_bounds(
        &mut self,
        frame_0_uv_bounds: [f32; 4],
        chunk_id: u16,
        texture_id: u32,
    ) {
        let u_min = frame_0_uv_bounds[0];
        let v_min = frame_0_uv_bounds[1];
        let u_span = frame_0_uv_bounds[2] - u_min;
        let v_span = frame_0_uv_bounds[3] - v_min;
        let remapped: Vec<Vec2> = self
            .uvs
            .iter()
            .map(|uv| {
                let u = uv.x.clamp(0.0, 1.0);
                let v = uv.y.clamp(0.0, 1.0);
                Vec2::new(u_min + u * u_span, v_min + (1.0 - v) * v_span)
            })
            .collect();
        self.atlas_uvs = Some(remapped);
        self.atlas_chunk_id = Some(chunk_id);
        self.atlas_texture_id = Some(texture_id);
    }
}

/// Raw unbaked polygon face parsed from a Wavefront `.obj` file.
#[derive(Debug, Clone, PartialEq)]
pub struct RawObjFace {
    pub object_name: Option<String>,
    pub group_names: Vec<String>,
    pub material: Option<String>,
    pub vertices: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub normals: Vec<Option<Vec3>>,
}

/// Parsed Wavefront `.obj` geometry prior to blockstate/model transforms and texture baking.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedObjModel {
    pub mtllibs: Vec<String>,
    pub faces: Vec<RawObjFace>,
}

#[inline]
fn resolve_obj_index(raw: isize, len: usize) -> Option<usize> {
    if raw > 0 {
        let idx = (raw - 1) as usize;
        (idx < len).then_some(idx)
    } else if raw < 0 {
        let offset = (-raw) as usize;
        (offset <= len && offset > 0).then(|| len - offset)
    } else {
        None
    }
}

fn logical_obj_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut buf = String::new();
    for raw in text.lines() {
        let stripped = raw.trim_end_matches('\r');
        if let Some(prefix) = stripped.strip_suffix('\\') {
            buf.push_str(prefix);
            buf.push(' ');
        } else if buf.is_empty() {
            lines.push(stripped.to_string());
        } else {
            buf.push_str(stripped);
            lines.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        lines.push(buf);
    }
    lines
}

impl ParsedObjModel {
    /// Parses a Wavefront `.obj` text payload supporting `v`, `vt`, `vn`, `f`, `mtllib`, `usemtl`,
    /// `o`, `g`, negative relative indices, and `\` line continuations.
    pub fn parse_str(text: &str) -> Self {
        let mut verts: Vec<Vec3> = Vec::new();
        let mut uvs: Vec<Vec2> = Vec::new();
        let mut norms: Vec<Vec3> = Vec::new();
        let mut mtllibs: Vec<String> = Vec::new();
        let mut faces: Vec<RawObjFace> = Vec::new();

        let mut cur_obj: Option<String> = None;
        let mut cur_groups: Vec<String> = Vec::new();
        let mut cur_mtl: Option<String> = None;

        for line in logical_obj_lines(text) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let Some(cmd) = parts.next() else {
                continue;
            };
            match cmd {
                "v" => {
                    let (Some(x), Some(y), Some(z)) = (
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                    ) else {
                        continue;
                    };
                    verts.push(Vec3::new(x, y, z));
                }
                "vt" => {
                    let (Some(u), Some(v)) = (
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                    ) else {
                        continue;
                    };
                    uvs.push(Vec2::new(u, v));
                }
                "vn" => {
                    let (Some(nx), Some(ny), Some(nz)) = (
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                        parts.next().and_then(|s| s.parse::<f32>().ok()),
                    ) else {
                        continue;
                    };
                    norms.push(Vec3::new(nx, ny, nz));
                }
                "mtllib" => {
                    let arg = trimmed["mtllib".len()..].trim();
                    if !arg.is_empty() {
                        mtllibs.push(arg.to_string());
                    }
                }
                "usemtl" => {
                    let arg = trimmed["usemtl".len()..].trim();
                    cur_mtl = (!arg.is_empty()).then(|| arg.to_string());
                }
                "o" => {
                    let arg = trimmed["o".len()..].trim();
                    cur_obj = (!arg.is_empty()).then(|| arg.to_string());
                }
                "g" => {
                    cur_groups = parts.map(str::to_string).collect();
                }
                "f" => {
                    let mut f_verts = Vec::new();
                    let mut f_uvs = Vec::new();
                    let mut f_norms = Vec::new();
                    let mut valid = true;

                    for tok in parts {
                        let mut segs = tok.split('/');
                        let Some(vi_raw) = segs.next().and_then(|s| s.parse::<isize>().ok()) else {
                            valid = false;
                            break;
                        };
                        let Some(vi) = resolve_obj_index(vi_raw, verts.len()) else {
                            valid = false;
                            break;
                        };

                        let uv = segs
                            .next()
                            .filter(|s| !s.is_empty())
                            .and_then(|s| s.parse::<isize>().ok())
                            .and_then(|idx| resolve_obj_index(idx, uvs.len()))
                            .map(|idx| uvs[idx])
                            .unwrap_or(Vec2::ZERO);

                        let normal = segs
                            .next()
                            .filter(|s| !s.is_empty())
                            .and_then(|s| s.parse::<isize>().ok())
                            .and_then(|idx| resolve_obj_index(idx, norms.len()))
                            .map(|idx| norms[idx]);

                        f_verts.push(verts[vi]);
                        f_uvs.push(uv);
                        f_norms.push(normal);
                    }

                    if valid && f_verts.len() >= 3 {
                        faces.push(RawObjFace {
                            object_name: cur_obj.clone(),
                            group_names: cur_groups.clone(),
                            material: cur_mtl.clone(),
                            vertices: f_verts,
                            uvs: f_uvs,
                            normals: f_norms,
                        });
                    }
                }
                _ => {}
            }
        }

        Self { mtllibs, faces }
    }
}

/// Preloaded repository of parsed `.obj` models and `.mtl` material libraries keyed by VFS asset path.
#[derive(Debug, Clone, Default)]
pub struct ObjAssetStore {
    objs: HashMap<String, ParsedObjModel>,
    mtls: HashMap<String, ParsedMtlLibrary>,
}

fn normalize_posix_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

impl ObjAssetStore {
    /// Creates an empty OBJ/MTL asset store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a parsed `.obj` model keyed by canonical VFS asset path (`assets/<ns>/...`).
    pub fn insert_obj(&mut self, asset_path: impl Into<String>, model: ParsedObjModel) {
        self.objs.insert(asset_path.into(), model);
    }

    /// Inserts a parsed `.mtl` library keyed by canonical VFS asset path (`assets/<ns>/...`).
    pub fn insert_mtl(&mut self, asset_path: impl Into<String>, mtl: ParsedMtlLibrary) {
        self.mtls.insert(asset_path.into(), mtl);
    }

    /// Number of stored `.obj` models.
    pub fn obj_count(&self) -> usize {
        self.objs.len()
    }

    /// Number of stored `.mtl` libraries.
    pub fn mtl_count(&self) -> usize {
        self.mtls.len()
    }

    /// Generates candidate VFS asset paths for a model JSON `"model"` reference (e.g.
    /// `"create:models/block/blaze_burner/blaze_cage.obj"` or `"immersiveengineering:models/block/slope.obj.ie"`).
    pub fn model_ref_to_asset_candidates(model_ref: &str, fallback_ns: &str) -> Vec<String> {
        let trimmed = model_ref.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let (ns, raw_path) = match trimmed.split_once(':') {
            Some((n, p)) => (n, p.trim_start_matches('/')),
            None => (fallback_ns, trimmed.trim_start_matches('/')),
        };
        let mut out = Vec::with_capacity(2);
        if raw_path.starts_with("models/") {
            out.push(format!("assets/{ns}/{raw_path}"));
        } else {
            out.push(format!("assets/{ns}/models/{raw_path}"));
            out.push(format!("assets/{ns}/{raw_path}"));
        }
        out
    }

    /// Generates candidate VFS asset paths for an `.mtl` reference from `mtllib` or `mtl_override`.
    pub fn resolve_mtl_candidates(obj_asset_path: &str, mtl_ref: &str) -> Vec<String> {
        let trimmed = mtl_ref.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        if let Some((ns, raw_path)) = trimmed.split_once(':') {
            let p = raw_path.trim_start_matches('/');
            let mut out = vec![format!("assets/{ns}/{p}")];
            if !p.starts_with("models/") {
                out.push(format!("assets/{ns}/models/{p}"));
            }
            return out;
        }
        let dir = obj_asset_path
            .rsplit_once('/')
            .map(|(d, _)| d)
            .unwrap_or("");
        let joined = if dir.is_empty() {
            trimmed.to_string()
        } else {
            format!("{dir}/{trimmed}")
        };
        vec![normalize_posix_path(&joined)]
    }

    /// Looks up a loaded `.obj` model by model reference, returning `(asset_path, &ParsedObjModel)`.
    pub(crate) fn get_obj(
        &self,
        model_ref: &str,
        fallback_ns: &str,
    ) -> Option<(&str, &ParsedObjModel)> {
        for cand in Self::model_ref_to_asset_candidates(model_ref, fallback_ns) {
            if let Some((key, val)) = self.objs.get_key_value(&cand) {
                return Some((key.as_str(), val));
            }
        }
        None
    }

    /// Looks up material metadata by name from `mtl_override` or the `.obj`'s declared `mtllib` entries.
    pub(crate) fn find_material(
        &self,
        obj_asset_path: &str,
        mtl_override: Option<&str>,
        mtllibs: &[String],
        material_name: &str,
    ) -> Option<&MtlMaterial> {
        if let Some(ovr) = mtl_override {
            for cand in Self::resolve_mtl_candidates(obj_asset_path, ovr) {
                if let Some(mat) = self
                    .mtls
                    .get(&cand)
                    .and_then(|m| m.materials.get(material_name))
                {
                    return Some(mat);
                }
            }
        }
        for lib in mtllibs {
            for cand in Self::resolve_mtl_candidates(obj_asset_path, lib) {
                if let Some(mat) = self
                    .mtls
                    .get(&cand)
                    .and_then(|m| m.materials.get(material_name))
                {
                    return Some(mat);
                }
            }
        }
        None
    }

    /// Validates whether a texture reference string from `.mtl` is a usable Minecraft identifier.
    #[inline]
    pub(crate) fn is_valid_mc_texture_ref(tex: &str) -> bool {
        mtl::is_valid_mc_texture_ref(tex)
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
    fn test_mesh_to_obj_string_empty() {
        let mesh = MeshData::new();
        let obj = mesh_to_obj_string(&mesh, "empty", &[]);
        assert!(obj.contains("o empty"));
    }

    #[test]
    fn test_parse_obj_indices_groups_and_continuations() {
        let obj_text = r#"
# Comment
mtllib sample.mtl
o Body
g frame left_wing
usemtl mat_a
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 \
  0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.0 1.0
vn 0.0 0.0 1.0
f 1/1/1 2/2/1 3/3/1 4/4/1
f -4/-4 -3/-3 -2/-2
f 1//1 2//1 3//1
"#;
        let parsed = ParsedObjModel::parse_str(obj_text);
        assert_eq!(parsed.mtllibs, vec!["sample.mtl"]);
        assert_eq!(parsed.faces.len(), 3);
        assert_eq!(parsed.faces[0].object_name.as_deref(), Some("Body"));
        assert_eq!(parsed.faces[0].group_names, vec!["frame", "left_wing"]);
        assert_eq!(parsed.faces[0].material.as_deref(), Some("mat_a"));
        assert_eq!(parsed.faces[0].vertices.len(), 4);
        assert_eq!(parsed.faces[0].vertices[2], Vec3::new(1.0, 1.0, 0.0));
        // Negative indices resolve identically to vertices 0, 1, 2
        assert_eq!(parsed.faces[1].vertices[0], Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(parsed.faces[1].vertices[2], Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(parsed.faces[1].normals[0], None);
        // v//vn keeps UV at ZERO and normal at +Z
        assert_eq!(parsed.faces[2].uvs[0], Vec2::ZERO);
        assert_eq!(parsed.faces[2].normals[0], Some(Vec3::Z));
    }

    #[test]
    fn test_parse_mtl_directives_and_tint_index() {
        let mtl_text = r#"
# Material file
newmtl metal
map_Kd #texture0
forge_TintIndex 1

newmtl direct
map_Kd create:block/axis

newmtl broken_local
map_Kd P:\IDEA Projects\Minecraft\bad.png
"#;
        let lib = ParsedMtlLibrary::parse_str(mtl_text);
        let metal = lib.materials.get("metal").unwrap();
        assert_eq!(metal.map_kd.as_deref(), Some("#texture0"));
        assert_eq!(metal.tint_index, 1);

        let direct = lib.materials.get("direct").unwrap();
        assert_eq!(direct.map_kd.as_deref(), Some("create:block/axis"));
        assert_eq!(direct.tint_index, -1);

        let direct_refs: Vec<&str> = lib.iter_direct_texture_refs().collect();
        assert_eq!(direct_refs, vec!["create:block/axis"]);
    }
}
