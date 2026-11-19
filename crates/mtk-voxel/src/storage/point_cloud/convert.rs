//! Conversion between `VoxelPointCloud` and `VoxelStorage`.

use std::collections::HashMap;

use glam::{IVec3, Vec3};

use crate::storage::biome::SectionBiomes;
use crate::storage::section::SectionStorage;
use crate::storage::world::VoxelStorage;
use crate::types::MesherConfig;

use super::{palette_index, VoxelPointCloud};

impl VoxelPointCloud {
    /// Extracts an unculled `VoxelPointCloud` from `VoxelStorage`.
    ///
    /// Every non-air block in non-empty sections is preserved as an authoritative 3D point.
    /// No face culling or hidden volume exclusion is performed.
    /// Block entity data (NBT) is extracted into the deduplicated `entity_palette`.
    pub fn extract_from_storage(storage: &VoxelStorage, config: &MesherConfig) -> Self {
        let non_empty = storage.get_all_non_empty_sections();
        if non_empty.is_empty() {
            return Self::new();
        }

        let mut effective_config = config.clone();
        let (bx, by, bz, sx, sy, sz) = storage.get_bounds();
        let has_bounds = sx > 0 && sy > 0 && sz > 0;
        if effective_config.origin_centered
            && effective_config.selection_bounds.is_none()
            && has_bounds
        {
            effective_config.selection_bounds = Some(([bx, by, bz], [sx, sy, sz]));
        }

        // Pre-estimate non-air block count for single reallocation
        let total_est: usize = non_empty
            .iter()
            .filter_map(|c| storage.sections.get(c))
            .map(|sec| sec.non_air_count as usize)
            .sum();

        let mut cloud = Self::with_capacity(total_est);
        if has_bounds {
            cloud.bounds = Some([bx, by, bz, sx, sy, sz]);
        }
        let mut state_map = HashMap::new();
        let mut biome_map = HashMap::new();
        let mut entity_map = HashMap::new();

        for sec_coord in non_empty {
            if let Some(sec) = storage.sections.get(&sec_coord) {
                if sec.is_empty() {
                    continue;
                }
                let base_x = sec_coord.x * 16;
                let base_y = sec_coord.y * 16;
                let base_z = sec_coord.z * 16;

                let (is_fully_inside, is_fully_outside) =
                    if storage.has_explicit_bounds && has_bounds {
                        let b_min_x = bx;
                        let b_max_x = bx + sx - 1;
                        let b_min_y = by;
                        let b_max_y = by + sy - 1;
                        let b_min_z = bz;
                        let b_max_z = bz + sz - 1;

                        let sec_min_x = base_x;
                        let sec_max_x = base_x + 15;
                        let sec_min_y = base_y;
                        let sec_max_y = base_y + 15;
                        let sec_min_z = base_z;
                        let sec_max_z = base_z + 15;

                        let outside = sec_max_x < b_min_x
                            || sec_min_x > b_max_x
                            || sec_max_y < b_min_y
                            || sec_min_y > b_max_y
                            || sec_max_z < b_min_z
                            || sec_min_z > b_max_z;

                        let inside = sec_min_x >= b_min_x
                            && sec_max_x <= b_max_x
                            && sec_min_y >= b_min_y
                            && sec_max_y <= b_max_y
                            && sec_min_z >= b_min_z
                            && sec_max_z <= b_max_z;

                        (inside, outside)
                    } else {
                        (true, false)
                    };

                if is_fully_outside {
                    continue;
                }

                for lx in 0..16 {
                    for ly in 0..16 {
                        for lz in 0..16 {
                            let state = sec.get_local_state(lx, ly, lz);
                            if state.is_empty() || state == "minecraft:air" {
                                continue;
                            }

                            let wx = base_x + lx as i32;
                            let wy = base_y + ly as i32;
                            let wz = base_z + lz as i32;

                            if !is_fully_inside && !storage.contains(wx, wy, wz) {
                                continue;
                            }

                            // Compute center point under active coordinate transform and centering
                            let center =
                                Vec3::new(wx as f32 + 0.5, wy as f32 + 0.5, wz as f32 + 0.5);
                            let transformed = effective_config.transform_position(center);

                            let biome = storage.get_biome(wx, wy, wz);

                            cloud.positions.extend_from_slice(&[
                                transformed.x,
                                transformed.y,
                                transformed.z,
                            ]);
                            cloud.block_x.push(wx);
                            cloud.block_y.push(wy);
                            cloud.block_z.push(wz);
                            cloud.state_indices.push(palette_index(
                                &mut cloud.state_palette,
                                &mut state_map,
                                state,
                            ));
                            cloud.biome_indices.push(palette_index(
                                &mut cloud.biome_palette,
                                &mut biome_map,
                                biome,
                            ));
                            cloud.light_levels.push(0);

                            let local_idx = crate::storage::block_index(lx, ly, lz) as u16;
                            if let Some(entity_data) = sec.get_block_entity(local_idx) {
                                let json_str = entity_data.to_visual_json();
                                let ent_idx = palette_index(
                                    &mut cloud.entity_palette,
                                    &mut entity_map,
                                    &json_str,
                                );
                                if cloud.entity_indices.len() < cloud.len() - 1 {
                                    cloud.entity_indices.resize(cloud.len() - 1, -1);
                                }
                                cloud.entity_indices.push(ent_idx);
                            } else if !cloud.entity_indices.is_empty() {
                                cloud.entity_indices.push(-1);
                            }
                        }
                    }
                }
            }
        }

        if !cloud.entity_palette.is_empty() && cloud.entity_indices.len() < cloud.len() {
            cloud.entity_indices.resize(cloud.len(), -1);
        }

        cloud
    }

    /// Reconstructs a full `VoxelStorage` instance from this point cloud.
    ///
    /// Computes discrete section placements, updates palettes, calculates bounding box,
    /// marks all populated sections as dirty for subsequent meshing, and restores block entities.
    pub fn reconstruct_storage(&self) -> VoxelStorage {
        let mut storage = VoxelStorage::new();
        let count = self.len();
        if count == 0 {
            return storage;
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut min_z = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        let mut max_z = i32::MIN;

        let has_entities = !self.entity_palette.is_empty() && self.entity_indices.len() == count;

        for i in 0..count {
            let bx = self.block_x[i];
            let by = self.block_y[i];
            let bz = self.block_z[i];
            let state = self.state_at(i).unwrap_or("minecraft:air");

            min_x = min_x.min(bx);
            min_y = min_y.min(by);
            min_z = min_z.min(bz);
            max_x = max_x.max(bx);
            max_y = max_y.max(by);
            max_z = max_z.max(bz);

            let sec_coord = IVec3::new(bx.div_euclid(16), by.div_euclid(16), bz.div_euclid(16));
            let lx = (bx & 15) as usize;
            let ly = (by & 15) as usize;
            let lz = (bz & 15) as usize;

            let sec = storage
                .sections
                .entry(sec_coord)
                .or_insert_with(|| SectionStorage::new(sec_coord));
            sec.set_local(lx, ly, lz, state);

            if has_entities {
                let ent_idx = self.entity_indices[i];
                if ent_idx >= 0 {
                    if let Some(json_str) = self.entity_palette.get(ent_idx as usize) {
                        if let Some(entity_data) =
                            mtk_model::BlockEntityData::from_visual_json(json_str)
                        {
                            let local_idx = crate::storage::block_index(lx, ly, lz) as u16;
                            sec.set_block_entity(local_idx, Some(entity_data));
                        }
                    }
                }
            }

            // Rebuild the section-level 4x4x4 quart biomes from the per-point data so the
            // reconstructed storage is structurally identical to save/live-sync storage.
            if let Some(biome) = self.biome_at(i) {
                if !biome.is_empty() {
                    let sb = sec
                        .biomes
                        .get_or_insert_with(|| SectionBiomes::blank(vec![biome.to_string()]));
                    sb.set_sampled_quart_biome((lx >> 2) & 3, (ly >> 2) & 3, (lz >> 2) & 3, biome);
                }
            }
        }

        // Fill quart cells that carry no point with the nearest sampled biome, then derive
        // the shared column map / primary biome through the single authoritative path.
        for sec in storage.sections.values_mut() {
            if let Some(biomes) = sec.biomes.as_mut() {
                biomes.fill_unwritten_nearest();
            }
        }

        // If bounds were recorded on the cloud, preserve exact bounds for origin stability.
        if let Some([bx, by, bz, sx, sy, sz]) = self.bounds {
            storage.set_bounds(bx, by, bz, sx, sy, sz);
        } else if min_x <= max_x && min_y <= max_y && min_z <= max_z {
            storage.set_bounds(
                min_x,
                min_y,
                min_z,
                max_x - min_x + 1,
                max_y - min_y + 1,
                max_z - min_z + 1,
            );
        }

        storage.finalize_biome_metadata();
        storage.mark_all_sections_dirty();
        storage
    }
}
