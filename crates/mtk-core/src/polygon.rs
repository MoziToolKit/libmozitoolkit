//! # Flat Polygon Mesh Topology Container
//!
//! Provides flat contiguous buffer representation of polygonal mesh topology and UVs.
//! Eliminates thousands of nested heap allocations (`Vec<Vec<T>>`) and enables
//! zero-copy memoryview ingestion directly from flat GPU/DCC vertex and loop buffers.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Flat contiguous buffer representation of polygonal mesh topology and UVs.
///
/// Eliminates thousands of nested heap allocations (`Vec<Vec<T>>`) and enables
/// zero-copy memoryview ingestion directly from flat GPU/DCC vertex and loop buffers.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FlatPolygonMesh {
    /// 3D vertex positions for the mesh [x, y, z]
    pub positions: Vec<[f32; 3]>,
    /// Flattened vertex indices for all polygon loops (contiguous buffer)
    pub loop_vertices: Vec<u32>,
    /// Flattened UV coordinates for all polygon loops (contiguous buffer)
    pub loop_uvs: Vec<[f32; 2]>,
    /// Starting offset of each face into loop_vertices and loop_uvs
    pub face_loop_starts: Vec<u32>,
    /// Number of vertices (loops) for each face
    pub face_loop_totals: Vec<u32>,
    /// Material slot ID for each face
    pub face_materials: Vec<u32>,
}

impl FlatPolygonMesh {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn face_count(&self) -> usize {
        self.face_loop_totals.len()
    }

    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    #[inline]
    pub fn loop_count(&self) -> usize {
        self.loop_vertices.len()
    }

    /// Returns a zero-copy slice of vertex indices for the given face.
    #[inline]
    pub fn face_vertices(&self, face_idx: usize) -> &[u32] {
        if face_idx >= self.face_loop_starts.len() || face_idx >= self.face_loop_totals.len() {
            return &[];
        }
        let start = self.face_loop_starts[face_idx] as usize;
        let total = self.face_loop_totals[face_idx] as usize;
        let end = (start + total).min(self.loop_vertices.len());
        if start <= end {
            &self.loop_vertices[start..end]
        } else {
            &[]
        }
    }

    /// Returns a zero-copy slice of UV coordinates for the given face.
    #[inline]
    pub fn face_uvs(&self, face_idx: usize) -> &[[f32; 2]] {
        if face_idx >= self.face_loop_starts.len() || face_idx >= self.face_loop_totals.len() {
            return &[];
        }
        let start = self.face_loop_starts[face_idx] as usize;
        let total = self.face_loop_totals[face_idx] as usize;
        let end = (start + total).min(self.loop_uvs.len());
        if start <= end {
            &self.loop_uvs[start..end]
        } else {
            &[]
        }
    }

    /// Constructs a `FlatPolygonMesh` by flattening nested face collections.
    pub fn from_nested(
        positions: Vec<[f32; 3]>,
        face_vertices: &[Vec<u32>],
        face_uvs: &[Vec<[f32; 2]>],
        face_materials: &[u32],
    ) -> Self {
        let face_count = face_vertices.len();
        let total_loops: usize = face_vertices.iter().map(|f| f.len()).sum();

        let mut loop_vertices = Vec::with_capacity(total_loops);
        let mut loop_uvs = Vec::with_capacity(total_loops);
        let mut face_loop_starts = Vec::with_capacity(face_count);
        let mut face_loop_totals = Vec::with_capacity(face_count);

        let mut current_offset = 0u32;
        for (i, f_verts) in face_vertices.iter().enumerate() {
            let total = f_verts.len() as u32;
            face_loop_starts.push(current_offset);
            face_loop_totals.push(total);

            loop_vertices.extend_from_slice(f_verts);

            if let Some(uvs) = face_uvs.get(i) {
                if uvs.len() == f_verts.len() {
                    loop_uvs.extend_from_slice(uvs);
                } else {
                    for j in 0..f_verts.len() {
                        loop_uvs.push(uvs.get(j).copied().unwrap_or([0.0, 0.0]));
                    }
                }
            } else {
                loop_uvs.resize(loop_uvs.len() + f_verts.len(), [0.0, 0.0]);
            }

            current_offset += total;
        }

        Self {
            positions,
            loop_vertices,
            loop_uvs,
            face_loop_starts,
            face_loop_totals,
            face_materials: face_materials.to_vec(),
        }
    }

    /// Constructs a `FlatPolygonMesh` from raw contiguous 1D slices (e.g. from NumPy or C-ABI).
    pub fn from_flat_buffers(
        positions_flat: &[f32],
        loop_vertices: Vec<u32>,
        loop_uvs_flat: &[f32],
        face_loop_starts: Vec<u32>,
        face_loop_totals: Vec<u32>,
        face_materials: Vec<u32>,
    ) -> Self {
        let positions = positions_flat.as_chunks::<3>().0.to_vec();
        let loop_uvs = loop_uvs_flat.as_chunks::<2>().0.to_vec();
        Self {
            positions,
            loop_vertices,
            loop_uvs,
            face_loop_starts,
            face_loop_totals,
            face_materials,
        }
    }

    /// Constructs a `FlatPolygonMesh` from CSR-style face offset indices (length `face_count + 1`).
    pub fn from_face_offsets(
        positions_flat: &[f32],
        loop_vertices: Vec<u32>,
        loop_uvs_flat: &[f32],
        face_offsets: &[i32],
        face_materials: Vec<u32>,
    ) -> Result<Self, String> {
        let face_count = if face_offsets.len() > 1 {
            face_offsets.len() - 1
        } else {
            0
        };
        let mut face_loop_starts = Vec::with_capacity(face_count);
        let mut face_loop_totals = Vec::with_capacity(face_count);
        for i in 0..face_count {
            let start = face_offsets[i];
            let end = face_offsets[i + 1];
            if start < 0 || end < start {
                return Err(format!(
                    "invalid face offset range at index {}: {}..{}",
                    i, start, end
                ));
            }
            face_loop_starts.push(start as u32);
            face_loop_totals.push((end - start) as u32);
        }
        let positions = positions_flat.as_chunks::<3>().0.to_vec();
        let loop_uvs = loop_uvs_flat.as_chunks::<2>().0.to_vec();
        Ok(Self {
            positions,
            loop_vertices,
            loop_uvs,
            face_loop_starts,
            face_loop_totals,
            face_materials,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_face_offsets() {
        let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
        let loop_verts = vec![0, 1, 2, 3];
        let loop_uvs = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let face_offsets = [0, 4];
        let mesh = FlatPolygonMesh::from_face_offsets(
            &positions,
            loop_verts,
            &loop_uvs,
            &face_offsets,
            vec![0],
        )
        .unwrap();
        assert_eq!(mesh.face_count(), 1);
        assert_eq!(mesh.face_loop_starts, vec![0]);
        assert_eq!(mesh.face_loop_totals, vec![4]);
    }
}
