//! # Deterministic 3D Noise Functions & Terrain Extrusion Heights
//!
//! Provides Perlin and Cellular 3D noise generation and quantized step extrusion height evaluation.

use alloc::vec::Vec;
use core::f32::consts::PI;

use super::types::ExtrudeNoiseType;

/// Simple fast hash function for 3D coordinates.
#[inline]
pub fn hash_3d(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut n = (x.wrapping_mul(374761393)
        ^ y.wrapping_mul(668265263)
        ^ z.wrapping_mul(951214043)
        ^ (seed as i32).wrapping_mul(374761393)) as u32;
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    (n ^ (n >> 16)) as f32 / 4294967295.0
}

/// 3D Perlin Gradient Noise in range [-1.0, 1.0].
pub fn perlin_noise_3d(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let z0 = z.floor() as i32;

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let fz = z - z0 as f32;

    // Smoothstep curve (6t^5 - 15t^4 + 10t^3)
    let u = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let v = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let w = fz * fz * fz * (fz * (fz * 6.0 - 15.0) + 10.0);

    let mut result = 0.0;
    for dx in 0..=1 {
        for dy in 0..=1 {
            for dz in 0..=1 {
                let h = hash_3d(x0 + dx, y0 + dy, z0 + dz, seed) * 2.0 * PI;
                let h2 = hash_3d(x0 + dx, y0 + dy, z0 + dz, seed.wrapping_add(101)) * PI;

                let gx = h.cos() * h2.sin();
                let gy = h.sin() * h2.sin();
                let gz = h2.cos();

                let dot = gx * (fx - dx as f32) + gy * (fy - dy as f32) + gz * (fz - dz as f32);

                let wx = if dx == 0 { 1.0 - u } else { u };
                let wy = if dy == 0 { 1.0 - v } else { v };
                let wz = if dz == 0 { 1.0 - w } else { w };

                result += dot * wx * wy * wz;
            }
        }
    }

    result.clamp(-1.0, 1.0)
}

/// 3D Cellular / Voronoi F1 Noise in range [0.0, 1.0].
#[inline]
pub(crate) fn cellular_noise_3d(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let z0 = z.floor() as i32;

    let mut min_dist_sq = f32::MAX;

    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let cx = x0 + dx;
                let cy = y0 + dy;
                let cz = z0 + dz;

                let px = cx as f32 + hash_3d(cx, cy, cz, seed);
                let py = cy as f32 + hash_3d(cx, cy, cz, seed.wrapping_add(53));
                let pz = cz as f32 + hash_3d(cx, cy, cz, seed.wrapping_add(107));

                let d_x = x - px;
                let d_y = y - py;
                let d_z = z - pz;
                let dist_sq = d_x * d_x + d_y * d_y + d_z * d_z;

                if dist_sq < min_dist_sq {
                    min_dist_sq = dist_sq;
                }
            }
        }
    }

    min_dist_sq.sqrt().clamp(0.0, 1.0)
}

/// Computes discrete extrusion heights for faces according to noise configuration.
pub fn generate_extrude_heights(
    centers: &[[f32; 3]],
    noise_type: ExtrudeNoiseType,
    min_height: f32,
    max_height: f32,
    noise_scale: f32,
    seed: u32,
    discrete_steps: Option<u32>,
) -> Vec<f32> {
    let scale = if noise_scale <= 0.0 { 1.0 } else { noise_scale };
    let height_range = max_height - min_height;

    centers
        .iter()
        .enumerate()
        .map(|(idx, &[x, y, z])| {
            let t = match noise_type {
                ExtrudeNoiseType::UniformRandom => hash_3d(idx as i32, 0, 0, seed),
                ExtrudeNoiseType::Perlin => {
                    (perlin_noise_3d(x * scale, y * scale, z * scale, seed) + 1.0) * 0.5
                }
                ExtrudeNoiseType::Cellular => {
                    cellular_noise_3d(x * scale, y * scale, z * scale, seed)
                }
            };

            let mut h = min_height + t * height_range;
            if let Some(steps) = discrete_steps {
                if steps > 1 {
                    let step_val = height_range / (steps - 1) as f32;
                    let step_idx = ((h - min_height) / step_val).round();
                    h = min_height + step_idx * step_val;
                }
            }
            h
        })
        .collect()
}
