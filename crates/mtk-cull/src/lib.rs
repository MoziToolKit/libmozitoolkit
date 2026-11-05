#![forbid(unsafe_code)]

extern crate alloc;
pub mod engine;
pub mod geometry;
pub mod mesh_cull;
pub mod rules;
pub mod types;

pub use engine::is_inherently_waterlogged_name;
pub use engine::{
    compute_block_cull_meta, derive_parametric_face_shapes, is_non_full_or_partial_block,
    parse_block_name_and_props, FaceCuller,
};
pub use geometry::{
    check_coplanar_overlap, clip_face_excluding_hidden_volume, extract_quad_face_occlusion_rect,
    is_face_completely_occluded, ClippedQuadPiece, CoplanarOverlap, CoplanarRelation,
    FaceAlignment,
};
pub use mesh_cull::{cull_mesh_faces, MeshCullConfig, MeshCullResult};
pub use rules::fluid_cull::{
    fluid_group, is_face_occluded_by_state, is_same_fluid, should_render_fluid_face,
};
pub use rules::should_skip_rendering;
pub use types::{
    BlockCullMeta, CullCategory, GlassCullMode, LeavesCullMode, EMPTY_FACE_RECT, FULL_FACE_RECT,
};
