use glam::IVec3;

use super::container::VoxelStorage;
use crate::storage::{PaddedVoxelArray, SectionStorage};

impl VoxelStorage {
    /// Builds a 18x18x18 padded array for a target section coordinate.
    pub fn get_section_padded_array(&self, coord: IVec3) -> PaddedVoxelArray {
        let default_sec = SectionStorage::new(coord);
        let sec = self.sections.get(&coord).unwrap_or(&default_sec);

        let sec_wx = coord.x * 16;
        let sec_wy = coord.y * 16;
        let sec_wz = coord.z * 16;

        // Pre-lookup the 27 neighborhood sections to avoid 1,736 HashMap queries
        let mut neighbors = [None; 27];
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                for dz in -1i32..=1 {
                    let n_coord = IVec3::new(coord.x + dx, coord.y + dy, coord.z + dz);
                    let idx = ((dx + 1) * 9 + (dy + 1) * 3 + (dz + 1)) as usize;
                    neighbors[idx] = self.sections.get(&n_coord);
                }
            }
        }

        let has_bounds = self.has_explicit_bounds;
        let mut padded = sec.build_padded_array(|lx, ly, lz| {
            if has_bounds && !self.contains(sec_wx + lx, sec_wy + ly, sec_wz + lz) {
                return "minecraft:air";
            }
            let dx = if lx < 0 {
                -1
            } else if lx >= 16 {
                1
            } else {
                0
            };
            let dy = if ly < 0 {
                -1
            } else if ly >= 16 {
                1
            } else {
                0
            };
            let dz = if lz < 0 {
                -1
            } else if lz >= 16 {
                1
            } else {
                0
            };
            let n_idx = ((dx + 1) * 9 + (dy + 1) * 3 + (dz + 1)) as usize;

            if let Some(n_sec) = neighbors[n_idx] {
                let local_x = (lx & 15) as usize;
                let local_y = (ly & 15) as usize;
                let local_z = (lz & 15) as usize;
                n_sec.get_local_state(local_x, local_y, local_z)
            } else {
                "minecraft:air"
            }
        });

        if self.has_explicit_bounds {
            for lx in 0..16 {
                let wx = sec_wx + lx as i32;
                for ly in 0..16 {
                    let wy = sec_wy + ly as i32;
                    for lz in 0..16 {
                        let wz = sec_wz + lz as i32;
                        if !self.contains(wx, wy, wz) {
                            let p_idx = (lx + 1) * 324 + (ly + 1) * 18 + (lz + 1);
                            padded.padded_voxels[p_idx] = 0;
                        }
                    }
                }
            }

            if !padded.block_entities.is_empty() {
                padded.block_entities.retain(|&local_idx, _| {
                    let lx = ((local_idx >> 8) & 15) as i32;
                    let ly = ((local_idx >> 4) & 15) as i32;
                    let lz = (local_idx & 15) as i32;
                    self.contains(sec_wx + lx, sec_wy + ly, sec_wz + lz)
                });
            }
        }

        let has_biomes = sec.biomes.is_some()
            || self.primary_biome.is_some()
            || !self.biome_column_map.is_empty();

        if !padded.is_empty && has_biomes {
            let (min_x, _, min_z, size_x, _, size_z) = self.get_bounds();
            let has_bounds = size_x > 0 && size_z > 0;
            let min_bound_x = min_x;
            let max_bound_x = min_x + size_x - 1;
            let min_bound_z = min_z;
            let max_bound_z = min_z + size_z - 1;

            // Sample queries must never escape the active selection: an unclamped read
            // would pick up placeholder/out-of-selection biome data at section borders.
            let clamp_x = |bx: i32| -> i32 {
                if has_bounds {
                    bx.clamp(min_bound_x, max_bound_x)
                } else {
                    bx
                }
            };
            let clamp_z = |bz: i32| -> i32 {
                if has_bounds {
                    bz.clamp(min_bound_z, max_bound_z)
                } else {
                    bz
                }
            };

            let is_3d = sec.biomes.as_ref().is_some_and(|b| b.varies_vertically());

            if is_3d {
                let mut biome_cols = Vec::with_capacity(1024);
                for qy in 0..4 {
                    let sample_y = sec_wy + (qy * 4) + 2;
                    for lx in 0..16 {
                        let wx = sec_wx + lx;
                        for lz in 0..16 {
                            let wz = sec_wz + lz;
                            let col = crate::biome::get_smoothed_column_biome(
                                |bx, y, bz| self.get_biome(clamp_x(bx), y, clamp_z(bz)),
                                wx,
                                sample_y,
                                wz,
                            );
                            biome_cols.push(col);
                        }
                    }
                }
                padded.biome_data = Some(biome_cols);
            } else {
                let mut biome_cols = Vec::with_capacity(256);
                let mid_y = sec_wy + 8;
                for lx in 0..16 {
                    let wx = sec_wx + lx;
                    for lz in 0..16 {
                        let wz = sec_wz + lz;

                        // Prefer block height in this column if non-air block exists, else default to mid_y
                        let mut sample_y = mid_y;
                        for ly in (0..16).rev() {
                            let state = sec.get_local_state(lx as usize, ly as usize, lz as usize);
                            if state != "minecraft:air" && !state.starts_with("minecraft:air") {
                                sample_y = sec_wy + ly;
                                break;
                            }
                        }

                        let col = crate::biome::get_smoothed_column_biome(
                            |bx, y, bz| self.get_biome(clamp_x(bx), y, clamp_z(bz)),
                            wx,
                            sample_y,
                            wz,
                        );
                        biome_cols.push(col);
                    }
                }
                padded.biome_data = Some(biome_cols);
            }
        }

        padded
    }
}
