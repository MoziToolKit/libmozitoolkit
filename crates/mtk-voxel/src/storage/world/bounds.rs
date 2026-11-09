//! 3D spatial bounding box management and incremental boundary pruning for VoxelStorage.

use super::container::VoxelStorage;

impl VoxelStorage {
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
}
