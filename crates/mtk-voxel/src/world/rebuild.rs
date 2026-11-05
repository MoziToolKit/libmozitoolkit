//! # World Rebuilding and Section Meshing Orchestration
//!
//! Handles full parallel rebuilding (`rebuild_all_with_progress`) and incremental
//! delta rebuilding (`rebuild_dirty`, `rebuild_dirty_in_place`).

use glam::IVec3;
use mtk_core::MeshData;
use mtk_core::{ProgressCallback, ProgressReport};
use mtk_model::baker::baked_model::BakedModelDatabase;

use crate::mesher::{DeltaMesher, SectionMesher};
use crate::types::VoxelError;

use super::VoxelWorld;

impl VoxelWorld {
    /// Resolves the model source (variant group or single model) for a blockstate.
    #[inline]
    pub fn model_source_for_state<'a>(
        db: Option<&'a BakedModelDatabase>,
        state: &str,
    ) -> crate::mesher::ModelSource<'a> {
        if let Some(db) = db {
            if let Some(group) = db.get_variant_group(state) {
                return crate::mesher::ModelSource::Variant(group);
            }
            if let Some(model) = db.get(state) {
                return crate::mesher::ModelSource::Single(model);
            }
        }
        if mtk_model::entity::banner::extract_banner_color_from_state(state).is_some() {
            let mut baker = mtk_model::baker::ModelBaker::new();
            if let Ok(baked) = baker.bake_blockstate_with_entity(state, None, None, |_| None) {
                return crate::mesher::ModelSource::OwnedSingle(std::sync::Arc::new(baked));
            }
        }
        crate::mesher::ModelSource::None
    }

    /// Re-meshes all non-empty sections in parallel and updates the section cache.
    pub fn rebuild_all(&mut self) -> Result<&MeshData, VoxelError> {
        self.rebuild_all_with_progress(None)
    }

    /// Re-meshes all non-empty sections in parallel with progress reporting and updates the section cache.
    pub fn rebuild_all_with_progress(
        &mut self,
        progress: Option<ProgressCallback>,
    ) -> Result<&MeshData, VoxelError> {
        let mut profiler = mtk_core::ExecutionProfiler::start();

        profiler.measure("sync_selection_bounds", || {
            self.sync_selection_bounds();
        });

        profiler.measure("ensure_dynamic_entities_resolved", || {
            self.ensure_dynamic_entities_resolved();
        });

        let non_empty = self.storage.get_all_non_empty_sections();
        if non_empty.is_empty() {
            self.section_mesh_cache.clear();
            self.world_mesh = Some(MeshData::new());
            self.used_chunk_ids.clear();
            if let Some(cb) = progress {
                cb(ProgressReport::new(
                    "meshing_sections",
                    0,
                    0,
                    "No sections to mesh",
                ));
            }
            self.last_profile = Some(profiler.finish());
            return Ok(self.world_mesh.as_ref().unwrap());
        }

        #[cfg(feature = "parallel")]
        let padded_sections: Vec<_> = profiler
            .measure_with_count("padded_array_extraction", non_empty.len(), || {
                use rayon::prelude::*;
                mtk_core::constants::concurrency::execute_parallel(self.num_threads, || {
                    non_empty
                        .par_iter()
                        .map(|&coord| self.storage.get_section_padded_array(coord))
                        .collect()
                })
            })
            .map_err(VoxelError::ThreadPoolError)?;

        #[cfg(not(feature = "parallel"))]
        let padded_sections: Vec<_> =
            profiler.measure_with_count("padded_array_extraction", non_empty.len(), || {
                non_empty
                    .iter()
                    .map(|&coord| self.storage.get_section_padded_array(coord))
                    .collect()
            });

        let db_opt = self.model_db.as_deref();
        let model_lookup = |state: &str| Self::model_source_for_state(db_opt, state);

        let results =
            profiler.measure_with_count("parallel_section_meshing", non_empty.len(), || {
                SectionMesher::mesh_sections_parallel_with_source_and_progress(
                    &padded_sections,
                    &self.culler,
                    model_lookup,
                    &self.config,
                    self.num_threads,
                    progress,
                )
            })?;

        profiler.measure("section_cache_insertion", || {
            self.section_mesh_cache.clear();
            for (coord, mesh) in results {
                if !mesh.is_empty() {
                    self.section_mesh_cache.insert(coord, mesh);
                }
            }
        });

        if let Some(cb) = progress {
            let total = non_empty.len();
            cb(ProgressReport::new(
                "assembling_world_mesh",
                total,
                total,
                format!("Assembled world mesh from {} chunks", total),
            ));
        }

        self.assemble_world_mesh(true, Some(&mut profiler));
        self.storage.clear_dirty_sections();

        self.last_profile = Some(profiler.finish());
        Ok(self.world_mesh.as_ref().unwrap())
    }

    /// Incrementally rebuilds only the modified/dirty sections directly in-place into the
    /// section cache without allocating or cloning redundant return buffers.
    ///
    /// Updates the unified assembled world mesh and returns the count of rebuilt sections.
    pub fn rebuild_dirty_in_place(&mut self) -> Result<usize, VoxelError> {
        if self.storage.dirty_sections.is_empty() {
            return Ok(0);
        }

        let mut profiler = mtk_core::ExecutionProfiler::start();

        profiler.measure("sync_selection_bounds", || {
            self.sync_selection_bounds();
        });

        profiler.measure("ensure_dynamic_entities_resolved", || {
            let dirty: Vec<glam::IVec3> = self.storage.dirty_sections.iter().copied().collect();
            self.ensure_dynamic_entities_resolved_for_sections(&dirty);
        });

        let db_opt = self.model_db.as_deref();
        let model_lookup = |state: &str| Self::model_source_for_state(db_opt, state);

        let dirty_count = self.storage.dirty_sections.len();
        let rebuilt = profiler.measure_with_count("rebuild_dirty_sections", dirty_count, || {
            DeltaMesher::rebuild_dirty_sections_with_source(
                &mut self.storage,
                &self.culler,
                model_lookup,
                &self.config,
            )
        });

        let rebuilt_count = rebuilt.len();
        profiler.measure("section_cache_insertion", || {
            for (coord, mesh) in rebuilt {
                if mesh.is_empty() {
                    self.section_mesh_cache.remove(&coord);
                } else {
                    self.section_mesh_cache.insert(coord, mesh);
                }
            }
        });

        self.assemble_world_mesh(false, Some(&mut profiler));

        self.last_profile = Some(profiler.finish());

        Ok(rebuilt_count)
    }

    /// Incrementally rebuilds only the modified/dirty sections and updates the section cache.
    /// Returns the list of updated `(section_coord, MeshData)`.
    pub fn rebuild_dirty(&mut self) -> Result<Vec<(IVec3, MeshData)>, VoxelError> {
        if self.storage.dirty_sections.is_empty() {
            return Ok(Vec::new());
        }

        let mut profiler = mtk_core::ExecutionProfiler::start();

        profiler.measure("sync_selection_bounds", || {
            self.sync_selection_bounds();
        });

        profiler.measure("ensure_dynamic_entities_resolved", || {
            let dirty: Vec<glam::IVec3> = self.storage.dirty_sections.iter().copied().collect();
            self.ensure_dynamic_entities_resolved_for_sections(&dirty);
        });

        let db_opt = self.model_db.as_deref();
        let model_lookup = |state: &str| Self::model_source_for_state(db_opt, state);

        let dirty_count = self.storage.dirty_sections.len();
        let rebuilt = profiler.measure_with_count("rebuild_dirty_sections", dirty_count, || {
            DeltaMesher::rebuild_dirty_sections_with_source(
                &mut self.storage,
                &self.culler,
                model_lookup,
                &self.config,
            )
        });

        profiler.measure("section_cache_insertion", || {
            for (coord, mesh) in &rebuilt {
                if mesh.is_empty() {
                    self.section_mesh_cache.remove(coord);
                } else {
                    self.section_mesh_cache.insert(*coord, mesh.clone());
                }
            }
        });

        self.assemble_world_mesh(false, Some(&mut profiler));

        self.last_profile = Some(profiler.finish());

        Ok(rebuilt)
    }
}
