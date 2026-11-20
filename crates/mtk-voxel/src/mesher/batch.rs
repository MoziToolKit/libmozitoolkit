//! Parallel batch meshing for multiple chunk sections.

use glam::IVec3;
use mtk_core::MeshData;
use mtk_core::{ProgressCallback, ProgressThrottler};
use mtk_cull::FaceCuller;
use std::collections::HashMap;
use std::sync::Arc;

use crate::mesher::palette_entry::ResolvedPaletteEntry;
use crate::mesher::section_mesher::SectionMesher;
use crate::mesher::shading::ModelSource;
use crate::storage::PaddedVoxelArray;
use crate::types::{MesherConfig, VoxelError};

impl SectionMesher {
    /// Meshes a batch of `PaddedVoxelArray`s with optional progress feedback.
    pub(crate) fn mesh_sections_with_source_and_progress<'a, F>(
        sections: &[PaddedVoxelArray],
        culler: &FaceCuller,
        model_provider: F,
        config: &MesherConfig,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<(IVec3, MeshData)>, VoxelError>
    where
        F: Fn(&str) -> ModelSource<'a> + Sync + Send,
    {
        // Pre-resolve PaletteMeshingData once per unique blockstate across the thread pool.
        let palette_cache = build_palette_cache(sections, &model_provider, culler, config);

        let throttler = ProgressThrottler::new("meshing_sections", sections.len(), progress)
            .with_prefix("Meshing chunk");

        #[cfg(feature = "parallel")]
        let results = {
            use rayon::prelude::*;
            let results: Vec<(IVec3, MeshData)> = sections
                .par_iter()
                .map(|sec| {
                    let mesh = Self::mesh_section_with_cache(
                        sec,
                        culler,
                        |st| model_provider(st),
                        config,
                        Some(&palette_cache),
                    );
                    throttler.inc();
                    (sec.coord, mesh)
                })
                .collect();
            results
        };

        #[cfg(not(feature = "parallel"))]
        let results = {
            let results: Vec<(IVec3, MeshData)> = sections
                .iter()
                .map(|sec| {
                    let mesh = Self::mesh_section_with_cache(
                        sec,
                        culler,
                        |st| model_provider(st),
                        config,
                        Some(&palette_cache),
                    );
                    throttler.inc();
                    (sec.coord, mesh)
                })
                .collect();
            results
        };

        Ok(results)
    }

    /// Meshes a batch of `PaddedVoxelArray`s with optional thread pool count and progress feedback.
    pub fn mesh_sections_parallel_with_source_and_progress<'a, F>(
        sections: &[PaddedVoxelArray],
        culler: &FaceCuller,
        model_provider: F,
        config: &MesherConfig,
        num_threads: Option<usize>,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<(IVec3, MeshData)>, VoxelError>
    where
        F: Fn(&str) -> ModelSource<'a> + Sync + Send,
    {
        mtk_core::threading::execute_parallel(num_threads, || {
            Self::mesh_sections_with_source_and_progress(
                sections,
                culler,
                model_provider,
                config,
                progress,
            )
        })
        .map_err(VoxelError::ThreadPoolError)?
    }
}

/// Distinct blockstates carried by the non-empty `sections`.
fn collect_unique_states<'s>(
    sections: &'s [PaddedVoxelArray],
) -> std::collections::HashSet<&'s str> {
    let mut unique_states: std::collections::HashSet<&'s str> = std::collections::HashSet::new();
    for sec in sections {
        if !sec.is_empty {
            for st in &sec.palette {
                unique_states.insert(st.as_str());
            }
        }
    }
    unique_states
}

/// Resolves every distinct blockstate of `sections` once, across the thread pool.
///
/// Shared by the batch mesher: resolving a palette entry walks texture candidates and material
/// aliases, so a per-section resolution made an N-section pass pay N times for the same
/// blockstates.
pub(crate) fn build_palette_cache<'s, 'm, F>(
    sections: &'s [PaddedVoxelArray],
    model_provider: &F,
    culler: &FaceCuller,
    config: &MesherConfig,
) -> HashMap<&'s str, Arc<ResolvedPaletteEntry>>
where
    F: Fn(&str) -> ModelSource<'m> + Sync + Send,
{
    let unique_states = collect_unique_states(sections);

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        unique_states
            .par_iter()
            .map(|&st| {
                let source = model_provider(st);
                let entry = ResolvedPaletteEntry::resolve(st, &source, culler, config);
                (st, Arc::new(entry))
            })
            .collect()
    }

    #[cfg(not(feature = "parallel"))]
    {
        unique_states
            .iter()
            .map(|&st| {
                let source = model_provider(st);
                let entry = ResolvedPaletteEntry::resolve(st, &source, culler, config);
                (st, Arc::new(entry))
            })
            .collect()
    }
}

/// Sequential variant for callers whose model lookup is `FnMut` (the delta mesher).
pub(crate) fn build_palette_cache_sequential<'s, 'm, F>(
    sections: &'s [PaddedVoxelArray],
    model_provider: &mut F,
    culler: &FaceCuller,
    config: &MesherConfig,
) -> HashMap<&'s str, Arc<ResolvedPaletteEntry>>
where
    F: FnMut(&str) -> ModelSource<'m>,
{
    collect_unique_states(sections)
        .into_iter()
        .map(|st| {
            let source = model_provider(st);
            let entry = ResolvedPaletteEntry::resolve(st, &source, culler, config);
            (st, Arc::new(entry))
        })
        .collect()
}
