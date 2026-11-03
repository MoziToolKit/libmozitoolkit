#![forbid(unsafe_code)]
//! # mtk-voxel
//!
//! **mtk-voxel** provides Minecraft chunk section and world-level voxel storage,
//! physically-accurate fluid surface meshing, smooth ambient occlusion lighting,
//! biome color blending, high-performance world mesh assembly, and abstract
//! voxel source interfaces for multi-source ingestion.

pub mod biome;
pub mod fluid;
pub mod mesher;
pub mod source;
pub mod storage;
pub mod types;

pub use biome::{
    get_biome_meta, get_colormap_uv, get_smoothed_biome_data, get_smoothed_column_biome, BiomeMeta,
    SmoothedBiomeColumn,
};
pub use fluid::{
    batch_repair_fluid_uv, calculate_corner_average, calculate_fluid_corner_heights,
    calculate_fluid_flow_vector, emit_fluid_geometry, get_fluid_base_height, get_fluid_side_uvs,
    get_fluid_side_uvs_vec2, get_fluid_top_uvs, get_fluid_top_uvs_vec2,
    repair_mesh_fluid_uv_inplace, repair_quad_fluid_uv, sample_fluid_height, FluidType,
    MAX_FLUID_HEIGHT,
};
pub use mesher::{
    ao_brightness_to_level, ao_level_to_brightness, calculate_face_ao, calculate_face_ao_float,
    calculate_face_ao_with_inset, is_face_cubic, is_face_inset, should_flip_quad_diagonal,
    vertex_ao, DeltaMesher, SectionMesher,
};
pub use source::{ingest_from_source, VoxelReader, VoxelSource, VoxelWriter};
pub use storage::{
    block_index, crc32, crc32_update, extract_canonical_state_str, get_empty_section_crc,
    padded_index, BlockEntityData, PaddedVoxelArray, PointCloudVoxelSource, SectionStorage,
    SparseVoxelColumns, SparseVoxelPayload, VoxelPointCloud, VoxelStorage, EMPTY_SECTION_CRC,
    PADDED_SIZE, PADDED_VOLUME, SECTION_SIZE, SECTION_VOLUME,
};
pub use types::{CoordinateSystem, MesherConfig, VoxelError, WorldMeshBuildResult};
pub use world::VoxelWorld;

pub mod world;
