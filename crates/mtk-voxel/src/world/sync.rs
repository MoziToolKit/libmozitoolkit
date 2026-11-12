//! Storage delta synchronization routines for VoxelWorld.

use std::collections::HashSet;

use glam::IVec3;

use crate::storage::VoxelStorage;

use super::VoxelWorld;

impl VoxelWorld {
    /// Syncs world storage with an updated storage instance and marks only sections
    /// with modified section content (and their immediate boundary neighbors) as dirty.
    /// Returns the number of dirty sections requiring meshing.
    pub fn sync_from_storage(&mut self, new_storage: &VoxelStorage) -> usize {
        let mut dirty = HashSet::new();

        // 1. Check sections removed in new_storage
        for &coord in self.storage.sections.keys() {
            if !new_storage.sections.contains_key(&coord) {
                dirty.insert(coord);
                for offset in [
                    IVec3::new(1, 0, 0),
                    IVec3::new(-1, 0, 0),
                    IVec3::new(0, 1, 0),
                    IVec3::new(0, -1, 0),
                    IVec3::new(0, 0, 1),
                    IVec3::new(0, 0, -1),
                ] {
                    dirty.insert(coord + offset);
                }
            }
        }

        // 2. Check sections added or modified in new_storage
        for (&coord, new_sec) in &new_storage.sections {
            if let Some(old_sec) = self.storage.sections.get_mut(&coord) {
                if old_sec != new_sec {
                    dirty.insert(coord);
                    for offset in [
                        IVec3::new(1, 0, 0),
                        IVec3::new(-1, 0, 0),
                        IVec3::new(0, 1, 0),
                        IVec3::new(0, -1, 0),
                        IVec3::new(0, 0, 1),
                        IVec3::new(0, 0, -1),
                    ] {
                        dirty.insert(coord + offset);
                    }
                    *old_sec = new_sec.clone();
                }
            } else {
                dirty.insert(coord);
                for offset in [
                    IVec3::new(1, 0, 0),
                    IVec3::new(-1, 0, 0),
                    IVec3::new(0, 1, 0),
                    IVec3::new(0, -1, 0),
                    IVec3::new(0, 0, 1),
                    IVec3::new(0, 0, -1),
                ] {
                    dirty.insert(coord + offset);
                }
                self.storage.sections.insert(coord, new_sec.clone());
            }
        }

        // Remove sections from old storage that are missing in new_storage
        self.storage
            .sections
            .retain(|coord, _| new_storage.sections.contains_key(coord));

        // Sync bounds and biomes
        self.storage.min_x = new_storage.min_x;
        self.storage.min_y = new_storage.min_y;
        self.storage.min_z = new_storage.min_z;
        self.storage.size_x = new_storage.size_x;
        self.storage.size_y = new_storage.size_y;
        self.storage.size_z = new_storage.size_z;
        self.storage.has_explicit_bounds = new_storage.has_explicit_bounds;
        self.storage.primary_biome = new_storage.primary_biome.clone();
        self.storage.biome_column_map = new_storage.biome_column_map.clone();
        self.storage.entities = new_storage.entities.clone();

        let count = dirty.len();
        self.storage.dirty_sections = dirty;
        self.sync_selection_bounds();
        count
    }
}
