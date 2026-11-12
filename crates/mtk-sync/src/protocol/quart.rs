//! # 4x4x4 Quart Spatial Geometry (RFC v3.1 Section 4.5)
//!
//! Provides spatial calculations for mapping voxel coordinate volumes to 4x4x4 quart biome cells.

use glam::IVec3;

/// 4x4x4 Quart spatial geometry helper aligned with Minecraft biome grid conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuartGrid {
    pub min_pos: IVec3,
    pub size: IVec3,
    pub min_q: IVec3,
    pub max_q: IVec3,
    pub size_q: IVec3, // (qw, qh, qd)
    pub quart_slice_area: usize,
    pub total_quarts: usize,
}

impl QuartGrid {
    pub fn from_bounds(min_pos: IVec3, size: IVec3) -> Self {
        let min_qx = min_pos.x.div_euclid(4);
        let max_qx = (min_pos.x + size.x - 1).div_euclid(4);
        let qw = (max_qx - min_qx + 1).max(0);

        let min_qy = min_pos.y.div_euclid(4);
        let max_qy = (min_pos.y + size.y - 1).div_euclid(4);
        let qh = (max_qy - min_qy + 1).max(0);

        let min_qz = min_pos.z.div_euclid(4);
        let max_qz = (min_pos.z + size.z - 1).div_euclid(4);
        let qd = (max_qz - min_qz + 1).max(0);

        let quart_slice_area = (qw as usize) * (qd as usize);
        let total_quarts = (qw as usize) * (qh as usize) * (qd as usize);

        Self {
            min_pos,
            size,
            min_q: IVec3::new(min_qx, min_qy, min_qz),
            max_q: IVec3::new(max_qx, max_qy, max_qz),
            size_q: IVec3::new(qw, qh, qd),
            quart_slice_area,
            total_quarts,
        }
    }

    /// Computes the flat 1D quart index for local relative voxel coordinates (lx, ly, lz).
    /// Properly accounts for unaligned and negative bounding volumes.
    #[inline]
    pub fn relative_voxel_to_quart_index(&self, lx: u32, ly: u32, lz: u32) -> usize {
        let qx = ((self.min_pos.x + lx as i32).div_euclid(4) - self.min_q.x) as usize;
        let qy = ((self.min_pos.y + ly as i32).div_euclid(4) - self.min_q.y) as usize;
        let qz = ((self.min_pos.z + lz as i32).div_euclid(4) - self.min_q.z) as usize;
        qy * self.quart_slice_area + qz * (self.size_q.x as usize) + qx
    }

    /// Converts a flat 1D quart stream index into its world quart coordinate (world_qx, world_qy, world_qz).
    #[inline]
    pub fn quart_index_to_world_coords(&self, q_idx: usize) -> IVec3 {
        let qy = (q_idx / self.quart_slice_area) as i32;
        let rem = q_idx % self.quart_slice_area;
        let qw = self.size_q.x as usize;
        let qz = (rem / qw) as i32;
        let qx = (rem % qw) as i32;

        IVec3::new(self.min_q.x + qx, self.min_q.y + qy, self.min_q.z + qz)
    }
}

/// Helper for converting flat 1D voxel indices to 3D relative coordinates (lx, ly, lz)
/// in canonical RFC (Y-slowest, Z-next, X-fastest) traversal order.
#[inline]
pub fn voxel_index_to_relative_coords(idx: usize, size: IVec3) -> (u32, u32, u32) {
    let sx = size.x as usize;
    let slice_area = sx * (size.z as usize);
    let ly = (idx / slice_area) as u32;
    let rem = idx % slice_area;
    let lz = (rem / sx) as u32;
    let lx = (rem % sx) as u32;
    (lx, ly, lz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quart_grid_dimensions() {
        let grid = QuartGrid::from_bounds(IVec3::new(0, 0, 0), IVec3::new(16, 16, 16));
        assert_eq!(grid.size_q, IVec3::new(4, 4, 4));
        assert_eq!(grid.quart_slice_area, 16);
        assert_eq!(grid.total_quarts, 64);

        assert_eq!(grid.relative_voxel_to_quart_index(0, 0, 0), 0);
        assert_eq!(grid.relative_voxel_to_quart_index(3, 3, 3), 0);
        assert_eq!(grid.relative_voxel_to_quart_index(4, 0, 0), 1);
        assert_eq!(grid.relative_voxel_to_quart_index(0, 4, 0), 16);
    }

    #[test]
    fn test_unaligned_negative_quart_grid() {
        let grid = QuartGrid::from_bounds(IVec3::new(-1, -1, -1), IVec3::new(2, 2, 2));
        assert_eq!(grid.min_q, IVec3::new(-1, -1, -1));
        assert_eq!(grid.max_q, IVec3::new(0, 0, 0));
        assert_eq!(grid.size_q, IVec3::new(2, 2, 2));
        assert_eq!(grid.total_quarts, 8);
    }
}
