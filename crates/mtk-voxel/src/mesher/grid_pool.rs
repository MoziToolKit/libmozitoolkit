//! # Compact Grid Vertex Pool for Section Meshing
//!
//! Provides zero-allocation, instant integer grid point deduplication for unit cube meshing.
//! For a 16x16x16 section, unit cube corners lie exclusively on integer points in 0..=16,
//! giving exactly 17 * 17 * 17 = 4,913 possible spatial vertex slots.

use glam::Vec3;
use mtk_core::MeshData;

use crate::types::MesherConfig;

/// Total number of unique discrete corner points in a 16x16x16 chunk section:
/// (16 + 1) * (16 + 1) * (16 + 1) = 4,913.
pub const GRID_POOL_SIZE: usize = 17 * 17 * 17;

/// Canonical corner relative coordinate offsets `(dx, dy, dz)` for each of the 6 cardinal directions,
/// matching `Direction::to_index()` order:
/// [0: East(+X), 1: West(-X), 2: Up(+Y), 3: Down(-Y), 4: South(+Z), 5: North(-Z)].
pub const UNIT_CUBE_CORNERS: [[[usize; 3]; 4]; 6] = [
    // 0: East (+X)
    [[1, 1, 1], [1, 0, 1], [1, 0, 0], [1, 1, 0]],
    // 1: West (-X)
    [[0, 1, 0], [0, 0, 0], [0, 0, 1], [0, 1, 1]],
    // 2: Up (+Y)
    [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
    // 3: Down (-Y)
    [[0, 0, 1], [0, 0, 0], [1, 0, 0], [1, 0, 1]],
    // 4: South (+Z)
    [[0, 1, 1], [0, 0, 1], [1, 0, 1], [1, 1, 1]],
    // 5: North (-Z)
    [[1, 1, 0], [1, 0, 0], [0, 0, 0], [0, 1, 0]],
];

/// Linear lookup pool for instant deduplication of unit cube vertices in a 16x16x16 Section.
pub struct GridVertexPool {
    grid: Vec<u32>,
}

impl GridVertexPool {
    /// Creates a new `GridVertexPool` initialized with `u32::MAX`.
    #[inline]
    pub fn new() -> Self {
        Self {
            grid: vec![u32::MAX; GRID_POOL_SIZE],
        }
    }

    /// Resets all vertex slots to `u32::MAX` for reuse across sections without reallocation.
    #[inline]
    pub fn reset(&mut self) {
        self.grid.fill(u32::MAX);
    }

    /// Checks if a discrete corner grid coordinate lies on any of the 6 boundary faces of the section.
    #[inline(always)]
    pub fn is_boundary(gx: usize, gy: usize, gz: usize) -> bool {
        gx == 0 || gx == 16 || gy == 0 || gy == 16 || gz == 0 || gz == 16
    }

    /// Retrieves an existing vertex index or allocates a new vertex in `mesh.positions` and `mesh.normals`.
    #[inline(always)]
    pub fn get_or_create(
        &mut self,
        mesh: &mut MeshData,
        config: &MesherConfig,
        world_origin: Vec3,
        gx: usize,
        gy: usize,
        gz: usize,
        normal: [f32; 3],
    ) -> u32 {
        let idx = (gx * 17 + gy) * 17 + gz;
        let existing = self.grid[idx];
        if existing != u32::MAX {
            return existing;
        }

        let new_idx = mesh.positions.len() as u32;
        let world_pos = world_origin + Vec3::new(gx as f32, gy as f32, gz as f32);
        let p = config.transform_position(world_pos);
        mesh.positions.push([p.x, p.y, p.z]);
        mesh.normals.push(normal);
        self.grid[idx] = new_idx;
        new_idx
    }
}

impl Default for GridVertexPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_vertex_pool_dedup() {
        let mut pool = GridVertexPool::new();
        let mut mesh = MeshData::new();
        let config = MesherConfig::default();
        let origin = Vec3::ZERO;

        let v1 = pool.get_or_create(&mut mesh, &config, origin, 1, 2, 3, [0.0, 1.0, 0.0]);
        assert_eq!(v1, 0);
        assert_eq!(mesh.vertex_count(), 1);

        // Same coordinate should return existing index without adding a new vertex
        let v2 = pool.get_or_create(&mut mesh, &config, origin, 1, 2, 3, [0.0, 1.0, 0.0]);
        assert_eq!(v2, 0);
        assert_eq!(mesh.vertex_count(), 1);

        // Different coordinate creates new index
        let v3 = pool.get_or_create(&mut mesh, &config, origin, 2, 2, 3, [0.0, 1.0, 0.0]);
        assert_eq!(v3, 1);
        assert_eq!(mesh.vertex_count(), 2);
    }

    #[test]
    fn test_is_boundary() {
        assert!(GridVertexPool::is_boundary(0, 5, 5));
        assert!(GridVertexPool::is_boundary(16, 5, 5));
        assert!(GridVertexPool::is_boundary(5, 0, 5));
        assert!(GridVertexPool::is_boundary(5, 16, 5));
        assert!(GridVertexPool::is_boundary(5, 5, 0));
        assert!(GridVertexPool::is_boundary(5, 5, 16));

        assert!(!GridVertexPool::is_boundary(1, 1, 1));
        assert!(!GridVertexPool::is_boundary(8, 8, 8));
        assert!(!GridVertexPool::is_boundary(15, 15, 15));
    }
}
