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
pub(crate) const GRID_POINT_COUNT: usize = 17 * 17 * 17;

/// One slot per `(corner, face direction)`.
///
/// Positions alone are not a valid weld key: a corner shared by three faces has three different
/// flat normals, and merging them would leave two of the three faces with a wrong normal (only
/// visible under normals-based shading or glTF export, never under Blender's flat shading).
pub(crate) const GRID_POOL_SIZE: usize = GRID_POINT_COUNT * 6;

/// Canonical corner relative coordinate offsets `(dx, dy, dz)` for each of the 6 cardinal directions,
/// matching `Direction::to_index()` order:
/// [0: East(+X), 1: West(-X), 2: Up(+Y), 3: Down(-Y), 4: South(+Z), 5: North(-Z)].
pub(crate) const UNIT_CUBE_CORNERS: [[[usize; 3]; 4]; 6] = [
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
///
/// Slots carry an epoch stamp instead of being cleared, so one pool can be reused for every
/// section on a thread: a 29,478-slot table is 118 KiB, and zeroing it per section (624 sections
/// per world rebuild) dominated the cost of the extra face-direction slots.
pub(crate) struct GridVertexPool {
    stamps: Vec<u32>,
    indices: Vec<u32>,
    epoch: u32,
}

impl GridVertexPool {
    /// Creates a new `GridVertexPool`; its slots read as free until [`Self::begin_mesh`].
    #[inline]
    pub fn new() -> Self {
        Self {
            stamps: vec![0; GRID_POOL_SIZE],
            indices: vec![u32::MAX; GRID_POOL_SIZE],
            epoch: 0,
        }
    }

    /// Claims a fresh epoch, so every slot from the previous mesh reads as free.
    #[inline]
    pub fn begin_mesh(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            // Wrapped after 2^32 meshes on one thread: clear once and restart at 1.
            self.stamps.fill(0);
            self.epoch = 1;
        }
    }

    /// Retrieves an existing vertex index or allocates a new vertex in `mesh.positions` and `mesh.normals`.
    ///
    /// `direction` is the face normal's `Direction::to_index()`: vertices are welded per face
    /// orientation so every quad keeps its own flat normal.
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
        direction: u8,
    ) -> u32 {
        let idx = ((gx * 17 + gy) * 17 + gz) * 6 + direction as usize;
        if self.stamps[idx] == self.epoch {
            return self.indices[idx];
        }

        let new_idx = mesh.positions.len() as u32;
        let world_pos = world_origin + Vec3::new(gx as f32, gy as f32, gz as f32);
        let p = config.transform_position(world_pos);
        mesh.positions.push([p.x, p.y, p.z]);
        mesh.normals.push(normal);
        self.stamps[idx] = self.epoch;
        self.indices[idx] = new_idx;
        new_idx
    }
}

/// Runs `body` with the calling thread's reusable pool, reset for a new mesh.
///
/// Sections are meshed in parallel batches; one pool per worker thread removes both the 118 KiB
/// allocation per section and its full-table clear (624 sections per world rebuild).
pub(crate) fn with_thread_pool<R>(body: impl FnOnce(&mut GridVertexPool) -> R) -> R {
    use std::cell::RefCell;
    thread_local! {
        static POOL: RefCell<GridVertexPool> = RefCell::new(GridVertexPool::new());
    }
    POOL.with(|cell| {
        let mut pool = cell.borrow_mut();
        pool.begin_mesh();
        body(&mut pool)
    })
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
        pool.begin_mesh();
        let mut mesh = MeshData::new();
        let config = MesherConfig::default();
        let origin = Vec3::ZERO;

        let v1 = pool.get_or_create(&mut mesh, &config, origin, 1, 2, 3, [0.0, 1.0, 0.0], 2);
        assert_eq!(v1, 0);
        assert_eq!(mesh.vertex_count(), 1);

        // Same coordinate and direction returns the existing index without adding a vertex
        let v2 = pool.get_or_create(&mut mesh, &config, origin, 1, 2, 3, [0.0, 1.0, 0.0], 2);
        assert_eq!(v2, 0);
        assert_eq!(mesh.vertex_count(), 1);

        // Same coordinate, different direction: a separate vertex with its own normal
        let v_side = pool.get_or_create(&mut mesh, &config, origin, 1, 2, 3, [-1.0, 0.0, 0.0], 1);
        assert_eq!(v_side, 1);
        assert_eq!(mesh.normals[v_side as usize], [-1.0, 0.0, 0.0]);

        // Different coordinate creates new index
        let v3 = pool.get_or_create(&mut mesh, &config, origin, 2, 2, 3, [0.0, 1.0, 0.0], 2);
        assert_eq!(v3, 2);
        assert_eq!(mesh.vertex_count(), 3);
    }
}
