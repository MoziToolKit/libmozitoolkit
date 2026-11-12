//! # 4x4x4 Quart Spatial Geometry (RFC v3.1 Section 4.5)
//!
//! Provides spatial calculations for mapping voxel coordinate volumes to 4x4x4 quart biome cells.

use glam::IVec3;

/// 4x4x4 Quart spatial geometry helper aligned with Minecraft biome grid conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuartGrid {
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
            min_q: IVec3::new(min_qx, min_qy, min_qz),
            max_q: IVec3::new(max_qx, max_qy, max_qz),
            size_q: IVec3::new(qw, qh, qd),
            quart_slice_area,
            total_quarts,
        }
    }

    /// Computes the flat 1D quart index for local relative voxel coordinates (lx, ly, lz).
    #[inline]
    pub fn relative_voxel_to_quart_index(&self, lx: u32, ly: u32, lz: u32) -> usize {
        let qy = (ly as usize) / 4;
        let qz = (lz as usize) / 4;
        let qx = (lx as usize) / 4;
        qy * self.quart_slice_area + qz * (self.size_q.x as usize) + qx
    }

    /// Computes the flat 1D quart index for local quart offsets (qx, qy, qz) within this grid.
    #[inline]
    pub fn quart_coords_to_index(&self, qx: usize, qy: usize, qz: usize) -> usize {
        qy * self.quart_slice_area + qz * (self.size_q.x as usize) + qx
    }
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
