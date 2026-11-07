//! High-performance mesh generator for individual and batch chunk sections.

use glam::IVec3;
use mtk_core::Direction;
use mtk_core::MeshData;
use mtk_core::{
    determine_block_offset_type, get_block_offset, mc_coordinate_seed, JavaRandom, OffsetType,
};
use mtk_core::{ProgressCallback, ProgressThrottler};
use mtk_cull::{is_non_mesh_placeholder_block, BlockCullMeta, FaceCuller};
use mtk_model::{is_block_emissive, BlockState};
use std::collections::HashMap;
use std::sync::Arc;

use crate::fluid::emit_fluid_geometry;
use crate::mesher::ao::{calculate_face_ao, calculate_face_ao_float, is_face_cubic};
use crate::storage::{padded_index, PaddedVoxelArray, SECTION_SIZE};
use crate::types::{MesherConfig, VoxelError};

use super::collector::FaceAttributesCollector;
use super::emitter::{emit_baked_face, emit_unit_cube_face};
use super::shading::{
    resolve_model_face_shading, resolve_palette_meshing_data_for_state,
    resolve_unit_cube_face_shading, sample_biome_tint, ModelSource, PaletteMeshingData,
    PreResolvedModelFace,
};

/// Pre-resolved entry bundling all per-blockstate meshing metadata, model sources, and culling data.
#[derive(Clone)]
pub struct ResolvedPaletteEntry {
    pub has_model: bool,
    pub meta: Arc<BlockCullMeta>,
    pub is_emissive: bool,
    pub offset_type: OffsetType,
    pub meshing_data: Arc<PaletteMeshingData>,
}

impl ResolvedPaletteEntry {
    pub fn resolve<'a>(
        st: &str,
        source: &ModelSource<'a>,
        culler: &FaceCuller,
        config: &MesherConfig,
    ) -> Self {
        let (mut meta, is_emissive) = if let Some(model) = source.primary_model() {
            let mut meta = model.get_or_compute_cull_meta();
            if st.contains("waterlogged=true") {
                meta.is_waterlogged = true;
            }
            (Arc::new(meta), model.is_emissive)
        } else {
            let runtime_meta = culler.get_meta(st, None, None);
            let parsed = BlockState::parse(st);
            let default_state = BlockState::default();
            let state = parsed.as_ref().unwrap_or(&default_state);
            let emissive = is_block_emissive(state);
            (runtime_meta, emissive)
        };

        let offset_type = if config.enable_random_offsets {
            determine_block_offset_type(st)
        } else {
            OffsetType::None
        };

        let meshing_data = Arc::new(resolve_palette_meshing_data_for_state(st, source, config));
        if matches!(*meshing_data, PaletteMeshingData::Empty) {
            let mut empty_meta = (*meta).clone();
            empty_meta.is_air = true;
            empty_meta.is_full_cube = false;
            empty_meta.is_opaque = false;
            meta = Arc::new(empty_meta);
        }

        Self {
            has_model: !source.is_none(),
            meta,
            is_emissive,
            offset_type,
            meshing_data,
        }
    }
}

/// High-performance mesh generator for individual and batch chunk sections.
pub struct SectionMesher;

impl SectionMesher {
    /// Meshes a single `PaddedVoxelArray` into a complete `MeshData` buffer using a unified `ModelSource` lookup.
    pub fn mesh_section<'a, F>(
        padded: &PaddedVoxelArray,
        culler: &FaceCuller,
        get_model_source: F,
        config: &MesherConfig,
    ) -> MeshData
    where
        F: FnMut(&str) -> ModelSource<'a>,
    {
        Self::mesh_section_with_cache(padded, culler, get_model_source, config, None)
    }

    /// Meshes a single `PaddedVoxelArray` with an optional pre-resolved palette cache.
    pub fn mesh_section_with_cache<'a, F>(
        padded: &PaddedVoxelArray,
        culler: &FaceCuller,
        mut get_model_source: F,
        config: &MesherConfig,
        palette_cache: Option<&std::collections::HashMap<&str, Arc<ResolvedPaletteEntry>>>,
    ) -> MeshData
    where
        F: FnMut(&str) -> ModelSource<'a>,
    {
        if padded.is_empty {
            return MeshData::new();
        }

        let mut mesh = MeshData::with_quad_capacity(2048, 3072, 512, true);
        let mut collector = FaceAttributesCollector::with_capacity(512);

        let palette_entries: Vec<Arc<ResolvedPaletteEntry>> = padded
            .palette
            .iter()
            .map(|st| {
                if let Some(cache) = palette_cache {
                    if let Some(entry) = cache.get(st.as_str()) {
                        return Arc::clone(entry);
                    }
                }
                let source = get_model_source(st);
                Arc::new(ResolvedPaletteEntry::resolve(st, &source, culler, config))
            })
            .collect();

        let world_offset_x = (padded.coord.x * 16) as f32;
        let world_offset_y = (padded.coord.y * 16) as f32;
        let world_offset_z = (padded.coord.z * 16) as f32;
        let world_origin = glam::Vec3::new(world_offset_x, world_offset_y, world_offset_z);

        let mut grid_pool = if config.weld_vertices {
            Some(super::grid_pool::GridVertexPool::new())
        } else {
            None
        };
        let mut has_non_grid_geometry = false;

        let is_solid_opaque_fn = |px: usize, py: usize, pz: usize| -> bool {
            let idx = padded.get_padded_idx(px, py, pz) as usize;
            if idx < palette_entries.len() {
                let meta = &palette_entries[idx].meta;
                meta.is_full_cube && meta.is_opaque && !meta.is_air && !meta.is_fluid
            } else {
                false
            }
        };

        // RFC v3.2: Pre-resolve dynamic model data for any sparse block entities in this section
        let mut entity_meshing_entries = HashMap::new();
        if !padded.block_entities.is_empty() {
            let mut resolved_cache: HashMap<
                (&str, &mtk_model::BlockEntityData),
                Arc<crate::mesher::shading::types::PaletteMeshingData>,
            > = HashMap::new();

            for (&local_idx, be) in &padded.block_entities {
                let lx = (local_idx >> 8) as usize;
                let ly = ((local_idx >> 4) & 15) as usize;
                let lz = (local_idx & 15) as usize;
                let px = lx + 1;
                let py = ly + 1;
                let pz = lz + 1;
                let pal_idx = padded.get_padded_idx(px, py, pz) as usize;
                if pal_idx < padded.palette.len() {
                    let state_str = padded.palette[pal_idx].as_str();
                    if let Some(cached) = resolved_cache.get(&(state_str, be)) {
                        entity_meshing_entries.insert(local_idx, Arc::clone(cached));
                        continue;
                    }

                    let baked = mtk_model::BuiltinModelRegistry::bake_block_entity(state_str, be);

                    if let Some(baked_model) = baked {
                        let source =
                            crate::mesher::shading::types::ModelSource::Single(&baked_model);
                        let m_data = Arc::new(
                            crate::mesher::shading::palette::resolve_palette_meshing_data_for_state(
                                state_str, &source, config,
                            ),
                        );
                        resolved_cache.insert((state_str, be), Arc::clone(&m_data));
                        entity_meshing_entries.insert(local_idx, m_data);
                    }
                }
            }
        }

        let has_block_entities = !entity_meshing_entries.is_empty();

        for lx in 0..SECTION_SIZE {
            let px = lx + 1;
            let wx = world_offset_x + lx as f32;

            for ly in 0..SECTION_SIZE {
                let py = ly + 1;
                let wy = world_offset_y + ly as f32;

                for lz in 0..SECTION_SIZE {
                    let pz = lz + 1;
                    let wz = world_offset_z + lz as f32;

                    let pal_idx = padded.get_padded_idx(px, py, pz) as usize;
                    if pal_idx >= palette_entries.len() {
                        continue;
                    }

                    let entry = &palette_entries[pal_idx];
                    let meta = &entry.meta;
                    if meta.is_air && !meta.is_waterlogged {
                        continue;
                    }

                    let state_str = &padded.palette[pal_idx];
                    if is_non_mesh_placeholder_block(state_str) && !meta.is_waterlogged {
                        continue;
                    }

                    let block_pos = IVec3::new(wx as i32, wy as i32, wz as i32);

                    // 1. Fluid Meshing (Water, Lava, or Waterlogged)
                    if config.mesh_fluids && (meta.is_fluid || meta.is_waterlogged) {
                        has_non_grid_geometry = true;
                        let get_state = |nx: i32, ny: i32, nz: i32| -> String {
                            let n_px = (nx - block_pos.x + px as i32) as usize;
                            let n_py = (ny - block_pos.y + py as i32) as usize;
                            let n_pz = (nz - block_pos.z + pz as i32) as usize;
                            if n_px < 18 && n_py < 18 && n_pz < 18 {
                                padded.get_padded_state(n_px, n_py, n_pz).to_string()
                            } else {
                                "minecraft:air".to_string()
                            }
                        };

                        let fluid_col = if let Some(ref biome_cols) = padded.biome_data {
                            let col_idx = if biome_cols.len() == 1024 {
                                let qy = (ly >> 2) & 3;
                                (qy << 8) | ((lx & 15) << 4) | (lz & 15)
                            } else {
                                lx * 16 + lz
                            };
                            if col_idx < biome_cols.len() {
                                Some(biome_cols[col_idx].water_color)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        emit_fluid_geometry(
                            &mut mesh,
                            block_pos.x,
                            block_pos.y,
                            block_pos.z,
                            wx,
                            wy,
                            wz,
                            state_str,
                            get_state,
                            culler,
                            config,
                            0,
                            Some(&mut collector),
                            fluid_col,
                        );

                        if meta.is_fluid && !meta.is_waterlogged {
                            continue;
                        }
                    }

                    // Skip static solid geometry for invisible technical / placeholder blocks
                    if is_non_mesh_placeholder_block(state_str) {
                        continue;
                    }

                    let offset_type = entry.offset_type;
                    let plant_offset = if offset_type != OffsetType::None {
                        has_non_grid_geometry = true;
                        get_block_offset(offset_type, block_pos.x, block_pos.y, block_pos.z)
                    } else {
                        glam::Vec3::ZERO
                    };

                    // 2. Solid Block / Baked Model Meshing
                    let is_emissive = entry.is_emissive;

                    let get_padded_neighbor_state = |target_pos: IVec3| -> Option<&str> {
                        let rel_x = target_pos.x - block_pos.x + px as i32;
                        let rel_y = target_pos.y - block_pos.y + py as i32;
                        let rel_z = target_pos.z - block_pos.z + pz as i32;
                        if (0..18).contains(&rel_x)
                            && (0..18).contains(&rel_y)
                            && (0..18).contains(&rel_z)
                        {
                            Some(padded.get_padded_state(
                                rel_x as usize,
                                rel_y as usize,
                                rel_z as usize,
                            ))
                        } else {
                            None
                        }
                    };

                    let entity_data = if has_block_entities {
                        let local_idx = ((lx << 8) | (ly << 4) | lz) as u16;
                        entity_meshing_entries.get(&local_idx)
                    } else {
                        None
                    };

                    if entity_data.is_some() || entry.has_model {
                        has_non_grid_geometry = true;
                        let (culled_faces, unculled_faces) = if let Some(emd) = entity_data {
                            match &**emd {
                                PaletteMeshingData::Model {
                                    ref culled_faces,
                                    ref unculled_faces,
                                } => (culled_faces, unculled_faces),
                                _ => unreachable!(),
                            }
                        } else {
                            match &*entry.meshing_data {
                                PaletteMeshingData::Model {
                                    ref culled_faces,
                                    ref unculled_faces,
                                } => (culled_faces, unculled_faces),
                                PaletteMeshingData::VariantModel {
                                    ref variants,
                                    ref weights,
                                    total_weight,
                                } => {
                                    let seed =
                                        mc_coordinate_seed(block_pos.x, block_pos.y, block_pos.z);
                                    let mut rng = JavaRandom::new(seed);
                                    let mut target = rng.next_int(*total_weight);
                                    let mut chosen = 0;
                                    for (i, &w) in weights.iter().enumerate() {
                                        if target < w {
                                            chosen = i;
                                            break;
                                        }
                                        target = target.saturating_sub(w);
                                    }
                                    let b = &variants[chosen];
                                    (&b.culled_faces, &b.unculled_faces)
                                }
                                PaletteMeshingData::UnitCube { .. } | PaletteMeshingData::Empty => {
                                    unreachable!()
                                }
                            }
                        };

                        let mut emit_model_face = |mf: &PreResolvedModelFace| {
                            let face = &mf.face;
                            let face_dir = face.direction;

                            // Calculate 4-corner AO for face.direction (matching Minecraft BlockModelLighter)
                            let face_cubic =
                                is_face_cubic(face_dir, &face.vertices, meta.is_full_cube);
                            let ao_brightness = if config.enable_ao && !is_emissive {
                                calculate_face_ao_float(face_dir, face_cubic, |dx, dy, dz| {
                                    let sx = (px as i32 + dx) as usize;
                                    let sy = (py as i32 + dy) as usize;
                                    let sz = (pz as i32 + dz) as usize;
                                    if sx < 18 && sy < 18 && sz < 18 {
                                        is_solid_opaque_fn(sx, sy, sz)
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                [1.0, 1.0, 1.0, 1.0]
                            };

                            let shading = resolve_model_face_shading(
                                mf,
                                state_str,
                                block_pos,
                                config,
                                get_padded_neighbor_state,
                            );

                            let (final_tint_data, final_tint_color) = sample_biome_tint(
                                padded.biome_data.as_deref(),
                                lx,
                                ly,
                                lz,
                                shading.tint_data,
                                shading.tint_color,
                                config.colormaps.as_deref(),
                            );

                            emit_baked_face(
                                &mut mesh,
                                face,
                                wx + plant_offset.x,
                                wy + plant_offset.y,
                                wz + plant_offset.z,
                                ao_brightness,
                                config,
                                &mut collector,
                                &shading,
                                final_tint_data,
                                final_tint_color,
                                block_pos,
                            );
                        };

                        // 1. Process 6 cull directions (Directional Fast-Path)
                        for dir in Direction::ALL {
                            let faces = &culled_faces[dir.to_index()];
                            if faces.is_empty() {
                                continue;
                            }

                            let offset = dir.offset();
                            let npx = (px as i32 + offset.x) as usize;
                            let npy = (py as i32 + offset.y) as usize;
                            let npz = (pz as i32 + offset.z) as usize;

                            let npal_idx =
                                padded.padded_voxels[padded_index(npx, npy, npz)] as usize;
                            let n_meta = if npal_idx < palette_entries.len() {
                                Some(&*palette_entries[npal_idx].meta)
                            } else {
                                None
                            };
                            let neighbor_pos = block_pos + offset;

                            if !culler.should_render_face(
                                meta,
                                n_meta,
                                dir,
                                None,
                                Some(block_pos),
                                Some(neighbor_pos),
                            ) {
                                continue;
                            }

                            for mf in faces {
                                emit_model_face(mf);
                            }
                        }

                        // 2. Process unculled / internal faces (zero neighbor lookup!)
                        for mf in unculled_faces {
                            emit_model_face(mf);
                        }
                    } else {
                        // Standard unit cube meshing
                        let cube_faces = match &*entry.meshing_data {
                            PaletteMeshingData::UnitCube { faces } => faces,
                            PaletteMeshingData::Empty => continue,
                            _ => unreachable!(),
                        };

                        for dir in Direction::ALL {
                            let offset = dir.offset();
                            let npx = (px as i32 + offset.x) as usize;
                            let npy = (py as i32 + offset.y) as usize;
                            let npz = (pz as i32 + offset.z) as usize;

                            let npal_idx =
                                padded.padded_voxels[padded_index(npx, npy, npz)] as usize;
                            let n_meta = if npal_idx < palette_entries.len() {
                                Some(&*palette_entries[npal_idx].meta)
                            } else {
                                None
                            };

                            let neighbor_pos = block_pos + offset;

                            // Check occlusion visibility
                            if !culler.should_render_face(
                                meta,
                                n_meta,
                                dir,
                                None,
                                Some(block_pos),
                                Some(neighbor_pos),
                            ) {
                                continue;
                            }

                            // Calculate 4-corner AO (UnitCube is always face_cubic = true)
                            let ao_brightness = if config.enable_ao && !is_emissive {
                                calculate_face_ao(dir, |dx, dy, dz| {
                                    let sx = (px as i32 + dx) as usize;
                                    let sy = (py as i32 + dy) as usize;
                                    let sz = (pz as i32 + dz) as usize;
                                    if sx < 18 && sy < 18 && sz < 18 {
                                        is_solid_opaque_fn(sx, sy, sz)
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                [1.0, 1.0, 1.0, 1.0]
                            };

                            let pre_face = &cube_faces[dir.to_index()];
                            let shading = resolve_unit_cube_face_shading(
                                pre_face,
                                dir,
                                state_str,
                                block_pos,
                                config,
                                get_padded_neighbor_state,
                            );

                            let (final_tint_data, final_tint_color) = sample_biome_tint(
                                padded.biome_data.as_deref(),
                                lx,
                                ly,
                                lz,
                                shading.tint_data,
                                shading.tint_color,
                                config.colormaps.as_deref(),
                            );

                            let grid_ctx = grid_pool
                                .as_mut()
                                .map(|pool| (pool, [lx, ly, lz], world_origin));

                            emit_unit_cube_face(
                                &mut mesh,
                                dir,
                                wx + plant_offset.x,
                                wy + plant_offset.y,
                                wz + plant_offset.z,
                                ao_brightness,
                                config,
                                &mut collector,
                                &shading,
                                final_tint_data,
                                final_tint_color,
                                block_pos,
                                grid_ctx,
                            );
                        }
                    }
                }
            }
        }

        collector.attach_to_mesh(&mut mesh);
        if config.weld_vertices && has_non_grid_geometry {
            mesh.weld_vertices(1e-4);
        }
        mesh
    }

    /// Meshes a batch of `PaddedVoxelArray`s with optional progress feedback.
    pub fn mesh_sections_with_source_and_progress<'a, F>(
        sections: &[PaddedVoxelArray],
        culler: &FaceCuller,
        model_provider: F,
        config: &MesherConfig,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<(IVec3, MeshData)>, VoxelError>
    where
        F: Fn(&str) -> ModelSource<'a> + Sync + Send,
    {
        // 1. Collect all distinct blockstates across all sections
        let mut unique_states: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for sec in sections {
            if !sec.is_empty {
                for st in &sec.palette {
                    unique_states.insert(st.as_str());
                }
            }
        }

        // 2. Pre-resolve PaletteMeshingData once per unique blockstate across the thread pool
        #[cfg(feature = "parallel")]
        let palette_cache: std::collections::HashMap<&str, Arc<ResolvedPaletteEntry>> = {
            use rayon::prelude::*;
            unique_states
                .par_iter()
                .map(|&st| {
                    let source = model_provider(st);
                    let entry = ResolvedPaletteEntry::resolve(st, &source, culler, config);
                    (st, Arc::new(entry))
                })
                .collect()
        };

        #[cfg(not(feature = "parallel"))]
        let palette_cache: std::collections::HashMap<&str, Arc<ResolvedPaletteEntry>> =
            unique_states
                .iter()
                .map(|&st| {
                    let source = model_provider(st);
                    let entry = ResolvedPaletteEntry::resolve(st, &source, culler, config);
                    (st, Arc::new(entry))
                })
                .collect();

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
        mtk_core::constants::concurrency::execute_parallel(num_threads, || {
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
