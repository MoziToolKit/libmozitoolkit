#![forbid(unsafe_code)]

extern crate alloc;

// Private internal modules
mod attributes;
pub mod constants;
mod direction;
mod extrude;
mod geometry;
mod mesh;
mod polygon;
mod profile;
mod progress;
mod random;
mod subdivide;
mod uv;

// 1. Mesh Attributes & Schema
pub use attributes::{
    constants as attribute_constants, AttributeData, AttributeDomain, FaceAttributes,
    MeshAttribute, MESH_ATTRIBUTE_SPECS, RETIRED_MESH_ATTRIBUTES,
};

// 2. Topology & Direction
pub use direction::{DirMask, Direction};

// 3. Extrusion & Side UV Repair
pub use extrude::{
    generate_extrude_heights, perlin_noise_3d, process_flat_mesh_extrude_repair,
    repair_extruded_side_uv, ExtrudeMeshOutput, ExtrudeNoiseType, ExtrudeUvMode,
    MeshExtrudeRepairConfig,
};

// 4. Geometry & Math
pub use geometry::{
    derive_quad_face_tangents, mc_rotation_to_z_up, mc_scale_to_z_up, mc_world_to_z_up,
    z_up_rotation_to_mc, z_up_scale_to_mc, z_up_to_mc_world, Aabb2d, CoordinateSystem, Quad,
    MC_TO_Z_UP_BASIS, Z_UP_TO_MC_BASIS,
};

// 5. Mesh Data & Topology Analysis
pub use mesh::{
    analyze_hard_edges, greedy_mesh_2d_mask, greedy_mesh_2d_values, greedy_span_1d, GreedyRect,
    MeshData, UvLayout,
};
pub use polygon::FlatPolygonMesh;

// 6. Profiling & Diagnostics
pub use profile::{ExecutionProfile, ExecutionProfiler, StageSpan};

// 7. Progress Reporting
pub use progress::{ProgressCallback, ProgressReport, ProgressThrottler};

// 8. Minecraft Coordinate Seed & Random Offset
pub use random::{
    determine_block_offset_type, get_block_offset, mc_coordinate_seed, JavaRandom, OffsetType,
};

// 9. Adaptive Pixel Subdivision & Splitting
pub use subdivide::{
    adaptive_pixel_split_mesh, calculate_face_target_grid, calculate_pixel_grid_cut_factors,
    slice_face_by_pixel_grid,
};

// 10. UV Geometry & Analysis
pub use uv::{
    calculate_uv_area, detect_uv_rotation, get_uv_bounds, get_uv_center, is_orthogonal_angle,
    is_uv_collapsed, normalize_uv_for_atlas_tiling, restore_atlas_tiling_uv, scale_uv,
    straighten_uv, uv_requires_atlas_tiling,
};

// Re-export glam types for convenience
pub use glam::{IVec3, Vec2, Vec3};
