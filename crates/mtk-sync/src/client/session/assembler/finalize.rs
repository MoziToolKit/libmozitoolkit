use std::collections::HashMap;

use glam::IVec3;
use mtk_voxel::{SectionBiomes, SectionStorage, VoxelStorage};

use super::BaselineAssembler;
use crate::protocol::*;

impl BaselineAssembler {
    /// Validates BASELINE_END, reconstructs VoxelStorage, and verifies state_hash SSOT.
    pub fn finalize(&self, end_pkt: &Packet) -> Result<VoxelStorage, ResyncReason> {
        let (epoch, snapshot_uuid, snapshot_seq, total_chunks_sent, final_state_hash, status) =
            match end_pkt {
                Packet::BaselineEnd {
                    epoch,
                    snapshot_uuid,
                    snapshot_seq,
                    total_chunks_sent,
                    final_state_hash,
                    status,
                } => (
                    *epoch,
                    *snapshot_uuid,
                    *snapshot_seq,
                    *total_chunks_sent,
                    *final_state_hash,
                    *status,
                ),
                _ => return Err(ResyncReason::ChunkCorrupted),
            };

        if status != 0 {
            return Err(ResyncReason::ChunkCorrupted);
        }

        if epoch != self.epoch
            || snapshot_uuid != self.snapshot_uuid
            || snapshot_seq != self.snapshot_seq
        {
            return Err(ResyncReason::ChunkCorrupted);
        }

        if total_chunks_sent != self.chunks_received {
            return Err(ResyncReason::SequenceGap);
        }

        if self.raw_voxel_indices.len() < self.total_voxels {
            return Err(ResyncReason::ChunkCorrupted);
        }

        // Build VoxelStorage
        let mut storage = VoxelStorage::new();
        storage.min_x = self.min_pos.x;
        storage.min_y = self.min_pos.y;
        storage.min_z = self.min_pos.z;
        storage.size_x = self.size.x;
        storage.size_y = self.size.y;
        storage.size_z = self.size.z;
        storage.has_explicit_bounds = true;

        if let Some(first_biome) = self.biome_palette.first() {
            storage.primary_biome = Some(first_biome.clone());
        }

        let p_len = self.block_palette.len();
        let quart_grid = QuartGrid::from_bounds(self.min_pos, self.size);
        let mut section_biomes_map: HashMap<IVec3, SectionBiomes> = HashMap::new();

        for (idx, &p_idx_raw) in self
            .raw_voxel_indices
            .iter()
            .enumerate()
            .take(self.total_voxels)
        {
            let p_idx = p_idx_raw as usize;
            if p_idx >= p_len {
                continue;
            }
            let state_str = &self.block_palette[p_idx];

            let (lx, ly, lz) = voxel_index_to_relative_coords(idx, self.size);

            let wx = self.min_pos.x + lx as i32;
            let wy = self.min_pos.y + ly as i32;
            let wz = self.min_pos.z + lz as i32;

            let sec_coord = IVec3::new(wx >> 4, wy >> 4, wz >> 4);
            let sec = storage
                .sections
                .entry(sec_coord)
                .or_insert_with(|| SectionStorage::new(sec_coord));

            let blx = (wx & 15) as usize;
            let bly = (wy & 15) as usize;
            let blz = (wz & 15) as usize;

            sec.set_local(blx, bly, blz, state_str);
        }

        // Apply Biomes if present
        let bp_len = self.biome_palette.len();
        if bp_len == 1 {
            // Single-biome selection: the server omits the quart index stream entirely and
            // only ships a one-entry palette. Every section in the selection is that biome.
            let biome = self.biome_palette[0].clone();
            for sec in storage.sections.values_mut() {
                sec.biomes = Some(SectionBiomes::uniform(biome.clone()));
            }
            storage.finalize_biome_metadata();
        } else if bp_len > 1 && !self.raw_quart_biome_indices.is_empty() {
            for (q_idx, &b_idx_raw) in self.raw_quart_biome_indices.iter().enumerate() {
                let b_idx = b_idx_raw as usize;
                if b_idx >= bp_len {
                    continue;
                }
                let biome_str = &self.biome_palette[b_idx];

                let world_q = quart_grid.quart_index_to_world_coords(q_idx);
                let world_qx = world_q.x;
                let world_qy = world_q.y;
                let world_qz = world_q.z;

                let sec_coord = IVec3::new(
                    world_qx.div_euclid(4),
                    world_qy.div_euclid(4),
                    world_qz.div_euclid(4),
                );
                // Placeholder cells stay unassigned and are filled with the nearest
                // sampled biome after the stream is consumed. Marking them as
                // authoritative palette[0] (the selection's min corner, usually
                // deep_dark/lush_caves) used to leak that biome into partial border
                // sections and flip the 2D/3D smoothing mode.
                let sb = section_biomes_map
                    .entry(sec_coord)
                    .or_insert_with(|| SectionBiomes::blank(self.biome_palette.clone()));

                let sec_qx = world_qx.rem_euclid(4) as usize;
                let sec_qy = world_qy.rem_euclid(4) as usize;
                let sec_qz = world_qz.rem_euclid(4) as usize;
                sb.set_sampled_quart_biome(sec_qx, sec_qy, sec_qz, biome_str);
            }

            for (sec_coord, mut sb) in section_biomes_map {
                // A selection-anchored quart grid does not cover every cell of the
                // sections it touches. Fill the holes from the nearest sampled cell so
                // the section is self-consistent with what the save loader produces.
                sb.fill_unwritten_nearest();
                if let Some(sec) = storage.sections.get_mut(&sec_coord) {
                    sec.biomes = Some(sb);
                }
            }

            // Derive the shared biome metadata (surface column map + dominant primary)
            // through the same implementation the save loader uses.
            storage.finalize_biome_metadata();
        }

        // Apply RFC v3.2 Sparse Block Entities
        for (rel_pos, entity) in &self.block_entities {
            if rel_pos.x < 0
                || rel_pos.x >= self.size.x
                || rel_pos.y < 0
                || rel_pos.y >= self.size.y
                || rel_pos.z < 0
                || rel_pos.z >= self.size.z
            {
                continue;
            }
            let wx = self.min_pos.x + rel_pos.x;
            let wy = self.min_pos.y + rel_pos.y;
            let wz = self.min_pos.z + rel_pos.z;
            let sec_coord = IVec3::new(wx >> 4, wy >> 4, wz >> 4);
            let sec = storage
                .sections
                .entry(sec_coord)
                .or_insert_with(|| SectionStorage::new(sec_coord));
            let blx = (wx & 15) as usize;
            let bly = (wy & 15) as usize;
            let blz = (wz & 15) as usize;
            let local_idx = mtk_voxel::block_index(blx, bly, blz) as u16;
            sec.set_block_entity(local_idx, Some(entity.clone()));
        }

        // Apply and validate RFC v3.2 Continuous Visual Entities
        if let Some(expected_hash) = self.visual_entities_hash {
            let computed_entities_hash =
                crate::protocol::compute_visual_entities_hash(&self.visual_entities);
            if computed_entities_hash != expected_hash {
                return Err(ResyncReason::VisualEntitiesHashMismatch);
            }
        }
        storage.entities = self.visual_entities.clone();

        // Validate state_hash SSOT
        let computed_hash = compute_sparse_state_hash_for_storage(
            &storage,
            self.min_pos,
            self.size,
            &self.dimension,
        );
        if computed_hash != final_state_hash {
            return Err(ResyncReason::StateHashMismatch);
        }

        Ok(storage)
    }
}
