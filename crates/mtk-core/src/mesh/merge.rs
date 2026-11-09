//! # Mesh Merging Algorithms
//!
//! Provides batch merging of multiple `MeshData` buffers into a single unified mesh,
//! including zero-copy reference merging and streaming single-pass boundary welding.

use std::collections::HashMap;

use super::merge_attributes::seed_custom_attributes;
use super::{MeshData, UvLayout};

struct MergedMeshPrealloc {
    merged: MeshData,
    total_verts: usize,
    has_colors: bool,
    has_custom_attrs: bool,
}

impl MergedMeshPrealloc {
    fn new(meshes: &[&MeshData]) -> Self {
        let mut total_verts = 0;
        let mut total_normals = 0;
        let mut total_uvs = 0;
        let mut total_indices = 0;
        let mut total_quads = 0;
        let mut total_faces = 0;
        let mut total_colors = 0;
        let mut total_sec_uvs = 0;
        let mut has_sec_uvs = false;
        let mut has_colors = false;
        let mut has_custom_attrs = false;

        for &m in meshes {
            total_verts += m.positions.len();
            total_normals += m.normals.len();
            total_uvs += m.uvs.len();
            total_indices += m.indices.len();
            if let Some(ref q) = m.quad_indices {
                total_quads += q.len();
            }
            total_faces += m.face_materials.len();
            if let Some(ref s) = m.secondary_uvs {
                total_sec_uvs += s.len();
                has_sec_uvs = true;
            }
            if let Some(ref c) = m.colors {
                total_colors += c.len();
                has_colors = true;
            }
            if !m.custom_attributes.is_empty() {
                has_custom_attrs = true;
            }
        }

        let total_corners = total_quads.max(total_uvs).max(total_faces * 4);

        let uv_layout = meshes
            .iter()
            .map(|m| m.uv_layout)
            .find(|layout| *layout != super::UvLayout::None)
            .unwrap_or_default();

        let mut merged = MeshData {
            positions: Vec::with_capacity(total_verts),
            normals: Vec::with_capacity(total_normals),
            uvs: Vec::with_capacity(total_uvs),
            uv_layout,
            secondary_uvs: if has_sec_uvs {
                Some(Vec::with_capacity(total_sec_uvs.max(total_uvs)))
            } else {
                None
            },
            colors: if has_colors {
                Some(Vec::with_capacity(total_colors.max(total_corners)))
            } else {
                None
            },
            indices: Vec::with_capacity(total_indices),
            quad_indices: if total_quads > 0 {
                Some(Vec::with_capacity(total_quads))
            } else {
                None
            },
            face_materials: Vec::with_capacity(total_faces),
            face_tint_indices: Vec::with_capacity(total_faces),
            custom_attributes: HashMap::new(),
        };

        if has_custom_attrs {
            seed_custom_attributes(
                &mut merged.custom_attributes,
                meshes,
                total_faces,
                total_verts,
                total_corners,
            );
        }

        Self {
            merged,
            total_verts,
            has_colors,
            has_custom_attrs,
        }
    }
}

impl MeshData {
    /// Appends another `MeshData` into this mesh data buffer.
    pub fn append_mesh(&mut self, other: &MeshData) {
        let base_idx = self.positions.len() as u32;
        if self.uv_layout == UvLayout::None {
            self.uv_layout = other.uv_layout;
        }

        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);

        let old_idx_len = self.indices.len();
        self.indices.extend_from_slice(&other.indices);
        for idx in &mut self.indices[old_idx_len..] {
            *idx += base_idx;
        }

        if let Some(ref o_quads) = other.quad_indices {
            let quads = self.quad_indices.get_or_insert_with(Vec::new);
            let old_quad_len = quads.len();
            quads.extend_from_slice(o_quads);
            for idx in &mut quads[old_quad_len..] {
                *idx += base_idx;
            }
        }

        self.face_materials.extend_from_slice(&other.face_materials);
        self.face_tint_indices
            .extend_from_slice(&other.face_tint_indices);

        if let Some(ref other_sec) = other.secondary_uvs {
            let sec = self.secondary_uvs.get_or_insert_with(Vec::new);
            sec.extend_from_slice(other_sec);
        }

        if self.colors.is_some() || other.colors.is_some() {
            let col = self.colors.get_or_insert_with(Vec::new);
            let prev_target = if let Some(ref q) = self.quad_indices {
                let other_q_len = other.quad_indices.as_ref().map_or(0, |v| v.len());
                q.len().saturating_sub(other_q_len)
            } else if !self.uvs.is_empty() {
                self.uvs.len().saturating_sub(other.uvs.len())
            } else {
                base_idx as usize
            };
            while col.len() < prev_target {
                col.push([1.0, 1.0, 1.0, 1.0]);
            }
            if let Some(ref other_col) = other.colors {
                col.extend_from_slice(other_col);
            }
            let total_target = if let Some(ref q) = self.quad_indices {
                q.len()
            } else if !self.uvs.is_empty() {
                self.uvs.len()
            } else {
                self.positions.len()
            };
            while col.len() < total_target {
                col.push([1.0, 1.0, 1.0, 1.0]);
            }
        }

        Self::extend_custom_attributes(&mut self.custom_attributes, &other.custom_attributes);
    }

    /// Efficiently merges multiple `MeshData` references into a single unified `MeshData`,
    /// pre-allocating exact capacities upfront to eliminate vector reallocations and cloning.
    pub fn merge_all_refs(meshes: &[&MeshData]) -> Self {
        if meshes.is_empty() {
            return Self::new();
        }
        if meshes.len() == 1 {
            return (*meshes[0]).clone();
        }

        let pre = MergedMeshPrealloc::new(meshes);
        let mut merged = pre.merged;
        let has_colors = pre.has_colors;
        let has_custom_attrs = pre.has_custom_attrs;

        for &m in meshes {
            let base_idx = merged.positions.len() as u32;
            merged.positions.extend_from_slice(&m.positions);
            merged.normals.extend_from_slice(&m.normals);
            merged.uvs.extend_from_slice(&m.uvs);

            let old_idx_len = merged.indices.len();
            merged.indices.extend_from_slice(&m.indices);
            for idx in &mut merged.indices[old_idx_len..] {
                *idx += base_idx;
            }

            if let Some(ref o_quads) = m.quad_indices {
                let quads = merged.quad_indices.get_or_insert_with(Vec::new);
                let old_quad_len = quads.len();
                quads.extend_from_slice(o_quads);
                for idx in &mut quads[old_quad_len..] {
                    *idx += base_idx;
                }
            }

            merged.face_materials.extend_from_slice(&m.face_materials);
            merged
                .face_tint_indices
                .extend_from_slice(&m.face_tint_indices);

            if let Some(ref other_sec) = m.secondary_uvs {
                let sec = merged.secondary_uvs.get_or_insert_with(Vec::new);
                sec.extend_from_slice(other_sec);
            }

            if has_colors {
                let col = merged.colors.get_or_insert_with(Vec::new);
                let expected_count = if let Some(ref q) = m.quad_indices {
                    q.len()
                } else if !m.uvs.is_empty() {
                    m.uvs.len()
                } else {
                    m.positions.len()
                };
                if let Some(ref other_col) = m.colors {
                    col.extend_from_slice(other_col);
                    while col.len() < merged.uvs.len().max(merged.positions.len()) {
                        col.push([1.0, 1.0, 1.0, 1.0]);
                    }
                } else {
                    col.resize(col.len() + expected_count, [1.0, 1.0, 1.0, 1.0]);
                }
            }

            if has_custom_attrs {
                Self::extend_custom_attributes(&mut merged.custom_attributes, &m.custom_attributes);
            }
        }

        merged
    }

    /// Merges multiple internally-welded Section meshes into a single seamless unified mesh,
    /// deduplicating only co-located boundary vertices along chunk interfaces in a single streaming pass.
    pub fn merge_welded_sections(meshes: &[&MeshData], tolerance: f32) -> Self {
        Self::merge_welded_sections_with(meshes, tolerance, |_| true)
    }

    /// Merges multiple internally-welded Section meshes, welding only the vertices for which
    /// `is_weld_candidate` returns `true`.
    pub fn merge_welded_sections_filtered<F>(
        meshes: &[&MeshData],
        tolerance: f32,
        is_weld_candidate: F,
    ) -> Self
    where
        F: Fn([i32; 3]) -> bool,
    {
        Self::merge_welded_sections_with(meshes, tolerance, is_weld_candidate)
    }

    fn merge_welded_sections_with<F>(
        meshes: &[&MeshData],
        tolerance: f32,
        is_weld_candidate: F,
    ) -> Self
    where
        F: Fn([i32; 3]) -> bool,
    {
        if meshes.is_empty() {
            return Self::new();
        }
        if meshes.len() == 1 {
            return (*meshes[0]).clone();
        }
        if tolerance <= 0.0 {
            return Self::merge_all_refs(meshes);
        }

        let pre = MergedMeshPrealloc::new(meshes);
        let mut merged = pre.merged;
        let total_verts = pre.total_verts;
        let has_colors = pre.has_colors;
        let has_custom_attrs = pre.has_custom_attrs;

        let inv_dist = 1.0 / tolerance;
        // Spatial hash map mapping discrete grid position [i32; 3] to global vertex index in merged.positions.
        let mut coord_map = rustc_hash::FxHashMap::<[i32; 3], u32>::with_capacity_and_hasher(
            (total_verts / 4).max(64),
            Default::default(),
        );

        let mut local_remap = Vec::with_capacity(2048);

        for &m in meshes {
            if m.positions.is_empty() {
                continue;
            }

            local_remap.clear();
            local_remap.reserve(m.positions.len());

            for (i, &p) in m.positions.iter().enumerate() {
                let key = [
                    (p[0] * inv_dist).round() as i32,
                    (p[1] * inv_dist).round() as i32,
                    (p[2] * inv_dist).round() as i32,
                ];

                if is_weld_candidate(key) {
                    if let Some(&existing_idx) = coord_map.get(&key) {
                        local_remap.push(existing_idx);
                        continue;
                    }
                    let new_idx = merged.positions.len() as u32;
                    coord_map.insert(key, new_idx);
                    local_remap.push(new_idx);
                } else {
                    local_remap.push(merged.positions.len() as u32);
                }

                merged.positions.push(p);
                if let Some(&norm) = m.normals.get(i) {
                    merged.normals.push(norm);
                }
            }

            // Remap and append triangle indices directly in a single streaming pass
            merged.indices.reserve(m.indices.len());
            for &idx in &m.indices {
                if let Some(&new_i) = local_remap.get(idx as usize) {
                    merged.indices.push(new_i);
                } else {
                    merged.indices.push(idx);
                }
            }

            // Remap and append quad indices directly in a single pass
            if let Some(ref o_quads) = m.quad_indices {
                let quads = merged.quad_indices.get_or_insert_with(Vec::new);
                quads.reserve(o_quads.len());
                for &idx in o_quads {
                    if let Some(&new_i) = local_remap.get(idx as usize) {
                        quads.push(new_i);
                    } else {
                        quads.push(idx);
                    }
                }
            }

            // Direct contiguous memory copies for per-loop and per-face attributes
            merged.uvs.extend_from_slice(&m.uvs);
            merged.face_materials.extend_from_slice(&m.face_materials);
            merged
                .face_tint_indices
                .extend_from_slice(&m.face_tint_indices);

            if let Some(ref other_sec) = m.secondary_uvs {
                let sec = merged.secondary_uvs.get_or_insert_with(Vec::new);
                sec.extend_from_slice(other_sec);
            }

            if has_colors {
                let col = merged.colors.get_or_insert_with(Vec::new);
                let expected_count = if let Some(ref q) = m.quad_indices {
                    q.len()
                } else if !m.uvs.is_empty() {
                    m.uvs.len()
                } else {
                    m.positions.len()
                };
                if let Some(ref other_col) = m.colors {
                    col.extend_from_slice(other_col);
                    while col.len() < merged.uvs.len().max(merged.positions.len()) {
                        col.push([1.0, 1.0, 1.0, 1.0]);
                    }
                } else {
                    col.resize(col.len() + expected_count, [1.0, 1.0, 1.0, 1.0]);
                }
            }

            if has_custom_attrs {
                Self::extend_custom_attributes(&mut merged.custom_attributes, &m.custom_attributes);
            }
        }

        merged
    }
}
