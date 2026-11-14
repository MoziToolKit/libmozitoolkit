//! # Mesh Data Buffer and Geometry Pipeline
//!
//! Provides flat contiguous POD mesh buffer abstractions (`MeshData`) designed
//! for zero-copy transfers into graphics APIs and DCC host environments.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::attributes::{FaceAttributes, MaterialSlotId, MeshAttribute, TintIndex};
use crate::geometry::{mc_world_to_z_up, CoordinateSystem, Quad};

mod attributes;
mod from_flat;
pub mod greedy;
mod merge;
mod merge_attributes;
mod topology;
mod weld;

pub use greedy::{greedy_mesh_2d_mask, greedy_mesh_2d_values, greedy_span_1d, GreedyRect};
pub use topology::analyze_hard_edges;

/// Explicit interpretation of the `uvs` buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum UvLayout {
    /// No UVs are present.
    #[default]
    None,
    /// One UV pair per vertex, indexed by `indices`.
    PerVertex,
    /// One UV pair per polygon loop, in loop order.
    PerLoop,
}

/// Flat, contiguous mesh data buffer designed for zero-copy or direct buffer transfers
/// into graphics APIs (OpenGL/WebGPU/Vulkan) or 3D host engines (Blender, Bevy, Three.js).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MeshData {
    /// Contiguous 3D vertex positions `[x, y, z]`.
    pub positions: Vec<[f32; 3]>,
    /// Contiguous vertex normal vectors `[nx, ny, nz]`.
    pub normals: Vec<[f32; 3]>,
    /// Triangle face indices (every 3 consecutive u32 form a triangle).
    pub indices: Vec<u32>,
    /// Primary UV coordinates `[u, v]`; interpretation is defined by `uv_layout`.
    pub uvs: Vec<[f32; 2]>,
    /// Explicit layout of the `uvs` buffer.
    pub uv_layout: UvLayout,
    /// Secondary UV coordinates (e.g. for atlas tiling or colormap sampling).
    pub secondary_uvs: Option<Vec<[f32; 2]>>,
    /// Vertex RGBA colors `[r, g, b, a]` in linear color space.
    pub colors: Option<Vec<[f32; 4]>>,
    /// Material slot ID per face (per 2 triangles in quad baking).
    pub face_materials: Vec<MaterialSlotId>,
    /// Tint index per face.
    pub face_tint_indices: Vec<TintIndex>,
    /// Optional Quad polygon vertex indices (4 u32 per quad) preserving exact corner cyclic order.
    pub quad_indices: Option<Vec<u32>>,
    /// Generic typed custom attributes indexed by attribute name (Domain: Point/Corner/Face/Mesh).
    pub custom_attributes: HashMap<String, MeshAttribute>,
}

impl MeshData {
    /// Creates a new empty `MeshData` buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `MeshData` with pre-allocated capacity for efficiency.
    pub fn with_capacity(num_vertices: usize, num_indices: usize, num_faces: usize) -> Self {
        Self {
            positions: Vec::with_capacity(num_vertices),
            normals: Vec::with_capacity(num_vertices),
            indices: Vec::with_capacity(num_indices),
            uvs: Vec::with_capacity(num_vertices),
            uv_layout: UvLayout::None,
            secondary_uvs: None,
            colors: None,
            face_materials: Vec::with_capacity(num_faces),
            face_tint_indices: Vec::with_capacity(num_faces),
            quad_indices: None,
            custom_attributes: HashMap::new(),
        }
    }

    /// Creates a new `MeshData` with pre-allocated capacity for quad-based voxel geometry,
    /// explicitly pre-allocating `quad_indices` and optional `colors` buffers.
    pub fn with_quad_capacity(
        num_vertices: usize,
        num_indices: usize,
        num_faces: usize,
        with_colors: bool,
    ) -> Self {
        let quad_count = num_faces * 4;
        Self {
            positions: Vec::with_capacity(num_vertices),
            normals: Vec::with_capacity(num_vertices),
            indices: Vec::with_capacity(num_indices),
            uvs: Vec::with_capacity(num_vertices),
            uv_layout: UvLayout::None,
            secondary_uvs: None,
            colors: if with_colors {
                Some(Vec::with_capacity(quad_count.max(num_vertices)))
            } else {
                None
            },
            face_materials: Vec::with_capacity(num_faces),
            face_tint_indices: Vec::with_capacity(num_faces),
            quad_indices: Some(Vec::with_capacity(quad_count)),
            custom_attributes: HashMap::new(),
        }
    }

    /// Number of vertices in the mesh buffer.
    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles in the mesh buffer.
    #[inline]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of quad-level faces recorded in `face_materials`.
    #[inline]
    pub fn face_count(&self) -> usize {
        self.face_materials.len()
    }

    /// Number of quad faces recorded.
    #[inline]
    pub fn quad_count(&self) -> usize {
        if let Some(ref quads) = self.quad_indices {
            quads.len() / 4
        } else {
            self.indices.len() / 6
        }
    }

    /// Returns true if the mesh topology represents quad polygons
    /// (either explicitly through `quad_indices` or canonically pairing 2 triangles per face material).
    #[inline]
    pub fn is_quad_topology(&self) -> bool {
        self.quad_indices.is_some()
            || (!self.indices.is_empty()
                && self.indices.len() % 6 == 0
                && self.face_materials.len() == self.indices.len() / 6)
    }

    /// Number of polygons in the mesh (quad count if quad topology, triangle count otherwise).
    #[inline]
    pub fn poly_count(&self) -> usize {
        if self.is_quad_topology() {
            self.quad_count()
        } else {
            self.triangle_count()
        }
    }

    /// Total number of corner loops in the mesh (4 per quad, 3 per triangle).
    #[inline]
    pub fn loop_count(&self) -> usize {
        if self.is_quad_topology() {
            if let Some(ref quads) = self.quad_indices {
                quads.len()
            } else {
                (self.indices.len() / 6) * 4
            }
        } else {
            self.indices.len()
        }
    }

    /// Quad polygon vertex indices `[v0, v1, v2, v3, ...]` (4 u32 per quad).
    ///
    /// If `quad_indices` is present, returns a clone.
    /// Otherwise, reconstructs quad face polygons by topologically pairing adjacent triangle pairs.
    pub fn reconstruct_quad_indices(&self) -> Vec<u32> {
        if let Some(ref quads) = self.quad_indices {
            return quads.clone();
        }
        let quad_count = self.indices.len() / 6;
        let mut quads = Vec::with_capacity(quad_count * 4);
        for q in 0..quad_count {
            let base = q * 6;
            let t1 = [
                self.indices[base],
                self.indices[base + 1],
                self.indices[base + 2],
            ];
            let t2 = [
                self.indices[base + 3],
                self.indices[base + 4],
                self.indices[base + 5],
            ];

            // Identify the unique non-shared vertex of triangle 1 (O1)
            let mut o1_idx = 0;
            for i in 0..3 {
                if !t2.contains(&t1[i]) {
                    o1_idx = i;
                    break;
                }
            }
            let o1 = t1[o1_idx];
            let d1 = t1[(o1_idx + 2) % 3]; // predecessor in t1
            let d2 = t1[(o1_idx + 1) % 3]; // successor in t1

            // Identify the unique non-shared vertex of triangle 2 (O2)
            let o2 = t2.iter().copied().find(|v| !t1.contains(v)).unwrap_or(d2);

            quads.push(d1);
            quads.push(o1);
            quads.push(d2);
            quads.push(o2);
        }
        quads
    }

    /// Checks if the mesh data buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Adds or replaces a custom attribute on the mesh.
    pub fn add_custom_attribute(&mut self, attr: MeshAttribute) {
        self.custom_attributes.insert(attr.name.clone(), attr);
    }

    /// Gets a reference to a custom attribute by name.
    pub fn get_custom_attribute(&self, name: &str) -> Option<&MeshAttribute> {
        self.custom_attributes.get(name)
    }

    /// Removes a custom attribute by name, returning it if present.
    pub fn remove_custom_attribute(&mut self, name: &str) -> Option<MeshAttribute> {
        self.custom_attributes.remove(name)
    }

    /// Checks if a custom attribute exists.
    pub fn has_custom_attribute(&self, name: &str) -> bool {
        self.custom_attributes.contains_key(name)
    }

    /// Returns a list of all custom attribute names.
    pub fn custom_attribute_names(&self) -> Vec<String> {
        self.custom_attributes.keys().cloned().collect()
    }

    /// Clears all vertices, indices, and attributes while retaining memory allocations.
    pub fn clear(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.indices.clear();
        self.uvs.clear();
        self.uv_layout = UvLayout::None;
        if let Some(ref mut sec_uvs) = self.secondary_uvs {
            sec_uvs.clear();
        }
        if let Some(ref mut cols) = self.colors {
            cols.clear();
        }
        self.face_materials.clear();
        self.face_tint_indices.clear();
        if let Some(ref mut quads) = self.quad_indices {
            quads.clear();
        }
        self.custom_attributes.clear();
    }

    /// Transforms all vertex positions and normals in place according to the target coordinate system.
    ///
    /// Minecraft local/world geometry is converted to the requested target basis (e.g. Z-up right-handed).
    pub fn transform_coordinate_system(&mut self, target: CoordinateSystem) {
        match target {
            CoordinateSystem::Minecraft | CoordinateSystem::YUpRightHanded => {}
            CoordinateSystem::ZUpRightHanded => {
                for pos in &mut self.positions {
                    let v = mc_world_to_z_up(pos[0], pos[1], pos[2]);
                    *pos = [v.x, v.y, v.z];
                }
                for norm in &mut self.normals {
                    let n = mc_world_to_z_up(norm[0], norm[1], norm[2]);
                    *norm = [n.x, n.y, n.z];
                }
            }
        }
    }

    /// Appends a quad (4 vertices, 2 triangles: 0-1-2 and 0-2-3) and its face attributes.
    pub fn append_quad(&mut self, quad: &Quad, attributes: &FaceAttributes) {
        let base_idx = self.positions.len() as u32;

        let n = [quad.normal.x, quad.normal.y, quad.normal.z];
        for i in 0..4 {
            let v = quad.vertices[i];
            let uv = quad.uvs[i];
            self.positions.push([v.x, v.y, v.z]);
            self.normals.push(n);
            self.uvs.push([uv.x, uv.y]);
        }

        // Triangulate CCW quad: 0-1-2 and 0-2-3
        self.indices.push(base_idx);
        self.indices.push(base_idx + 1);
        self.indices.push(base_idx + 2);

        self.indices.push(base_idx);
        self.indices.push(base_idx + 2);
        self.indices.push(base_idx + 3);

        let quads = self.quad_indices.get_or_insert_with(Vec::new);
        quads.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx + 3]);
        self.uv_layout = UvLayout::PerLoop;

        self.face_materials.push(attributes.material_slot);
        self.face_tint_indices.push(attributes.tint_index);
    }

    /// Expands UVs to one pair per polygon loop according to `uv_layout`.
    pub fn loop_uvs_expanded(&self) -> Vec<[f32; 2]> {
        if self.uv_layout == UvLayout::PerLoop {
            return self.uvs.clone();
        }
        let total_loops = self.loop_count();
        if total_loops == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(total_loops);
        if self.is_quad_topology() {
            for v_idx in self.reconstruct_quad_indices() {
                out.push(self.uvs.get(v_idx as usize).copied().unwrap_or([0.0, 0.0]));
            }
        } else if !self.indices.is_empty() {
            for &v_idx in &self.indices {
                out.push(self.uvs.get(v_idx as usize).copied().unwrap_or([0.0, 0.0]));
            }
        } else {
            out.extend_from_slice(&self.uvs);
        }
        out
    }

    /// Loop start offsets for the active polygon topology.
    pub fn loop_starts(&self) -> Vec<u32> {
        let count = self.poly_count();
        let step = if self.is_quad_topology() { 4 } else { 3 };
        (0..count).map(|i| (i * step) as u32).collect()
    }

    /// Loop totals for the active polygon topology.
    pub fn loop_totals(&self) -> Vec<u32> {
        let count = self.poly_count();
        let step = if self.is_quad_topology() { 4 } else { 3 };
        vec![step as u32; count]
    }

    /// Returns a flat contiguous slice of vertex positions `[x0, y0, z0, x1, y1, z1, ...]`.
    #[inline]
    pub fn positions_flat(&self) -> &[f32] {
        bytemuck::cast_slice(&self.positions)
    }

    /// Returns a flat contiguous slice of vertex normals `[nx0, ny0, nz0, nx1, ny1, nz1, ...]`.
    #[inline]
    pub fn normals_flat(&self) -> &[f32] {
        bytemuck::cast_slice(&self.normals)
    }

    /// Returns a flat contiguous slice of primary UVs `[u0, v0, u1, v1, ...]`.
    #[inline]
    pub fn uvs_flat(&self) -> &[f32] {
        bytemuck::cast_slice(&self.uvs)
    }

    /// Returns a sorted list of unique material slot IDs present across all faces in this mesh.
    pub fn used_materials(&self) -> Vec<MaterialSlotId> {
        let mut set = std::collections::BTreeSet::new();
        for &m in &self.face_materials {
            set.insert(m);
        }
        set.into_iter().collect()
    }

    /// Returns true if this mesh contains any faces referencing dynamic entity atlas chunks (>= 0xFFF0).
    #[inline]
    pub fn has_dynamic_entity_chunks(&self) -> bool {
        self.face_materials
            .iter()
            .any(|&c| c >= crate::constants::atlas::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID)
    }

    /// Compacts `face_materials` to contiguous indices `0..N-1` and returns the mapping of original material IDs.
    pub fn compact_materials(&mut self) -> Vec<MaterialSlotId> {
        let original_ids = self.used_materials();
        if original_ids.is_empty() {
            return Vec::new();
        }
        let id_to_slot: HashMap<MaterialSlotId, MaterialSlotId> = original_ids
            .iter()
            .enumerate()
            .map(|(slot, &orig)| (orig, slot as MaterialSlotId))
            .collect();
        for m in &mut self.face_materials {
            if let Some(&new_slot) = id_to_slot.get(m) {
                *m = new_slot;
            }
        }
        original_ids
    }
}

#[cfg(test)]
mod tests;
