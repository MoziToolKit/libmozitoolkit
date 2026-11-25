use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::{HashMap, HashSet};

use glam::IVec3;
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::storage::{SectionBiomes, SectionStorage, VoxelPointCloud};
use crate::types::MesherConfig;

#[cfg(feature = "serde")]
mod serde_atomic_u64 {
    use super::*;

    pub fn serialize<S>(val: &AtomicU64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(val.load(Ordering::Relaxed))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<AtomicU64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = u64::deserialize(deserializer)?;
        Ok(AtomicU64::new(v))
    }
}

/// 3D sparse world voxel container managing multiple 16x16x16 chunk sections,
/// delta synchronization, and boundary dirty tracking.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VoxelStorage {
    /// Minimum world bounding coordinate (X).
    pub min_x: i32,
    /// Minimum world bounding coordinate (Y).
    pub min_y: i32,
    /// Minimum world bounding coordinate (Z).
    pub min_z: i32,
    /// Selection box size along X axis.
    pub size_x: i32,
    /// Selection box size along Y axis.
    pub size_y: i32,
    /// Selection box size along Z axis.
    pub size_z: i32,
    /// Sparse map of section coordinates `(sx, sy, sz)` to `SectionStorage`.
    pub sections: HashMap<IVec3, SectionStorage>,
    /// 2D column biome cache: surface biome per block column `(x, z)`.
    pub biome_column_map: HashMap<[i32; 2], String>,
    /// Default primary biome when unspecified.
    pub primary_biome: Option<String>,
    /// Set of section coordinates that have been modified and require remeshing.
    pub dirty_sections: HashSet<IVec3>,
    /// Set of section coordinates known to be completely empty.
    pub known_empty_sections: HashSet<IVec3>,
    /// Incremental generation counter (Atomic for lock-free multi-thread sync).
    #[cfg_attr(feature = "serde", serde(with = "serde_atomic_u64"))]
    pub generation: AtomicU64,
    /// Indicates whether bounds were explicitly established (e.g. by set_bounds / set_full_snapshot).
    pub has_explicit_bounds: bool,
}

impl Clone for VoxelStorage {
    fn clone(&self) -> Self {
        Self {
            min_x: self.min_x,
            min_y: self.min_y,
            min_z: self.min_z,
            size_x: self.size_x,
            size_y: self.size_y,
            size_z: self.size_z,
            sections: self.sections.clone(),
            biome_column_map: self.biome_column_map.clone(),
            primary_biome: self.primary_biome.clone(),
            dirty_sections: self.dirty_sections.clone(),
            known_empty_sections: self.known_empty_sections.clone(),
            generation: AtomicU64::new(self.generation.load(Ordering::Relaxed)),
            has_explicit_bounds: self.has_explicit_bounds,
        }
    }
}

impl Default for VoxelStorage {
    fn default() -> Self {
        Self {
            min_x: 0,
            min_y: 0,
            min_z: 0,
            size_x: 0,
            size_y: 0,
            size_z: 0,
            sections: HashMap::new(),
            biome_column_map: HashMap::new(),
            primary_biome: None,
            dirty_sections: HashSet::new(),
            known_empty_sections: HashSet::new(),
            generation: AtomicU64::new(0),
            has_explicit_bounds: false,
        }
    }
}

impl VoxelStorage {
    /// Creates a new empty `VoxelStorage`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets current generation counter value atomically.
    #[inline]
    pub fn get_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Advances the generation counter atomically and returns new value.
    #[inline]
    pub fn advance_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Sets the generation counter value atomically.
    #[inline]
    pub fn set_generation(&self, val: u64) {
        self.generation.store(val, Ordering::Release);
    }

    /// Clears all sections, biomes, and bounds.
    pub fn clear(&mut self) {
        self.sections.clear();
        self.biome_column_map.clear();
        self.dirty_sections.clear();
        self.known_empty_sections.clear();
        self.primary_biome = None;
        self.min_x = 0;
        self.min_y = 0;
        self.min_z = 0;
        self.size_x = 0;
        self.size_y = 0;
        self.size_z = 0;
        self.has_explicit_bounds = false;
        self.advance_generation();
    }

    /// Explicit bounding box `[min_x, min_y, min_z, size_x, size_y, size_z]`, if configured.
    #[inline]
    pub fn bounds(&self) -> Option<[i32; 6]> {
        if self.has_explicit_bounds {
            Some([
                self.min_x,
                self.min_y,
                self.min_z,
                self.size_x,
                self.size_y,
                self.size_z,
            ])
        } else {
            None
        }
    }

    /// Gets the blockstate string at world coordinate `(x, y, z)`.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> &str {
        if self.has_explicit_bounds && !self.contains(x, y, z) {
            return "minecraft:air";
        }
        let sec_coord = IVec3::new(x >> 4, y >> 4, z >> 4);
        if let Some(sec) = self.sections.get(&sec_coord) {
            let lx = (x & 15) as usize;
            let ly = (y & 15) as usize;
            let lz = (z & 15) as usize;
            sec.get_local_state(lx, ly, lz)
        } else {
            "minecraft:air"
        }
    }

    /// Gets the block entity at world coordinate `(x, y, z)`, if present.
    pub fn get_block_entity(&self, x: i32, y: i32, z: i32) -> Option<&mtk_model::BlockEntityData> {
        let sec_coord = IVec3::new(x >> 4, y >> 4, z >> 4);
        if let Some(sec) = self.sections.get(&sec_coord) {
            let lx = (x & 15) as usize;
            let ly = (y & 15) as usize;
            let lz = (z & 15) as usize;
            let local_idx = crate::storage::block_index(lx, ly, lz) as u16;
            sec.get_block_entity(local_idx)
        } else {
            None
        }
    }

    /// Sets or removes the block entity at world coordinate `(x, y, z)`.
    pub fn set_block_entity(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        data: Option<mtk_model::BlockEntityData>,
    ) {
        let sec_coord = IVec3::new(x >> 4, y >> 4, z >> 4);
        let lx = (x & 15) as usize;
        let ly = (y & 15) as usize;
        let lz = (z & 15) as usize;
        let local_idx = crate::storage::block_index(lx, ly, lz) as u16;

        if let Some(sec) = self.sections.get_mut(&sec_coord) {
            sec.set_block_entity(local_idx, data);
        } else if data.is_some() {
            let mut sec = SectionStorage::new(sec_coord);
            sec.set_block_entity(local_idx, data);
            self.sections.insert(sec_coord, sec);
        }
    }

    /// Sets the blockstate at world coordinate `(x, y, z)` and marks dirty sections.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state: &str, biome: Option<&str>) -> bool {
        let sec_coord = IVec3::new(x >> 4, y >> 4, z >> 4);
        let lx = (x & 15) as usize;
        let ly = (y & 15) as usize;
        let lz = (z & 15) as usize;

        let sec = self
            .sections
            .entry(sec_coord)
            .or_insert_with(|| SectionStorage::new(sec_coord));

        if let Some(b) = biome {
            self.biome_column_map.insert([x, z], b.to_string());
            // Use a non-authoritative placeholder: `set_biome` below propagates the biome
            // to the whole column unless individual quarts were explicitly claimed.
            let sec_biomes = sec
                .biomes
                .get_or_insert_with(|| SectionBiomes::blank(vec![b.to_string()]));
            sec_biomes.set_biome(lx, ly, lz, b);
        }

        let state_changed = sec.set_local(lx, ly, lz, state);
        // A biome-only edit changes the baked tint even when the blockstate is unchanged, so
        // dirtiness must track both.
        let biome_changed = biome.is_some();
        if state_changed || biome_changed {
            self.dirty_sections.insert(sec_coord);
            self.known_empty_sections.remove(&sec_coord);

            // The mesher samples a full 3x3x3 neighbourhood (18^3 padded array), so an edit on
            // a section border also invalidates every diagonal neighbour, not just the six
            // orthogonal ones. Each axis contributes -1/0/+1 depending on the local coordinate.
            let x_offsets: &[i32] = if lx == 0 {
                &[-1, 0]
            } else if lx == 15 {
                &[0, 1]
            } else {
                &[0]
            };
            let y_offsets: &[i32] = if ly == 0 {
                &[-1, 0]
            } else if ly == 15 {
                &[0, 1]
            } else {
                &[0]
            };
            let z_offsets: &[i32] = if lz == 0 {
                &[-1, 0]
            } else if lz == 15 {
                &[0, 1]
            } else {
                &[0]
            };
            for &dx in x_offsets {
                for &dy in y_offsets {
                    for &dz in z_offsets {
                        if dx != 0 || dy != 0 || dz != 0 {
                            self.dirty_sections
                                .insert(sec_coord + IVec3::new(dx, dy, dz));
                        }
                    }
                }
            }

            self.advance_generation();
        }

        state_changed
    }

    /// Returns the biome registry identifier for block coordinate `(x, y, z)`.
    pub fn get_biome(&self, x: i32, y: i32, z: i32) -> &str {
        let sec_coord = IVec3::new(x >> 4, y >> 4, z >> 4);
        if let Some(sec) = self.sections.get(&sec_coord) {
            if let Some(ref biomes) = sec.biomes {
                let lx = (x & 15) as usize;
                let ly = (y & 15) as usize;
                let lz = (z & 15) as usize;
                if let Some(b) = biomes.get_biome(lx, ly, lz) {
                    return b;
                }
            }
        }
        if let Some(b) = self.biome_column_map.get(&[x, z]) {
            return b;
        }
        // If coordinate is outside active selection bounds, horizontally clamp to nearest valid column
        if self.size_x > 0 && self.size_z > 0 {
            let cx = x.clamp(self.min_x, self.min_x + self.size_x - 1);
            let cz = z.clamp(self.min_z, self.min_z + self.size_z - 1);
            if cx != x || cz != z {
                let c_sec_coord = IVec3::new(cx >> 4, y >> 4, cz >> 4);
                if let Some(sec) = self.sections.get(&c_sec_coord) {
                    if let Some(ref biomes) = sec.biomes {
                        let clx = (cx & 15) as usize;
                        let cly = (y & 15) as usize;
                        let clz = (cz & 15) as usize;
                        if let Some(b) = biomes.get_biome(clx, cly, clz) {
                            return b;
                        }
                    }
                }
                if let Some(b) = self.biome_column_map.get(&[cx, cz]) {
                    return b;
                }
            }
        }
        if let Some(ref pb) = self.primary_biome {
            return pb;
        }
        "minecraft:plains"
    }

    /// Marks all existing sections dirty for a full rebuild.
    pub fn mark_all_sections_dirty(&mut self) {
        self.dirty_sections.clear();
        for coord in self.sections.keys() {
            self.dirty_sections.insert(*coord);
        }
    }

    /// Clears the dirty sections set after meshing.
    pub fn clear_dirty_sections(&mut self) {
        self.dirty_sections.clear();
    }

    /// Returns a list of all non-empty section coordinates currently loaded.
    pub fn get_all_non_empty_sections(&self) -> Vec<IVec3> {
        self.sections
            .iter()
            .filter(|(_, sec)| !sec.is_empty())
            .map(|(coord, _)| *coord)
            .collect()
    }

    /// Extracts an unculled point cloud containing every populated voxel point.
    pub fn to_point_cloud(&self, config: &MesherConfig) -> VoxelPointCloud {
        VoxelPointCloud::extract_from_storage(self, config)
    }

    /// Reconstructs a `VoxelStorage` from an unculled point cloud.
    pub fn from_point_cloud(point_cloud: &VoxelPointCloud) -> Self {
        point_cloud.reconstruct_storage()
    }

    /// Replaces current storage state by reconstructing from a point cloud.
    pub fn load_from_point_cloud(&mut self, point_cloud: &VoxelPointCloud) {
        *self = point_cloud.reconstruct_storage();
    }
}

impl crate::source::VoxelReader for VoxelStorage {
    fn get_block(&self, x: i32, y: i32, z: i32) -> &str {
        VoxelStorage::get_block(self, x, y, z)
    }

    fn get_section(&self, sx: i32, sy: i32, sz: i32) -> Option<&SectionStorage> {
        self.sections.get(&IVec3::new(sx, sy, sz))
    }

    fn contains_section(&self, sx: i32, sy: i32, sz: i32) -> bool {
        self.sections.contains_key(&IVec3::new(sx, sy, sz))
    }

    fn block_bounds(&self) -> Option<(IVec3, IVec3)> {
        if self.size_x > 0 && self.size_y > 0 && self.size_z > 0 {
            Some((
                IVec3::new(self.min_x, self.min_y, self.min_z),
                IVec3::new(
                    self.min_x + self.size_x,
                    self.min_y + self.size_y,
                    self.min_z + self.size_z,
                ),
            ))
        } else {
            None
        }
    }

    fn get_biome(&self, x: i32, y: i32, z: i32) -> &str {
        VoxelStorage::get_biome(self, x, y, z)
    }
}

impl VoxelStorage {
    /// Counts non-air voxels strictly inside `[min_block, max_block]` (inclusive).
    pub fn count_non_air_in_bounds(&self, min_block: IVec3, max_block: IVec3) -> usize {
        let true_min = min_block.min(max_block);
        let true_max = min_block.max(max_block);
        let mut count = 0usize;
        for (&coord, section) in &self.sections {
            if section.non_air_count == 0 {
                continue;
            }
            let base = coord * 16;
            let sec_max = base + IVec3::splat(15);
            if sec_max.x < true_min.x
                || base.x > true_max.x
                || sec_max.y < true_min.y
                || base.y > true_max.y
                || sec_max.z < true_min.z
                || base.z > true_max.z
            {
                continue;
            }
            if base.x >= true_min.x
                && sec_max.x <= true_max.x
                && base.y >= true_min.y
                && sec_max.y <= true_max.y
                && base.z >= true_min.z
                && sec_max.z <= true_max.z
            {
                count += section.non_air_count as usize;
                continue;
            }
            let min_lx = (true_min.x - base.x).clamp(0, 15) as usize;
            let max_lx = (true_max.x - base.x).clamp(0, 15) as usize;
            let min_ly = (true_min.y - base.y).clamp(0, 15) as usize;
            let max_ly = (true_max.y - base.y).clamp(0, 15) as usize;
            let min_lz = (true_min.z - base.z).clamp(0, 15) as usize;
            let max_lz = (true_max.z - base.z).clamp(0, 15) as usize;
            for x in min_lx..=max_lx {
                for y in min_ly..=max_ly {
                    for z in min_lz..=max_lz {
                        let idx = crate::storage::block_index(x, y, z);
                        if section.voxels[idx] != 0 {
                            count += 1;
                        }
                    }
                }
            }
        }
        count
    }

    /// Scans all chunk section palettes for unknown blockstates lacking both models and textures.
    pub fn scan_unknown_blocks(
        &self,
        model_db: Option<&mtk_model::BakedModelDatabase>,
        atlas: Option<&mtk_texture::AtlasAddressMap>,
        custom_aliases: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Vec<String> {
        let unique_states: std::collections::HashSet<&str> = self
            .sections
            .values()
            .flat_map(|sec| sec.palette.iter().map(|s| s.as_str()))
            .collect();
        crate::storage::scan_unknown_blockstates(unique_states, model_db, atlas, custom_aliases)
    }
}

impl crate::source::VoxelWriter for VoxelStorage {
    fn set_block(&mut self, x: i32, y: i32, z: i32, state: &str, biome: Option<&str>) {
        VoxelStorage::set_block(self, x, y, z, state, biome);
    }

    fn set_section(&mut self, sx: i32, sy: i32, sz: i32, section: SectionStorage) {
        let coord = IVec3::new(sx, sy, sz);
        self.sections.insert(coord, section);
        self.dirty_sections.insert(coord);
        self.known_empty_sections.remove(&coord);
    }

    fn mark_section_dirty(&mut self, sx: i32, sy: i32, sz: i32) {
        self.dirty_sections.insert(IVec3::new(sx, sy, sz));
    }

    fn set_bounds(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
    ) {
        VoxelStorage::set_bounds(self, min_x, min_y, min_z, size_x, size_y, size_z);
    }

    fn clear(&mut self) {
        VoxelStorage::clear(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_only_change_still_marks_the_section_dirty() {
        let mut storage = VoxelStorage::new();
        storage.set_block(3, 64, 3, "minecraft:grass_block", Some("minecraft:plains"));
        storage.clear_dirty_sections();

        // Same blockstate, different biome: the baked tint changes, so the section must be
        // remeshed even though `set_local` reports "unchanged".
        let changed =
            storage.set_block(3, 64, 3, "minecraft:grass_block", Some("minecraft:desert"));

        assert!(!changed, "blockstate is identical");
        assert_eq!(storage.get_biome(3, 64, 3), "minecraft:desert");
        assert!(
            storage.dirty_sections.contains(&IVec3::new(0, 4, 0)),
            "biome-only edit must dirty the section, got {:?}",
            storage.dirty_sections
        );
    }

    #[test]
    fn corner_edit_dirties_every_diagonal_neighbour() {
        let mut storage = VoxelStorage::new();
        // Local (0,0,0) of section (1,1,1) sits on the shared corner of 8 sections: the
        // mesher samples all of them through the 18^3 padded array.
        storage.set_block(16, 16, 16, "minecraft:stone", None);
        storage.clear_dirty_sections();

        storage.set_block(16, 16, 16, "minecraft:dirt", None);

        let expected: Vec<IVec3> = (-1..=0)
            .flat_map(|dx| {
                (-1..=0)
                    .flat_map(move |dy| (-1..=0).map(move |dz| IVec3::new(1 + dx, 1 + dy, 1 + dz)))
            })
            .filter(|c| *c != IVec3::new(1, 1, 1))
            .collect();

        assert_eq!(expected.len(), 7);
        for coord in expected {
            assert!(
                storage.dirty_sections.contains(&coord),
                "corner edit must dirty neighbour {coord:?}, got {:?}",
                storage.dirty_sections
            );
        }
    }
}
