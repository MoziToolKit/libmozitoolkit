use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;
use serde::{Deserialize, Serialize};

use crate::parser::obj::BakedObjFace;

/// Represents a baked quad face with calculated geometry, UVs, and texture bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedFace {
    pub direction: Direction,
    pub texture: String,
    pub uv_rot: f32,
    pub uv_bounds: [f32; 4],
    pub tint_index: i16,
    pub cullface: Option<Direction>,
    pub vertices: [Vec3; 4],
    pub uvs: [Vec2; 4],
    pub normal: Vec3,
    #[serde(default)]
    pub atlas_uvs: Option<[Vec2; 4]>,
    #[serde(default)]
    pub atlas_chunk_id: Option<u16>,
    #[serde(default)]
    pub atlas_texture_id: Option<u32>,
}

impl BakedFace {
    /// Remaps this face's local UVs to absolute Atlas UV coordinates given frame 0 UV bounds.
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
        self.atlas_uvs = Some([
            Vec2::new(
                u_min + self.uvs[0].x * u_span,
                v_min + (1.0 - self.uvs[0].y) * v_span,
            ),
            Vec2::new(
                u_min + self.uvs[1].x * u_span,
                v_min + (1.0 - self.uvs[1].y) * v_span,
            ),
            Vec2::new(
                u_min + self.uvs[2].x * u_span,
                v_min + (1.0 - self.uvs[2].y) * v_span,
            ),
            Vec2::new(
                u_min + self.uvs[3].x * u_span,
                v_min + (1.0 - self.uvs[3].y) * v_span,
            ),
        ]);
        self.atlas_chunk_id = Some(chunk_id);
        self.atlas_texture_id = Some(texture_id);
    }
}

impl Default for BakedFace {
    fn default() -> Self {
        Self {
            direction: Direction::Up,
            texture: String::new(),
            uv_rot: 0.0,
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            tint_index: -1,
            cullface: None,
            vertices: [Vec3::ZERO; 4],
            uvs: [Vec2::ZERO; 4],
            normal: Vec3::Y,
            atlas_uvs: None,
            atlas_chunk_id: None,
            atlas_texture_id: None,
        }
    }
}

/// Represents a 3D box element within a baked model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedElement {
    pub from_pos: [f32; 3],
    pub to_pos: [f32; 3],
    pub faces: HashMap<Direction, BakedFace>,
}

impl BakedElement {
    /// Returns true if this element has inverted coordinates (`from > to` in any axis),
    /// which vanilla Minecraft uses to create inward-facing interior geometry (e.g. spawners, vaults).
    pub fn is_inverted(&self) -> bool {
        self.from_pos[0] > self.to_pos[0] + 1e-4
            || self.from_pos[1] > self.to_pos[1] + 1e-4
            || self.from_pos[2] > self.to_pos[2] + 1e-4
    }
}

/// Options for configuring mesh generation and face de-overlapping from baked models.
#[derive(Debug, Clone)]
pub struct ModelMeshOptions {
    /// Whether to clip faces occluded by interior element geometry via 2D boolean difference.
    pub clip_hidden_volume: bool,
    /// Whether to cull duplicate overlapping faces with the same normal (eliminating Z-fighting).
    pub cull_duplicates: bool,
    /// Whether to cull contacting interior faces with opposite normals.
    pub cull_coplanar_opposite: bool,
    /// Numerical tolerance for geometric equality.
    pub tolerance: f32,
}

impl Default for ModelMeshOptions {
    fn default() -> Self {
        Self {
            clip_hidden_volume: true,
            cull_duplicates: true,
            cull_coplanar_opposite: true,
            tolerance: 1e-3,
        }
    }
}

/// Fully baked model containing all elements, directional faces summary, and metadata.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct BakedModel {
    pub block_state: String,
    pub elements: Vec<BakedElement>,
    #[serde(default)]
    pub obj_faces: Vec<BakedObjFace>,
    pub faces: [BakedFace; 6],
    pub is_cube: bool,
    pub is_opaque: bool,
    pub is_emissive: bool,
    #[serde(default)]
    pub cull_meta: Option<mtk_cull::BlockCullMeta>,
    #[serde(skip)]
    pub culled_faces: [Vec<BakedFace>; 6],
    #[serde(skip)]
    pub unculled_faces: Vec<BakedFace>,
}

impl<'de> Deserialize<'de> for BakedModel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct BakedModelHelper {
            block_state: String,
            elements: Vec<BakedElement>,
            #[serde(default)]
            obj_faces: Vec<BakedObjFace>,
            faces: [BakedFace; 6],
            is_cube: bool,
            is_opaque: bool,
            is_emissive: bool,
            #[serde(default)]
            cull_meta: Option<mtk_cull::BlockCullMeta>,
        }

        let helper = BakedModelHelper::deserialize(deserializer)?;
        let mut model = BakedModel {
            block_state: helper.block_state,
            elements: helper.elements,
            obj_faces: helper.obj_faces,
            faces: helper.faces,
            is_cube: helper.is_cube,
            is_opaque: helper.is_opaque,
            is_emissive: helper.is_emissive,
            cull_meta: helper.cull_meta,
            culled_faces: Default::default(),
            unculled_faces: Default::default(),
        };
        model.rebuild_face_buckets();
        Ok(model)
    }
}

impl BakedModel {
    /// Rebuilds `culled_faces` (indexed by cull direction) and `unculled_faces` from `elements`.
    /// Automatically filters out standalone overlay decal faces (`_overlay`, `side_overlay`).
    pub fn rebuild_face_buckets(&mut self) {
        let mut culled: [Vec<BakedFace>; 6] = Default::default();
        let mut unculled = Vec::new();

        for el in &self.elements {
            for dir in mtk_core::Direction::ALL {
                if let Some(face) = el.faces.get(&dir) {
                    if face.texture.ends_with("_overlay")
                        || face.texture.ends_with("_OVERLAY")
                        || face.texture.contains("side_overlay")
                    {
                        continue;
                    }
                    if let Some(cull_dir) = face.cullface {
                        culled[cull_dir.to_index()].push(face.clone());
                    } else {
                        unculled.push(face.clone());
                    }
                }
            }
        }
        self.culled_faces = culled;
        self.unculled_faces = unculled;
    }
}

impl BakedModel {
    ///
    /// Automatically rebuilds `culled_faces` and `unculled_faces` buckets.
    /// Returns the number of removed faces.
    pub fn deduplicate_faces(&mut self) -> usize {
        if self.elements.len() <= 1
            && self.elements.first().map(|e| e.faces.len()).unwrap_or(0) <= 6
        {
            return 0;
        }

        let mut face_list = Vec::new();
        for (el_idx, el) in self.elements.iter().enumerate() {
            let is_plane = (el.from_pos[0] - el.to_pos[0]).abs() < 1e-4
                || (el.from_pos[1] - el.to_pos[1]).abs() < 1e-4
                || (el.from_pos[2] - el.to_pos[2]).abs() < 1e-4;
            let is_inverted = el.is_inverted();
            for (&dir, f) in &el.faces {
                face_list.push((
                    (el_idx, dir, is_plane, is_inverted),
                    f.vertices,
                    f.normal,
                    &f.texture,
                    f.tint_index,
                ));
            }
        }

        let mut to_remove = std::collections::HashSet::new();
        let tolerance = 1e-3;

        for i in 0..face_list.len() {
            if to_remove.contains(&(face_list[i].0 .0, face_list[i].0 .1)) {
                continue;
            }
            let ((el_a, dir_a, is_plane_a, is_inverted_a), verts_a, norm_a, tex_a, tint_a) =
                &face_list[i];

            for j in (i + 1)..face_list.len() {
                let ((el_b, dir_b, is_plane_b, is_inverted_b), verts_b, norm_b, tex_b, tint_b) =
                    &face_list[j];
                if el_a == el_b || to_remove.contains(&(*el_b, *dir_b)) {
                    continue;
                }

                if let Some(rel) = mtk_cull::geometry::coplanar::check_coplanar_overlap(
                    verts_a, *norm_a, verts_b, *norm_b, tolerance,
                ) {
                    use mtk_cull::geometry::coplanar::{CoplanarOverlap, FaceAlignment};
                    match rel.overlap {
                        CoplanarOverlap::Exact => {
                            if rel.alignment == FaceAlignment::SameDirection {
                                // Exact duplicate face: only remove B if identical texture and tint
                                if tex_a == tex_b && tint_a == tint_b {
                                    to_remove.insert((*el_b, *dir_b));
                                } else if is_emissive_overlay_of(tex_b, tex_a) {
                                    // Face B is an emissive companion overlay (e.g. firefly_bush_emissive over firefly_bush)
                                    // Remove redundant coplanar face B to eliminate Z-fighting and shadow deadlock
                                    to_remove.insert((*el_b, *dir_b));
                                } else if is_emissive_overlay_of(tex_a, tex_b) {
                                    // Face A is an emissive companion overlay over base face B
                                    to_remove.insert((*el_a, *dir_a));
                                    break;
                                }
                            } else if !*is_plane_a
                                && !*is_plane_b
                                && !*is_inverted_a
                                && !*is_inverted_b
                            {
                                // Solid cuboid back-to-back contacting faces: remove both
                                to_remove.insert((*el_a, *dir_a));
                                to_remove.insert((*el_b, *dir_b));
                                break;
                            }
                        }
                        CoplanarOverlap::ContainedInA => {
                            // Only solid cuboid back-to-back contacting faces: Face B is completely covered by Cuboid A
                            if rel.alignment == FaceAlignment::OppositeDirection
                                && !*is_plane_a
                                && !*is_plane_b
                                && !*is_inverted_a
                                && !*is_inverted_b
                            {
                                to_remove.insert((*el_b, *dir_b));
                            }
                        }
                        CoplanarOverlap::ContainedInB
                            // Only solid cuboid back-to-back contacting faces: Face A is completely covered by Cuboid B
                            if rel.alignment == FaceAlignment::OppositeDirection
                                && !*is_plane_a
                                && !*is_plane_b
                                && !*is_inverted_a
                                && !*is_inverted_b
                            => {
                                to_remove.insert((*el_a, *dir_a));
                                break;
                            }
                        _ => {}
                    }
                }
            }
        }

        let culled_count = to_remove.len();
        if culled_count > 0 {
            for (el_idx, dir) in to_remove {
                if let Some(el) = self.elements.get_mut(el_idx) {
                    el.faces.remove(&dir);
                }
            }
        }
        self.rebuild_face_buckets();

        culled_count
    }

    /// Returns references to face buckets. If `culled_faces` and `unculled_faces` are empty
    /// but `elements` is non-empty (e.g. manually constructed in tests), computes them on the fly.
    pub fn get_face_buckets(&self) -> ([Vec<BakedFace>; 6], Vec<BakedFace>) {
        if self.culled_faces.iter().any(|v| !v.is_empty()) || !self.unculled_faces.is_empty() {
            return (self.culled_faces.clone(), self.unculled_faces.clone());
        }
        let mut culled: [Vec<BakedFace>; 6] = Default::default();
        let mut unculled = Vec::new();
        for el in &self.elements {
            for dir in mtk_core::Direction::ALL {
                if let Some(face) = el.faces.get(&dir) {
                    if face.texture.ends_with("_overlay")
                        || face.texture.ends_with("_OVERLAY")
                        || face.texture.contains("side_overlay")
                    {
                        continue;
                    }
                    if let Some(cull_dir) = face.cullface {
                        culled[cull_dir.to_index()].push(face.clone());
                    } else {
                        unculled.push(face.clone());
                    }
                }
            }
        }
        (culled, unculled)
    }

    /// Returns pre-baked culling metadata, or dynamically computes it if missing.
    pub fn get_or_compute_cull_meta(&self) -> mtk_cull::BlockCullMeta {
        if let Some(ref meta) = self.cull_meta {
            return meta.clone();
        }
        let mut quads: Vec<([Vec3; 4], Direction)> = Vec::new();
        if !self.elements.is_empty() {
            for elem in &self.elements {
                for (&dir, face) in &elem.faces {
                    quads.push((face.vertices, dir));
                }
            }
        } else if self.is_cube {
            for dir in Direction::ALL {
                quads.push((self.faces[dir.to_index()].vertices, dir));
            }
        }
        let quads_slice = if quads.is_empty() {
            None
        } else {
            Some(quads.as_slice())
        };
        mtk_cull::compute_block_cull_meta(&self.block_state, quads_slice, Some(self.is_opaque))
    }

    /// Returns the standard face for a given direction (from 6-face summary).
    pub fn get_face(&self, dir: Direction) -> &BakedFace {
        &self.faces[dir.to_index()]
    }

    /// Remaps all elements, faces, and OBJ faces in this model to atlas coordinates using a lookup closure.
    pub fn remap_to_atlas_with<F>(&mut self, mut lookup_fn: F)
    where
        F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
    {
        for elem in &mut self.elements {
            for face in elem.faces.values_mut() {
                if let Some((bounds, chunk_id, tex_id)) = lookup_fn(&face.texture) {
                    face.remap_to_atlas_bounds(bounds, chunk_id, tex_id);
                }
            }
        }
        for bucket in &mut self.culled_faces {
            for face in bucket {
                if let Some((bounds, chunk_id, tex_id)) = lookup_fn(&face.texture) {
                    face.remap_to_atlas_bounds(bounds, chunk_id, tex_id);
                }
            }
        }
        for face in &mut self.unculled_faces {
            if let Some((bounds, chunk_id, tex_id)) = lookup_fn(&face.texture) {
                face.remap_to_atlas_bounds(bounds, chunk_id, tex_id);
            }
        }
        for face in &mut self.faces {
            if let Some((bounds, chunk_id, tex_id)) = lookup_fn(&face.texture) {
                face.remap_to_atlas_bounds(bounds, chunk_id, tex_id);
            }
        }
        for obj_face in &mut self.obj_faces {
            if let Some((bounds, _chunk_id, _tex_id)) = lookup_fn(&obj_face.texture) {
                let u_min = bounds[0];
                let v_min = bounds[1];
                let u_span = bounds[2] - u_min;
                let v_span = bounds[3] - v_min;
                for uv in &mut obj_face.uvs {
                    uv.x = u_min + uv.x * u_span;
                    uv.y = v_min + (1.0 - uv.y) * v_span;
                }
            }
        }
    }
}

/// Helper detecting if `emissive_tex` is an emissive companion overlay of `base_tex`.
#[inline]
fn is_emissive_overlay_of(emissive_tex: &str, base_tex: &str) -> bool {
    if let Some(stripped) = emissive_tex.strip_suffix("_emissive") {
        stripped == base_tex
    } else if let Some(stripped) = emissive_tex.strip_suffix("_e") {
        stripped == base_tex
    } else {
        emissive_tex.contains("cross_emissive") && !base_tex.contains("cross_emissive")
    }
}
