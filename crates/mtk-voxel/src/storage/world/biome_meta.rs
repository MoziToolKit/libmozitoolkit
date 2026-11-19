//! # Derived Biome Metadata Finalization
//!
//! Single source of truth for the biome metadata that is **derived** from the
//! section-level 4x4x4 quart biome data ([`SectionBiomes`]):
//!
//! - `biome_column_map`: the surface biome of every block column inside the active bounds.
//! - `primary_biome`: the dominant surface biome, used as a last-resort fallback.
//!
//! Every ingestion path (Anvil save loader, Live Sync baseline assembler, point-cloud
//! reconstruction) must funnel through [`VoxelStorage::finalize_biome_metadata`] instead of
//! re-implementing the derivation locally. That is what keeps save imports and live sync
//! byte-compatible.
//!
//! [`SectionBiomes`]: crate::storage::SectionBiomes

use std::collections::HashMap;

use super::container::VoxelStorage;

impl VoxelStorage {
    /// Rebuilds `biome_column_map` and `primary_biome` from the section-level quart biomes.
    ///
    /// The surface biome of a column is the biome of the highest section in that column that
    /// carries biome data, sampled at the top quart layer (`qy = 3`). This matches both the
    /// Anvil section layout and the live-sync selection grid, so the same world produces the
    /// same column map regardless of the provider.
    ///
    /// Cost: one pass over the sections (`O(sections * 16)`) plus one pass over the bounded
    /// columns. The previous implementation re-walked every column's section stack, which made a
    /// 576x576 selection perform ~8M section lookups (38 ms measured) for the same answer.
    pub fn finalize_biome_metadata(&mut self) {
        let (min_x, min_y, min_z, size_x, size_y, size_z) = self.get_bounds();
        if size_x <= 0 || size_y <= 0 || size_z <= 0 {
            self.biome_column_map.clear();
            self.primary_biome = None;
            return;
        }

        let (column_map, primary) =
            self.derive_biome_columns(min_x, min_y, min_z, size_x, size_y, size_z);
        self.biome_column_map = column_map;
        self.primary_biome = primary;
    }

    fn derive_biome_columns(
        &self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
    ) -> (HashMap<[i32; 2], String>, Option<String>) {
        let max_x = min_x + size_x - 1;
        let max_z = min_z + size_z - 1;
        let min_sec_y = min_y >> 4;
        let max_sec_y = (min_y + size_y - 1) >> 4;

        let width = size_x as usize;
        let depth = size_z as usize;

        // Dense per-column winner: highest contributing section y plus an interned biome index.
        // 6 bytes per column, i.e. a fraction of the `String`-valued column map built below, so
        // this does not change the memory profile of a selection.
        const UNSET: u16 = u16::MAX;
        let mut surface_y = vec![i32::MIN; width * depth];
        let mut surface_biome = vec![UNSET; width * depth];
        let mut names: Vec<&str> = Vec::with_capacity(16);

        for (coord, section) in &self.sections {
            let Some(biomes) = section.biomes.as_ref() else {
                continue;
            };
            if biomes.palette.is_empty() {
                continue;
            }
            if coord.y < min_sec_y || coord.y > max_sec_y {
                continue;
            }
            let base_x = coord.x * 16;
            let base_z = coord.z * 16;

            for qx in 0..4usize {
                let x0 = base_x + (qx as i32) * 4;
                if x0 + 3 < min_x || x0 > max_x {
                    continue;
                }
                for qz in 0..4usize {
                    let z0 = base_z + (qz as i32) * 4;
                    if z0 + 3 < min_z || z0 > max_z {
                        continue;
                    }
                    let Some(biome) = biomes.get_quart_biome(qx, 3, qz) else {
                        continue;
                    };
                    let biome_idx = match names.iter().position(|known| *known == biome) {
                        Some(index) => index as u16,
                        None => {
                            names.push(biome);
                            (names.len() - 1) as u16
                        }
                    };

                    for x in x0.max(min_x)..=(x0 + 3).min(max_x) {
                        let row = (x - min_x) as usize * depth;
                        for z in z0.max(min_z)..=(z0 + 3).min(max_z) {
                            let index = row + (z - min_z) as usize;
                            if coord.y > surface_y[index] {
                                surface_y[index] = coord.y;
                                surface_biome[index] = biome_idx;
                            }
                        }
                    }
                }
            }
        }

        let mut column_map: HashMap<[i32; 2], String> =
            HashMap::with_capacity((width * depth).min(1 << 20));
        let mut counts: HashMap<&str, u64> = HashMap::with_capacity(names.len() + 1);

        for x_index in 0..width {
            let x = min_x + x_index as i32;
            let row = x_index * depth;
            for z_index in 0..depth {
                let index = row + z_index;
                let biome = match surface_biome[index] {
                    UNSET => "minecraft:plains",
                    named => names[named as usize],
                };
                *counts.entry(biome).or_insert(0) += 1;
                column_map.insert([x, min_z + z_index as i32], biome.to_string());
            }
        }

        // Deterministic dominant biome: highest column count, ties broken by the
        // lexicographically smallest biome id so the result never depends on hash order.
        let primary = counts
            .into_iter()
            .max_by(|(name_a, count_a), (name_b, count_b)| {
                count_a.cmp(count_b).then_with(|| name_b.cmp(name_a))
            })
            .map(|(name, _)| name.to_string());

        (column_map, primary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::VoxelStorage;
    use glam::IVec3;

    /// The per-column implementation that `derive_biome_columns` replaced. Kept as an
    /// independent oracle: it walks each column's section stack from the top down, which is the
    /// definition the new single-pass version has to reproduce exactly.
    fn reference(storage: &VoxelStorage) -> (HashMap<[i32; 2], String>, Option<String>) {
        let (min_x, min_y, min_z, size_x, size_y, size_z) = storage.get_bounds();
        let mut column_map: HashMap<[i32; 2], String> = HashMap::new();
        let mut counts: HashMap<String, u64> = HashMap::new();
        if size_x <= 0 || size_y <= 0 || size_z <= 0 {
            return (column_map, None);
        }
        let max_x = min_x + size_x - 1;
        let max_z = min_z + size_z - 1;
        let min_sec_y = min_y >> 4;
        let max_sec_y = (min_y + size_y - 1) >> 4;

        for x in min_x..=max_x {
            let sx = x >> 4;
            let qx = ((x & 15) >> 2) as usize;
            for z in min_z..=max_z {
                let sz = z >> 4;
                let qz = ((z & 15) >> 2) as usize;
                let mut surface: Option<&str> = None;
                for sy in (min_sec_y..=max_sec_y).rev() {
                    let Some(sec) = storage.sections.get(&IVec3::new(sx, sy, sz)) else {
                        continue;
                    };
                    let Some(biomes) = sec.biomes.as_ref() else {
                        continue;
                    };
                    if biomes.palette.is_empty() {
                        continue;
                    }
                    if let Some(biome) = biomes.get_quart_biome(qx, 3, qz) {
                        surface = Some(biome);
                        break;
                    }
                }
                let biome = surface.unwrap_or("minecraft:plains");
                *counts.entry(biome.to_string()).or_insert(0) += 1;
                column_map.insert([x, z], biome.to_string());
            }
        }

        let primary = counts
            .into_iter()
            .max_by(|(name_a, count_a), (name_b, count_b)| {
                count_a.cmp(count_b).then_with(|| name_b.cmp(name_a))
            })
            .map(|(name, _)| name);
        (column_map, primary)
    }

    fn assert_matches_reference(storage: &VoxelStorage) {
        let (expected_map, expected_primary) = reference(storage);
        let mut actual = storage.clone();
        actual.finalize_biome_metadata();
        assert_eq!(
            actual.primary_biome, expected_primary,
            "primary biome drifted"
        );
        assert_eq!(
            actual.biome_column_map,
            expected_map,
            "column map drifted ({} columns)",
            expected_map.len()
        );
    }

    fn biome_for(x: i32, z: i32) -> &'static str {
        match (x / 8 + z / 8).rem_euclid(3) {
            0 => "minecraft:plains",
            1 => "minecraft:desert",
            _ => "minecraft:taiga",
        }
    }

    #[test]
    fn matches_reference_with_mixed_height_and_missing_sections() {
        let mut storage = VoxelStorage::new();
        storage.set_bounds(0, 0, 0, 48, 48, 48);
        for x in 0..48 {
            for z in 0..48 {
                // Every fifth column carries no section biome at all -> plains fallback.
                if (x + z) % 5 == 0 {
                    continue;
                }
                storage.set_block(x, 14, z, "minecraft:stone", Some(biome_for(x, z)));
            }
        }
        // A higher section for one corner: its biome must win for those columns.
        for x in 0..16 {
            for z in 0..16 {
                storage.set_block(x, 30, z, "minecraft:stone", Some("minecraft:snowy_plains"));
            }
        }
        // A section whose top quart has no biome data (contributes nothing).
        for x in 32..48 {
            for z in 32..48 {
                storage.set_block(x, 1, z, "minecraft:stone", Some("minecraft:badlands"));
            }
        }
        assert_matches_reference(&storage);
    }

    #[test]
    fn matches_reference_when_bounds_clip_the_loaded_data() {
        let mut storage = VoxelStorage::new();
        storage.set_bounds(-20, 0, -20, 64, 32, 64);
        for x in -20..44 {
            for z in -20..44 {
                storage.set_block(x, 14, z, "minecraft:stone", Some(biome_for(x, z)));
            }
        }
        // Section coordinates are not chunk aligned with these bounds.
        storage.set_bounds(3, 0, -7, 30, 32, 19);
        assert_matches_reference(&storage);
    }

    #[test]
    fn matches_reference_without_explicit_bounds() {
        let mut storage = VoxelStorage::new();
        storage.set_block(0, 14, 0, "minecraft:stone", Some("minecraft:desert"));
        assert_matches_reference(&storage);
    }
}
