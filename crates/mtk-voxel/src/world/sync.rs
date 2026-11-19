//! Storage delta synchronization routines for VoxelWorld.

use std::collections::HashSet;

use glam::IVec3;

use crate::storage::VoxelStorage;

use super::VoxelWorld;

/// Every section whose padded 18^3 array can observe a change in a neighbouring section.
///
/// The mesher samples a full 3x3x3 neighbourhood, so diagonal sections must be invalidated
/// too, not just the six orthogonal ones.
const NEIGHBOUR_OFFSETS_26: [IVec3; 26] = [
    IVec3::new(-1, -1, -1),
    IVec3::new(-1, -1, 0),
    IVec3::new(-1, -1, 1),
    IVec3::new(-1, 0, -1),
    IVec3::new(-1, 0, 0),
    IVec3::new(-1, 0, 1),
    IVec3::new(-1, 1, -1),
    IVec3::new(-1, 1, 0),
    IVec3::new(-1, 1, 1),
    IVec3::new(0, -1, -1),
    IVec3::new(0, -1, 0),
    IVec3::new(0, -1, 1),
    IVec3::new(0, 0, -1),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 1, -1),
    IVec3::new(0, 1, 0),
    IVec3::new(0, 1, 1),
    IVec3::new(1, -1, -1),
    IVec3::new(1, -1, 0),
    IVec3::new(1, -1, 1),
    IVec3::new(1, 0, -1),
    IVec3::new(1, 0, 0),
    IVec3::new(1, 0, 1),
    IVec3::new(1, 1, -1),
    IVec3::new(1, 1, 0),
    IVec3::new(1, 1, 1),
];

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
                for offset in NEIGHBOUR_OFFSETS_26 {
                    dirty.insert(coord + offset);
                }
            }
        }

        // 2. Check sections added or modified in new_storage
        for (&coord, new_sec) in &new_storage.sections {
            if let Some(old_sec) = self.storage.sections.get_mut(&coord) {
                if old_sec != new_sec {
                    dirty.insert(coord);
                    for offset in NEIGHBOUR_OFFSETS_26 {
                        dirty.insert(coord + offset);
                    }
                    *old_sec = new_sec.clone();
                }
            } else {
                dirty.insert(coord);
                for offset in NEIGHBOUR_OFFSETS_26 {
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

        // Never drop dirt queued before this sync: a section edited locally between meshes
        // would otherwise keep its stale geometry until something else touched it.
        self.storage.dirty_sections.extend(dirty);
        let count = self.storage.dirty_sections.len();
        self.sync_selection_bounds();
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_one_section() -> (VoxelWorld, VoxelStorage) {
        let mut storage = VoxelStorage::new();
        storage.set_block(0, 0, 0, "minecraft:stone", None);
        let mut world = VoxelWorld::from_storage(storage.clone(), None, None, None);
        // Prime selection bounds once: the first sync of a fresh world legitimately marks
        // everything dirty, which would mask what the following edits do.
        world.sync_from_storage(&storage);
        world.storage.clear_dirty_sections();
        (world, storage)
    }

    #[test]
    fn sync_keeps_dirt_queued_before_the_sync() {
        let (mut world, storage) = world_with_one_section();

        // A local edit queues a section, then a sync arrives with identical content.
        world.storage.dirty_sections.insert(IVec3::new(0, 0, 0));

        let count = world.sync_from_storage(&storage);

        assert_eq!(count, 1, "the pre-existing dirty section must survive");
        assert!(world.storage.dirty_sections.contains(&IVec3::new(0, 0, 0)));
    }

    #[test]
    fn sync_dirties_diagonal_neighbours_of_a_changed_section() {
        let (mut world, storage) = world_with_one_section();

        let mut changed = storage.clone();
        changed.set_block(1, 0, 0, "minecraft:dirt", None);
        world.sync_from_storage(&changed);

        // Section (0,0,0) changed, so the diagonal section (-1,-1,-1) samples a stale
        // padded array unless it is invalidated as well.
        assert!(
            world
                .storage
                .dirty_sections
                .contains(&IVec3::new(-1, -1, -1)),
            "diagonal neighbour must be dirtied, got {:?}",
            world.storage.dirty_sections
        );
    }
}
