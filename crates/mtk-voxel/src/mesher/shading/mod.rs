//! # Meshing Shading, Material, CTM, and Biome Resolver
//!
//! Encapsulates material resolution, CTM solver querying, Atlas UV lookup,
//! and Biome color sampling for voxel meshing, decoupling material logic
//! from voxel traversal.

pub(crate) mod anim;
pub(crate) mod palette;
pub(crate) mod resolver;
pub(crate) mod types;

pub(crate) use anim::{extract_anim_metrics, lookup_animated_from_cache};
pub(crate) use palette::resolve_palette_meshing_data_for_state;
pub(crate) use resolver::{
    resolve_model_face_shading, resolve_unit_cube_face_shading, sample_quad_corner_biome_tints,
};
pub use types::ModelSource;
pub(crate) use types::{PaletteMeshingData, PreResolvedModelFace, ResolvedFaceShading};
