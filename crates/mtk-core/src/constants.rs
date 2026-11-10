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

/// Concurrency, thread pool, and parallel scheduling constants & helpers.
pub mod concurrency {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static GLOBAL_OVERRIDE_CONCURRENCY: AtomicUsize = AtomicUsize::new(0);

    /// Manually sets or injects the host hardware concurrency (e.g. from `navigator.hardwareConcurrency` in WASM/browser).
    /// Pass `0` to reset to automatic detection.
    #[inline]
    pub fn set_hardware_concurrency(count: usize) {
        GLOBAL_OVERRIDE_CONCURRENCY.store(count, Ordering::Relaxed);
    }

    /// Safe hardware concurrency detector that works across Native OS and WebAssembly.
    ///
    /// - If an explicit override has been set via `set_hardware_concurrency`, returns that value.
    /// - On native targets, retrieves `std::thread::available_parallelism()`.
    /// - On `wasm32` without an explicit override, returns `1`.
    #[inline]
    pub fn get_safe_hardware_concurrency() -> usize {
        let override_val = GLOBAL_OVERRIDE_CONCURRENCY.load(Ordering::Relaxed);
        if override_val > 0 {
            return override_val;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        }
        #[cfg(target_arch = "wasm32")]
        {
            1
        }
    }

    /// Conservative thread count calculator: min(max_cap, max(1, available / 2)).
    /// Leaves headroom for host UI (e.g. Blender/Browser main thread) and OS interactivity.
    #[inline]
    pub fn determine_conservative_threads(max_cap: usize) -> usize {
        let available = get_safe_hardware_concurrency();
        (available / 2).clamp(1, max_cap.max(1))
    }

    /// Unified parallel execution helper that bridges native OS threads, Rayon thread pools,
    /// and WebAssembly (wasm-bindgen-rayon / web workers).
    #[inline]
    pub fn execute_parallel<F, R>(num_threads: Option<usize>, work: F) -> Result<R, String>
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        #[cfg(feature = "parallel")]
        {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = num_threads;
                Ok(work())
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(threads) = num_threads {
                    if threads > 0 {
                        let pool = rayon::ThreadPoolBuilder::new()
                            .num_threads(threads)
                            .thread_name(|i| format!("mtk-worker-{}", i))
                            .build()
                            .map_err(|e| e.to_string())?;
                        return Ok(pool.install(work));
                    }
                }
                Ok(work())
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            let _ = num_threads;
            Ok(work())
        }
    }
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

    #[test]
    fn test_concurrency_helpers() {
        concurrency::set_hardware_concurrency(0); // reset
        let conc = concurrency::get_safe_hardware_concurrency();
        assert!(conc >= 1);
        let cons = concurrency::determine_conservative_threads(8);
        assert!((1..=8).contains(&cons));

        // Test manual override
        concurrency::set_hardware_concurrency(16);
        assert_eq!(concurrency::get_safe_hardware_concurrency(), 16);
        assert_eq!(concurrency::determine_conservative_threads(32), 8);

        // Test execute_parallel
        let res = concurrency::execute_parallel(Some(2), || 42 * 2).unwrap();
        assert_eq!(res, 84);

        concurrency::set_hardware_concurrency(0); // cleanup
    }
}
