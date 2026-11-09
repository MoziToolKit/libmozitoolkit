pub(crate) mod face;
pub(crate) mod unit_cube;

use mtk_core::Direction;
use mtk_model::BakedModel;

use super::types::{ModelFaceBuckets, ModelSource, PaletteMeshingData, PreResolvedModelFace};
use crate::types::MesherConfig;
use face::resolve_model_face;
use unit_cube::resolve_unit_cube_palette_data;

fn resolve_model_buckets(
    baked: &BakedModel,
    state_str: &str,
    config: &MesherConfig,
) -> ModelFaceBuckets {
    let (c_buckets, u_bucket) = baked.get_face_buckets();
    let mut culled_faces: [Vec<PreResolvedModelFace>; 6] = Default::default();
    for dir in Direction::ALL {
        let idx = dir.to_index();
        culled_faces[idx] = c_buckets[idx]
            .iter()
            .map(|f| resolve_model_face(f, state_str, config))
            .collect();
    }
    let unculled_faces = u_bucket
        .iter()
        .map(|f| resolve_model_face(f, state_str, config))
        .collect();

    ModelFaceBuckets {
        culled_faces,
        unculled_faces,
    }
}

/// Pre-resolves meshing data for a single palette entry (either complex BakedModel, VariantGroup, or simple UnitCube).
pub fn resolve_palette_meshing_data_for_state<'a>(
    state_str: &str,
    source: &ModelSource<'a>,
    config: &MesherConfig,
) -> PaletteMeshingData {
    let clean_block = mtk_resource::extract_block_name(state_str);

    match source {
        ModelSource::Single(baked) => {
            let b = resolve_model_buckets(baked, state_str, config);
            PaletteMeshingData::Model {
                culled_faces: b.culled_faces,
                unculled_faces: b.unculled_faces,
            }
        }
        ModelSource::OwnedSingle(baked) => {
            let b = resolve_model_buckets(baked.as_ref(), state_str, config);
            PaletteMeshingData::Model {
                culled_faces: b.culled_faces,
                unculled_faces: b.unculled_faces,
            }
        }
        ModelSource::Variant(group) => {
            if group.len() <= 1 || !config.enable_alternate_blocks {
                let primary = group.select_primary();
                let b = resolve_model_buckets(primary, state_str, config);
                PaletteMeshingData::Model {
                    culled_faces: b.culled_faces,
                    unculled_faces: b.unculled_faces,
                }
            } else {
                let variants = group
                    .models
                    .iter()
                    .map(|m| resolve_model_buckets(m, state_str, config))
                    .collect();
                PaletteMeshingData::VariantModel {
                    variants,
                    weights: group.weights.clone(),
                    total_weight: group.total_weight,
                }
            }
        }
        ModelSource::OwnedVariant(group) => {
            if group.len() <= 1 || !config.enable_alternate_blocks {
                let primary = group.select_primary();
                let b = resolve_model_buckets(primary, state_str, config);
                PaletteMeshingData::Model {
                    culled_faces: b.culled_faces,
                    unculled_faces: b.unculled_faces,
                }
            } else {
                let variants = group
                    .models
                    .iter()
                    .map(|m| resolve_model_buckets(m, state_str, config))
                    .collect();
                PaletteMeshingData::VariantModel {
                    variants,
                    weights: group.weights.clone(),
                    total_weight: group.total_weight,
                }
            }
        }
        ModelSource::None => resolve_unit_cube_palette_data(state_str, clean_block, config),
    }
}
