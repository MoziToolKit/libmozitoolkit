#![forbid(unsafe_code)]

extern crate alloc;
pub mod attributes;
pub mod constants;
pub mod direction;
pub mod extrude;
pub mod geometry;
pub mod mesh;
pub mod polygon;
pub mod profile;
pub mod progress;
pub mod random;
pub mod subdivide;
pub mod uv;

pub use attributes::{
    AttributeData, AttributeDomain, FaceAttributes, LightLevel, MaterialSlotId, MeshAttribute,
    TintIndex,
};
pub use constants::concurrency;
pub use direction::{DirMask, Direction};
pub use extrude::{
    calculate_uv_area, cellular_noise_3d, generate_extrude_heights,
    is_uv_collapsed as is_mesh_uv_collapsed, perlin_noise_3d, repair_extruded_side_uv,
    ExtrudeMeshOutput, ExtrudeNoiseType, ExtrudeUvMode, MeshExtrudeRepairConfig,
};
pub use geometry::{
    derive_quad_face_tangents, mc_local_to_centered_z_up, mc_world_to_z_up, Aabb2d, Quad,
};
pub use mesh::{analyze_hard_edges, MeshData, UvLayout};
pub use polygon::FlatPolygonMesh;
pub use profile::{ExecutionProfile, ExecutionProfiler, StageSpan};
pub use progress::{ProgressCallback, ProgressReport, ProgressThrottler};
pub use random::{
    determine_block_offset_type, get_block_offset, mc_coordinate_seed, JavaRandom, OffsetType,
};
pub use subdivide::{
    adaptive_pixel_split_mesh, calculate_face_target_grid, interpolate_bilinear_2d,
    interpolate_bilinear_3d, interpolate_bilinear_4d,
};

// Re-export glam types for convenience
pub use glam::{IVec3, Vec2, Vec3, Vec4};
