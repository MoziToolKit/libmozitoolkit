//! Package error types.

use thiserror::Error;

/// Result type alias for package operations.
pub type PackageResult<T> = Result<T, PackageError>;

/// Errors that can occur during package reading, writing, and parsing.
#[derive(Debug, Error)]
pub enum PackageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid magic: expected {expected:?}, got {found:?}")]
    InvalidMagic { expected: [u8; 4], found: [u8; 4] },

    #[error("Unsupported version: major {major}, minor {minor}")]
    UnsupportedVersion { major: u16, minor: u16 },

    #[error("Header CRC32 mismatch: expected {expected:#010x}, calculated {calculated:#010x}")]
    HeaderCrcMismatch { expected: u32, calculated: u32 },

    #[error("Chunk checksum mismatch for '{identifier}': expected {expected:#018x}, calculated {calculated:#018x}")]
    ChunkChecksumMismatch {
        identifier: String,
        expected: u64,
        calculated: u64,
    },

    #[error("Invalid offset or length: offset {offset}, length {length}, file_size {file_size}")]
    OutOfBounds {
        offset: u64,
        length: u64,
        file_size: u64,
    },

    #[error("Unaligned chunk offset: {offset} is not 64-byte aligned")]
    UnalignedOffset { offset: u64 },

    #[error("Chunk not found: '{0}'")]
    ChunkNotFound(String),

    #[error("Decompression failed for '{identifier}': {source}")]
    DecompressionFailed {
        identifier: String,
        source: std::io::Error,
    },

    #[error("Compression failed for '{identifier}': {source}")]
    CompressionFailed {
        identifier: String,
        source: std::io::Error,
    },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid TOC format: {0}")]
    InvalidToc(String),

    #[error("Chunk '{identifier}' size {size} exceeds maximum limit of {limit} bytes")]
    ChunkTooLarge {
        identifier: String,
        size: u64,
        limit: u64,
    },

    #[error("Package writer has already been finalized")]
    WriterClosed,
}
