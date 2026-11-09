//! Pre-resolved entry bundling all per-blockstate meshing metadata, model sources, and culling data.

use mtk_core::{determine_block_offset_type, OffsetType};
use mtk_cull::{BlockCullMeta, FaceCuller};
use mtk_model::{is_block_emissive, BlockState};
use std::sync::Arc;

use crate::mesher::shading::types::ModelFaceBuckets;
use crate::mesher::shading::{
    resolve_palette_meshing_data_for_state, ModelSource, PaletteMeshingData, PreResolvedModelFace,
};
use crate::types::MesherConfig;
use mtk_core::{mc_coordinate_seed, JavaRandom};

/// Selects the face buckets for a variant model given the block position seed.
#[inline]
pub(crate) fn select_variant_faces<'a>(
    variants: &'a [ModelFaceBuckets],
    weights: &[u32],
    total_weight: u32,
    pos: glam::IVec3,
) -> (
    &'a [Vec<PreResolvedModelFace>; 6],
    &'a Vec<PreResolvedModelFace>,
) {
    let seed = mc_coordinate_seed(pos.x, pos.y, pos.z);
    let mut rng = JavaRandom::new(seed);
    let mut target = rng.next_int(total_weight);
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

/// Computes the 4-corner offset coordinates (0 or 1) for biome tint sampling.
#[inline]
pub(crate) fn quad_corner_offsets(vertices: &[glam::Vec3; 4]) -> [[usize; 3]; 4] {
    [
        [
            (vertices[0].x.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[0].y.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[0].z.clamp(0.0, 1.0).round() as usize).min(1),
        ],
        [
            (vertices[1].x.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[1].y.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[1].z.clamp(0.0, 1.0).round() as usize).min(1),
        ],
        [
            (vertices[2].x.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[2].y.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[2].z.clamp(0.0, 1.0).round() as usize).min(1),
        ],
        [
            (vertices[3].x.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[3].y.clamp(0.0, 1.0).round() as usize).min(1),
            (vertices[3].z.clamp(0.0, 1.0).round() as usize).min(1),
        ],
    ]
}

/// Helper to mesh fluid geometry at the given coordinate.
pub(crate) fn emit_fluid_at(
    mesh: &mut mtk_core::MeshData,
    padded: &crate::storage::PaddedVoxelArray,
    block_pos: glam::IVec3,
    (px, py, pz): (usize, usize, usize),
    (lx, ly, lz): (usize, usize, usize),
    (wx, wy, wz): (f32, f32, f32),
    state_str: &str,
    culler: &FaceCuller,
    config: &MesherConfig,
    collector: &mut super::collector::FaceAttributesCollector,
) {
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

    crate::fluid::emit_fluid_geometry(
        mesh,
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
        Some(collector),
        fluid_col,
    );
}

/// Pre-resolved entry bundling all per-blockstate meshing metadata, model sources, and culling data.
#[derive(Clone)]
pub(crate) struct ResolvedPaletteEntry {
    pub has_model: bool,
    pub meta: Arc<BlockCullMeta>,
    pub is_emissive: bool,
    pub offset_type: OffsetType,
    pub meshing_data: Arc<PaletteMeshingData>,
}

impl ResolvedPaletteEntry {
    pub(crate) fn resolve<'a>(
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
