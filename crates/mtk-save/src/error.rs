use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during Minecraft save reading and chunk parsing.
#[derive(Debug, Error)]
pub enum SaveError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("NBT decoding error: {0}")]
    Nbt(#[from] crate::nbt::NbtError),

    #[error("level.dat not found at: {0:?}")]
    LevelDatNotFound(PathBuf),

    #[error("Region directory not found at: {0:?}")]
    RegionDirNotFound(PathBuf),

    #[error("Invalid region file header: {0}")]
    InvalidRegionHeader(String),

    #[error("Unsupported chunk compression type: {0}")]
    UnsupportedCompression(u8),

    #[error("Chunk decompression failed: {0}")]
    DecompressionFailed(String),

    #[error(
        "Unsupported or legacy Minecraft DataVersion ({0}); minimum supported is 2844 (1.18+)"
    )]
    UnsupportedDataVersion(i32),

    #[error("Corrupt or invalid chunk data: {0}")]
    InvalidChunkData(String),

    #[error("Voxel storage error: {0}")]
    Voxel(#[from] mtk_voxel::VoxelError),
}
