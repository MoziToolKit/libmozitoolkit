use glam::Vec2;
use mtk_model::{BakedFace, BakedModel, BakedVariantGroup};
use std::borrow::Cow;
use std::sync::Arc;

/// Pre-resolved shading information for a static face.
#[derive(Clone, Debug)]
pub struct PreResolvedFace {
    pub source_texture_key: String,
    pub override_uvs: Option<[Vec2; 4]>,
    pub mat_slot: u16,
    pub chunk_id: i32,
    pub tint_data: [f32; 4],
    pub tint_color: [f32; 4],
    pub anim_timing: [f32; 3],
    pub anim_frame_size: [f32; 3],
    pub average_color: [f32; 4],
}

/// Pre-resolved model face bundling the original BakedFace geometry and pre-resolved shading.
#[derive(Clone, Debug)]
pub struct PreResolvedModelFace {
    pub face: BakedFace,
    pub pre: PreResolvedFace,
}

/// Pre-resolved face buckets for a single model or model variant.
#[derive(Clone, Debug, Default)]
pub struct ModelFaceBuckets {
    pub culled_faces: [Vec<PreResolvedModelFace>; 6],
    pub unculled_faces: Vec<PreResolvedModelFace>,
}

/// Source representation of a block model (single model, multi-variant group, or unit cube fallback).
#[derive(Clone, Debug, Default)]
pub enum ModelSource<'a> {
    #[default]
    None,
    Single(&'a BakedModel),
    Variant(&'a BakedVariantGroup),
    OwnedSingle(Arc<BakedModel>),
    OwnedVariant(Arc<BakedVariantGroup>),
}

impl<'a> ModelSource<'a> {
    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(self, ModelSource::None)
    }

    #[inline]
    pub fn primary_model(&self) -> Option<&BakedModel> {
        match self {
            ModelSource::None => None,
            ModelSource::Single(m) => Some(*m),
            ModelSource::Variant(g) => Some(g.select_primary()),
            ModelSource::OwnedSingle(m) => Some(m.as_ref()),
            ModelSource::OwnedVariant(g) => Some(g.select_primary()),
        }
    }
}

impl<'a> From<Option<&'a BakedModel>> for ModelSource<'a> {
    #[inline]
    fn from(opt: Option<&'a BakedModel>) -> Self {
        match opt {
            Some(m) => ModelSource::Single(m),
            None => ModelSource::None,
        }
    }
}

impl<'a> From<&'a BakedModel> for ModelSource<'a> {
    #[inline]
    fn from(m: &'a BakedModel) -> Self {
        ModelSource::Single(m)
    }
}

impl<'a> From<Option<Arc<BakedModel>>> for ModelSource<'a> {
    #[inline]
    fn from(opt: Option<Arc<BakedModel>>) -> Self {
        match opt {
            Some(m) => ModelSource::OwnedSingle(m),
            None => ModelSource::None,
        }
    }
}

impl<'a> From<Arc<BakedModel>> for ModelSource<'a> {
    #[inline]
    fn from(m: Arc<BakedModel>) -> Self {
        ModelSource::OwnedSingle(m)
    }
}

impl<'a> From<Option<&'a BakedVariantGroup>> for ModelSource<'a> {
    #[inline]
    fn from(opt: Option<&'a BakedVariantGroup>) -> Self {
        match opt {
            Some(g) => ModelSource::Variant(g),
            None => ModelSource::None,
        }
    }
}

impl<'a> From<&'a BakedVariantGroup> for ModelSource<'a> {
    #[inline]
    fn from(g: &'a BakedVariantGroup) -> Self {
        ModelSource::Variant(g)
    }
}

impl<'a> From<Option<Arc<BakedVariantGroup>>> for ModelSource<'a> {
    #[inline]
    fn from(opt: Option<Arc<BakedVariantGroup>>) -> Self {
        match opt {
            Some(g) => ModelSource::OwnedVariant(g),
            None => ModelSource::None,
        }
    }
}

impl<'a> From<Arc<BakedVariantGroup>> for ModelSource<'a> {
    #[inline]
    fn from(g: Arc<BakedVariantGroup>) -> Self {
        ModelSource::OwnedVariant(g)
    }
}

/// Pre-resolved meshing data for a palette entry (either complex BakedModel, VariantGroup, or simple UnitCube).
pub enum PaletteMeshingData {
    Model {
        culled_faces: [Vec<PreResolvedModelFace>; 6],
        unculled_faces: Vec<PreResolvedModelFace>,
    },
    VariantModel {
        variants: Vec<ModelFaceBuckets>,
        weights: Vec<u32>,
        total_weight: u32,
    },
    UnitCube {
        faces: [PreResolvedFace; 6],
    },
    Empty,
}

/// Unified fully-resolved shading and material data for an emitted face.
#[derive(Debug, Clone)]
pub struct ResolvedFaceShading<'a> {
    pub texture_key: Cow<'a, str>,
    pub override_uvs: Option<[Vec2; 4]>,
    pub mat_slot: u16,
    pub chunk_id: i32,
    pub tint_data: [f32; 4],
    pub tint_color: [f32; 4],
    pub anim_timing: [f32; 3],
    pub anim_frame_size: [f32; 3],
    pub average_color: [f32; 4],
}
