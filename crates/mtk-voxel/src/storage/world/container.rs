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

    /// Checks if world coordinate `(x, y, z)` falls within the active selection bounds.
    #[inline]
    pub fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        if self.size_x <= 0 || self.size_y <= 0 || self.size_z <= 0 {
            return false;
        }
        x >= self.min_x
            && x < self.min_x + self.size_x
            && y >= self.min_y
            && y < self.min_y + self.size_y
            && z >= self.min_z
            && z < self.min_z + self.size_z
    }

    /// Returns active or computed 3D world bounding box `(min_x, min_y, min_z, size_x, size_y, size_z)`.
    pub fn get_bounds(&self) -> (i32, i32, i32, i32, i32, i32) {
        if self.size_x > 0 && self.size_y > 0 && self.size_z > 0 {
            (
                self.min_x,
                self.min_y,
                self.min_z,
                self.size_x,
                self.size_y,
                self.size_z,
            )
        } else {
            let non_empty = self.get_all_non_empty_sections();
            if non_empty.is_empty() {
                return (0, 0, 0, 0, 0, 0);
            }
            let mut min_sec = non_empty[0];
            let mut max_sec = non_empty[0];
            for &c in &non_empty[1..] {
                min_sec = min_sec.min(c);
                max_sec = max_sec.max(c);
            }
            let min_x = min_sec.x * 16;
            let min_y = min_sec.y * 16;
            let min_z = min_sec.z * 16;
            let size_x = (max_sec.x - min_sec.x + 1) * 16;
            let size_y = (max_sec.y - min_sec.y + 1) * 16;
            let size_z = (max_sec.z - min_sec.z + 1) * 16;
            (min_x, min_y, min_z, size_x, size_y, size_z)
        }
    }

    /// Returns the selection bounding box if size is positive, in `([min_x, min_y, min_z], [size_x, size_y, size_z])` format.
    #[inline]
    pub fn default_selection_bounds(&self) -> Option<([i32; 3], [i32; 3])> {
        let (min_x, min_y, min_z, sz_x, sz_y, sz_z) = self.get_bounds();
        if sz_x > 0 && sz_y > 0 && sz_z > 0 {
            Some(([min_x, min_y, min_z], [sz_x, sz_y, sz_z]))
        } else {
            None
        }
    }

    /// Updates selection bounding box with incremental pruning and boundary seam dirtying.
    pub fn set_bounds(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
    ) -> bool {
        let old_bounds = (
            self.min_x,
            self.min_y,
            self.min_z,
            self.size_x,
            self.size_y,
            self.size_z,
        );
        let new_bounds = (min_x, min_y, min_z, size_x, size_y, size_z);
        if old_bounds == new_bounds {
            return false;
        }

        let old_has_data =
            self.size_x > 0 && self.size_y > 0 && self.size_z > 0 && !self.sections.is_empty();

        if !old_has_data || size_x <= 0 || size_y <= 0 || size_z <= 0 {
            if self.size_x > 0 {
                self.clear();
            }
            self.min_x = min_x;
            self.min_y = min_y;
            self.min_z = min_z;
            self.size_x = size_x;
            self.size_y = size_y;
            self.size_z = size_z;
            self.has_explicit_bounds = size_x > 0 && size_y > 0 && size_z > 0;
            self.advance_generation();
            return true;
        }

        // Calculate 3D overlap between old and new bounds
        let inter_min_x = self.min_x.max(min_x);
        let inter_max_x = (self.min_x + self.size_x - 1).min(min_x + size_x - 1);
        let inter_min_y = self.min_y.max(min_y);
        let inter_max_y = (self.min_y + self.size_y - 1).min(min_y + size_y - 1);
        let inter_min_z = self.min_z.max(min_z);
        let inter_max_z = (self.min_z + self.size_z - 1).min(min_z + size_z - 1);

        let has_overlap =
            inter_min_x <= inter_max_x && inter_min_y <= inter_max_y && inter_min_z <= inter_max_z;

        if !has_overlap {
            self.clear();
            self.min_x = min_x;
            self.min_y = min_y;
            self.min_z = min_z;
            self.size_x = size_x;
            self.size_y = size_y;
            self.size_z = size_z;
            self.has_explicit_bounds = size_x > 0 && size_y > 0 && size_z > 0;
            self.advance_generation();
            return true;
        }

        let new_max_x = min_x + size_x - 1;
        let new_max_y = min_y + size_y - 1;
        let new_max_z = min_z + size_z - 1;

        let new_min_sec_x = min_x >> 4;
        let new_max_sec_x = new_max_x >> 4;
        let new_min_sec_y = min_y >> 4;
        let new_max_sec_y = new_max_y >> 4;
        let new_min_sec_z = min_z >> 4;
        let new_max_sec_z = new_max_z >> 4;

        // Prune out-of-bounds sections
        self.sections.retain(|coord, _| {
            coord.x >= new_min_sec_x
                && coord.x <= new_max_sec_x
                && coord.y >= new_min_sec_y
                && coord.y <= new_max_sec_y
                && coord.z >= new_min_sec_z
                && coord.z <= new_max_sec_z
        });

        // Prune out-of-bounds column biomes
        self.biome_column_map
            .retain(|&[x, z], _| x >= min_x && x <= new_max_x && z >= min_z && z <= new_max_z);

        // Prune out-of-bounds known empty sections

        self.known_empty_sections.retain(|coord| {
            coord.x >= new_min_sec_x
                && coord.x <= new_max_sec_x
                && coord.y >= new_min_sec_y
                && coord.y <= new_max_sec_y
                && coord.z >= new_min_sec_z
                && coord.z <= new_max_sec_z
        });

        self.min_x = min_x;
        self.min_y = min_y;
        self.min_z = min_z;
        self.size_x = size_x;
        self.size_y = size_y;
        self.size_z = size_z;
        self.has_explicit_bounds = true;
        self.advance_generation();

        // Clear out-of-bounds voxels in boundary sections and mark dirty
        for (&coord, sec) in self.sections.iter_mut() {
            let is_boundary = coord.x == new_min_sec_x
                || coord.x == new_max_sec_x
                || coord.y == new_min_sec_y
                || coord.y == new_max_sec_y
                || coord.z == new_min_sec_z
                || coord.z == new_max_sec_z;
            if is_boundary {
                self.dirty_sections.insert(coord);
                let base_x = coord.x << 4;
                let base_y = coord.y << 4;
                let base_z = coord.z << 4;
                for lx in 0..16 {
                    let wx = base_x + lx as i32;
                    for ly in 0..16 {
                        let wy = base_y + ly as i32;
                        for lz in 0..16 {
                            let wz = base_z + lz as i32;
                            if wx < min_x
                                || wx > new_max_x
                                || wy < min_y
                                || wy > new_max_y
                                || wz < min_z
                                || wz > new_max_z
                            {
                                sec.set_local(lx, ly, lz, "minecraft:air");
                            }
                        }
                    }
                }
            }
        }

        true
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

        let changed = sec.set_local(lx, ly, lz, state);
        if changed {
            self.dirty_sections.insert(sec_coord);
            self.known_empty_sections.remove(&sec_coord);

            let has_bounds = self.size_x > 0 && self.size_y > 0 && self.size_z > 0;
            let max_x = self.min_x + self.size_x - 1;
            let max_y = self.min_y + self.size_y - 1;
            let max_z = self.min_z + self.size_z - 1;

            // Check boundary neighbors and mark dirty for cross-section face culling
            if (x & 15) == 0 && (!has_bounds || x > self.min_x) {
                self.dirty_sections.insert(sec_coord + IVec3::new(-1, 0, 0));
            } else if (x & 15) == 15 && (!has_bounds || x < max_x) {
                self.dirty_sections.insert(sec_coord + IVec3::new(1, 0, 0));
            }
            if (y & 15) == 0 && (!has_bounds || y > self.min_y) {
                self.dirty_sections.insert(sec_coord + IVec3::new(0, -1, 0));
            } else if (y & 15) == 15 && (!has_bounds || y < max_y) {
                self.dirty_sections.insert(sec_coord + IVec3::new(0, 1, 0));
            }
            if (z & 15) == 0 && (!has_bounds || z > self.min_z) {
                self.dirty_sections.insert(sec_coord + IVec3::new(0, 0, -1));
            } else if (z & 15) == 15 && (!has_bounds || z < max_z) {
                self.dirty_sections.insert(sec_coord + IVec3::new(0, 0, 1));
            }

            self.advance_generation();
        }

        changed
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
