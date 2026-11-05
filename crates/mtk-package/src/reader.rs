//! Zero-copy memory-mapped package reader and decoder.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::error::{PackageError, PackageResult};
use crate::header::{CompressionType, PackageHeader, HEADER_SIZE};
use crate::toc::TableOfContents;

enum Storage {
    #[cfg(feature = "mmap")]
    Mmap(memmap2::Mmap),
    Buffer(Vec<u8>),
}

impl Storage {
    fn as_slice(&self) -> &[u8] {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(mmap) => mmap.as_ref(),
            Self::Buffer(buf) => buf.as_slice(),
        }
    }
}

/// High-performance reader for MTK binary package files.
pub struct MtkPackageReader {
    storage: Storage,
    header: PackageHeader,
    toc: TableOfContents,
}

impl MtkPackageReader {
    /// Reads and validates only the 64-byte header of a file without reading the whole file.
    pub fn read_header_only(path: impl AsRef<Path>) -> PackageResult<PackageHeader> {
        let mut file = File::open(path)?;
        let mut buf = [0u8; HEADER_SIZE];
        file.read_exact(&mut buf)?;
        PackageHeader::from_bytes(&buf)
    }

    /// Opens a package file from disk using memory mapping.
    ///
    /// # Safety (OS level)
    /// This uses `memmap2::MmapOptions::map` internally. Memory-mapped files assume
    /// the underlying file is not truncated or concurrently modified by an external process.
    #[cfg(feature = "mmap")]
    #[allow(unsafe_code)] // `MmapOptions::map` is inherently unsafe; see SAFETY note below.
    pub fn open_file(path: impl AsRef<Path>) -> PackageResult<Self> {
        let file = File::open(path)?;
        // SAFETY: We map the file in read-only mode and do not modify the file descriptor.
        // It is assumed the underlying package file is not truncated concurrently by external processes.
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
        Self::from_storage(Storage::Mmap(mmap))
    }

    /// Opens a package file from disk by reading it into memory safely without memory mapping.
    pub fn open_file_buffered(path: impl AsRef<Path>) -> PackageResult<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(bytes)
    }

    /// Opens a package file from disk (buffered fallback when `mmap` feature is disabled).
    #[cfg(not(feature = "mmap"))]
    pub fn open_file(path: impl AsRef<Path>) -> PackageResult<Self> {
        Self::open_file_buffered(path)
    }

    /// Creates a package reader from in-memory bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> PackageResult<Self> {
        Self::from_storage(Storage::Buffer(bytes))
    }

    fn from_storage(storage: Storage) -> PackageResult<Self> {
        let slice = storage.as_slice();
        if slice.len() < HEADER_SIZE {
            return Err(PackageError::OutOfBounds {
                offset: 0,
                length: HEADER_SIZE as u64,
                file_size: slice.len() as u64,
            });
        }

        // 1. Parse and validate 64-byte header
        let header = PackageHeader::from_bytes(&slice[0..HEADER_SIZE])?;

        // 2. Locate and parse TOC
        let oob = || PackageError::OutOfBounds {
            offset: header.toc_offset,
            length: header.toc_length,
            file_size: slice.len() as u64,
        };
        let toc_start = usize::try_from(header.toc_offset).map_err(|_| oob())?;
        let toc_len = usize::try_from(header.toc_length).map_err(|_| oob())?;
        let toc_end = toc_start.checked_add(toc_len).ok_or_else(oob)?;
        if toc_end > slice.len() {
            return Err(oob());
        }

        let toc_slice = &slice[toc_start..toc_end];
        let toc = if header.flags.is_toc_zstd() {
            TableOfContents::decode_zstd(toc_slice)?
        } else {
            TableOfContents::decode_bytes(toc_slice)?
        };

        Ok(Self {
            storage,
            header,
            toc,
        })
    }

    /// Returns reference to package header.
    pub fn header(&self) -> &PackageHeader {
        &self.header
    }

    /// Returns 16-byte fingerprint from package header.
    pub fn fingerprint(&self) -> &[u8; 16] {
        &self.header.fingerprint
    }

    /// Returns an iterator over identifiers of chunks matching the given 4-byte chunk type.
    pub fn iter_chunk_identifiers<'a>(
        &'a self,
        chunk_type: [u8; 4],
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.toc
            .iter_by_type(chunk_type)
            .map(|e| e.identifier.as_str())
    }

    /// Returns true if a chunk with the given identifier exists.
    pub fn has_chunk(&self, identifier: &str) -> bool {
        self.toc.get_by_identifier(identifier).is_some()
    }

    /// Returns a zero-copy slice of the raw/compressed chunk payload.
    pub fn get_chunk_raw(&self, identifier: &str) -> PackageResult<&[u8]> {
        let entry = self
            .toc
            .get_by_identifier(identifier)
            .ok_or_else(|| PackageError::ChunkNotFound(identifier.to_string()))?;

        let slice = self.storage.as_slice();
        let start = usize::try_from(entry.offset).map_err(|_| PackageError::OutOfBounds {
            offset: entry.offset,
            length: entry.compressed_size,
            file_size: slice.len() as u64,
        })?;
        let size =
            usize::try_from(entry.compressed_size).map_err(|_| PackageError::OutOfBounds {
                offset: entry.offset,
                length: entry.compressed_size,
                file_size: slice.len() as u64,
            })?;
        let end = start
            .checked_add(size)
            .filter(|&end| end <= slice.len())
            .ok_or(PackageError::OutOfBounds {
                offset: entry.offset,
                length: entry.compressed_size,
                file_size: slice.len() as u64,
            })?;

        Ok(&slice[start..end])
    }

    /// Reads, decompresses (if compressed), and verifies checksum of chunk payload.
    pub fn read_chunk_decompressed(&self, identifier: &str) -> PackageResult<Vec<u8>> {
        let entry = self
            .toc
            .get_by_identifier(identifier)
            .ok_or_else(|| PackageError::ChunkNotFound(identifier.to_string()))?;

        let raw_payload = self.get_chunk_raw(identifier)?;

        let decompressed = match entry.compression {
            CompressionType::None => raw_payload.to_vec(),
            CompressionType::Zstd => zstd::decode_all(Cursor::new(raw_payload)).map_err(|e| {
                PackageError::DecompressionFailed {
                    identifier: identifier.to_string(),
                    source: e,
                }
            })?,
        };

        // Verify checksum on decompressed payload
        let calculated_crc = crc32fast::hash(&decompressed) as u64;
        if calculated_crc != entry.checksum {
            return Err(PackageError::ChunkChecksumMismatch {
                identifier: identifier.to_string(),
                expected: entry.checksum,
                calculated: calculated_crc,
            });
        }

        Ok(decompressed)
    }

    /// Reads and parses chunk as UTF-8 string.
    pub fn read_chunk_str(&self, identifier: &str) -> PackageResult<String> {
        let bytes = self.read_chunk_decompressed(identifier)?;
        String::from_utf8(bytes)
            .map_err(|e| PackageError::Serialization(format!("Invalid UTF-8: {}", e)))
    }

    /// Reads and deserializes JSON chunk.
    pub fn read_chunk_json<T: serde::de::DeserializeOwned>(
        &self,
        identifier: &str,
    ) -> PackageResult<T> {
        let bytes = self.read_chunk_decompressed(identifier)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| PackageError::Serialization(format!("Invalid JSON: {}", e)))
    }

    /// Extracts a chunk directly to a file on disk.
    pub fn extract_chunk_to_file(
        &self,
        identifier: &str,
        target_path: impl AsRef<Path>,
    ) -> PackageResult<()> {
        let data = self.read_chunk_decompressed(identifier)?;
        if let Some(parent) = target_path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target_path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forged_toc_offsets_rejected() {
        let mut header = PackageHeader::new_asset_cache([0x11; 16], 0);
        header.toc_offset = u64::MAX;
        header.toc_length = 2;

        let bytes = header.to_bytes().to_vec();
        match MtkPackageReader::from_bytes(bytes) {
            Err(PackageError::OutOfBounds { .. }) => {}
            other => panic!("Expected OutOfBounds, got {:?}", other.is_ok()),
        }
    }
}
