#![forbid(unsafe_code)]

extern crate alloc;

mod engine;
mod geometry;
mod mesh_cull;
mod rules;
mod types;

// 1. Face Culling Engine & Metadata Computation
pub use engine::FaceCuller;
pub use rules::{compute_block_cull_meta, parse_block_name_and_props, should_render_fluid_face};

// 2. Coplanar & Volume Geometry Analysis
pub use geometry::{
    check_coplanar_overlap, clip_face_excluding_hidden_volume, ClippedQuadPiece, CoplanarOverlap,
    CoplanarRelation, FaceAlignment,
};

// 3. Mesh-Level Culling
pub use mesh_cull::{cull_mesh_faces, MeshCullConfig, MeshCullResult};

// 4. Types & Enums
pub use types::{
    BlockCullMeta, CullCategory, GlassCullMode, LeavesCullMode, EMPTY_FACE_RECT, FULL_FACE_RECT,
};
