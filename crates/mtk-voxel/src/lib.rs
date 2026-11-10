#![forbid(unsafe_code)]
//! # mtk-voxel
//!
//! **mtk-voxel** provides Minecraft chunk section and world-level voxel storage,
//! physically-accurate fluid surface meshing, smooth ambient occlusion lighting,
//! biome color blending, high-performance world mesh assembly, and abstract
//! voxel source interfaces for multi-source ingestion.

mod biome;
mod fluid;
mod mesher;
mod source;
mod storage;
mod types;
mod world;

pub use biome::{get_point_column_biome, get_smoothed_column_biome, SmoothedBiomeColumn};
pub use fluid::{
    batch_repair_fluid_uv, get_fluid_side_uvs, get_fluid_side_uvs_vec2, get_fluid_top_uvs,
    get_fluid_top_uvs_vec2, repair_mesh_fluid_uv_inplace, repair_quad_fluid_uv, FluidType,
};
pub use mesher::{DeltaMesher, ModelSource, SectionMesher};
pub use source::{ingest_from_source, VoxelReader, VoxelSource, VoxelWriter};
pub use storage::{
    block_index, BlockEntityData, PaddedVoxelArray, PointCloudVoxelSource, SectionBiomes,
    SectionStorage, SparseVoxelColumns, SparseVoxelPayload, VoxelPointCloud, VoxelStorage,
};
pub use types::{CoordinateSystem, MesherConfig, UnknownBlockStrategy, VoxelError};
pub use world::{SkinResolvedCallback, VoxelWorld};
