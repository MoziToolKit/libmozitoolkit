//! Centralized physical, geometric, and voxel constants for MoziToolKit.

use crate::geometry::Aabb2d;

/// Chunk section and voxel domain constants.
pub mod voxel {
    /// Dimension size of a Minecraft chunk section along each axis (X, Y, Z).
    pub const SECTION_SIZE: usize = 16;

    /// Total voxel volume within a single 16x16x16 chunk section (`4096`).
    pub const SECTION_VOLUME: usize = SECTION_SIZE * SECTION_SIZE * SECTION_SIZE;

    /// Padded array dimension along each axis, including 1-block neighbor boundary on all 6 sides (`18`).
    pub const PADDED_SIZE: usize = 18;

    /// Total padded volume for a 1-block extended neighborhood buffer (`18 * 18 * 18 = 5832`).
    pub const PADDED_VOLUME: usize = PADDED_SIZE * PADDED_SIZE * PADDED_SIZE;

    /// Computes the flat linear index inside a 16x16x16 section storage.
    ///
    /// Standard coordinate layout: `x * 256 + y * 16 + z` (or `(x << 8) | (y << 4) | z`).
    #[inline(always)]
    pub const fn block_index(x: usize, y: usize, z: usize) -> usize {
        (x << 8) | (y << 4) | z
    }

    /// Computes the flat linear index in an 18x18x18 padded neighborhood storage buffer.
    ///
    /// Coordinate layout: `px * 324 + py * 18 + pz`.
    #[inline(always)]
    pub const fn padded_index(px: usize, py: usize, pz: usize) -> usize {
        px * (PADDED_SIZE * PADDED_SIZE) + py * PADDED_SIZE + pz
    }
}

/// Geometry, tolerance, and UV coordinate domain constants.
pub mod geometry {
    use super::*;

    /// Standard floating-point epsilon tolerance for geometric comparisons (`1e-4`).
    pub const EPS: f32 = 1e-4;

    /// Strict tolerance for intersection and clipping (`1e-5`).
    pub const STRICT_EPS: f32 = 1e-5;

    /// Fine numerical tolerance (`1e-6`).
    pub const TOLERANCE: f32 = 1e-6;

    /// Canonical full-face 2D bounding rectangle in `[0.0..1.0, 0.0..1.0]` normalized space.
    pub const FULL_FACE_RECT: Aabb2d = Aabb2d::UNIT;

    /// Canonical empty/degenerate 2D bounding rectangle.
    pub const EMPTY_FACE_RECT: Aabb2d = Aabb2d::ZERO;
}

/// Fluid physics and rendering constants.
pub mod fluid {
    /// Maximum height of a standard Minecraft still fluid block: `8/9` (~`0.8888889`).
    pub const MAX_FLUID_HEIGHT: f32 = 8.0 / 9.0;
}

/// Texture atlas domain constants.
pub mod atlas {
    /// Starting chunk ID for dynamically generated entity atlas sheets (65520 / 0xFFF0).
    pub const DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID: u16 = 0xFFF0;
}

/// Canonical mesh attribute constants.
pub mod attributes {
    pub use crate::attributes::constants::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn test_voxel_constants() {
        assert_eq!(voxel::SECTION_SIZE, 16);
        assert_eq!(voxel::SECTION_VOLUME, 4096);
        assert_eq!(voxel::PADDED_SIZE, 18);
        assert_eq!(voxel::PADDED_VOLUME, 5832);
        assert_eq!(voxel::block_index(0, 0, 0), 0);
        assert_eq!(voxel::block_index(1, 0, 0), 256);
        assert_eq!(voxel::block_index(0, 1, 0), 16);
        assert_eq!(voxel::block_index(0, 0, 1), 1);
        assert_eq!(voxel::block_index(15, 15, 15), 4095);
    }

    #[test]
    fn test_geometry_constants() {
        assert_eq!(geometry::FULL_FACE_RECT.min, Vec2::ZERO);
        assert_eq!(geometry::FULL_FACE_RECT.max, Vec2::ONE);
        assert_eq!(geometry::EMPTY_FACE_RECT.min, Vec2::ZERO);
        assert_eq!(geometry::EMPTY_FACE_RECT.max, Vec2::ZERO);
    }

    #[test]
    fn test_fluid_constants() {
        assert!((fluid::MAX_FLUID_HEIGHT - 8.0 / 9.0).abs() < 1e-6);
    }
}
