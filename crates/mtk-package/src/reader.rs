//! Zero-copy memory-mapped package reader and decoder.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::{PackageError, PackageResult};
use crate::header::{CompressionType, PackageHeader, HEADER_SIZE};
use crate::toc::{TableOfContents, MAX_DECOMPRESSED_CHUNK_SIZE};

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
        let file_len = slice.len() as u64;
        if file_len < HEADER_SIZE as u64 {
            return Err(PackageError::OutOfBounds {
                offset: 0,
                length: HEADER_SIZE as u64,
                file_size: file_len,
            });
        }

        // 1. Parse and validate 64-byte header
        let header = PackageHeader::from_bytes(&slice[0..HEADER_SIZE])?;

        // 2. Locate and parse TOC
        let oob = || PackageError::OutOfBounds {
            offset: header.toc_offset,
            length: header.toc_length,
            file_size: file_len,
        };
        let toc_start = usize::try_from(header.toc_offset).map_err(|_| oob())?;
        let toc_len = usize::try_from(header.toc_length).map_err(|_| oob())?;
        let toc_end = toc_start.checked_add(toc_len).ok_or_else(oob)?;
        if toc_end as u64 > file_len || header.toc_offset < HEADER_SIZE as u64 {
            return Err(oob());
        }
        if header.toc_offset % 64 != 0 {
            return Err(PackageError::UnalignedOffset {
                offset: header.toc_offset,
            });
        }

        let toc_slice = &slice[toc_start..toc_end];
        let toc = if header.flags.is_toc_zstd() {
            TableOfContents::decode_zstd(toc_slice)?
        } else {
            TableOfContents::decode_bytes(toc_slice)?
        };

        // 3. Validate header.chunk_count matches TOC entries length
        if header.chunk_count as usize != toc.entries.len() {
            return Err(PackageError::InvalidToc(format!(
                "Chunk count mismatch: header declared {}, TOC contains {}",
                header.chunk_count,
                toc.entries.len()
            )));
        }

        // 4. Validate all TOC entries: offsets, alignment, bounds, duplicates, and non-overlapping
        let mut seen_ids = std::collections::HashSet::new();
        let mut intervals = Vec::with_capacity(toc.entries.len() + 1);

        for entry in &toc.entries {
            if !seen_ids.insert(&entry.identifier) {
                return Err(PackageError::InvalidToc(format!(
                    "Duplicate chunk identifier: '{}'",
                    entry.identifier
                )));
            }

            if entry.offset < HEADER_SIZE as u64 {
                return Err(PackageError::OutOfBounds {
                    offset: entry.offset,
                    length: entry.compressed_size,
                    file_size: file_len,
                });
            }

            if entry.offset % 64 != 0 {
                return Err(PackageError::UnalignedOffset {
                    offset: entry.offset,
                });
            }

            let end = entry.offset.checked_add(entry.compressed_size).ok_or(
                PackageError::OutOfBounds {
                    offset: entry.offset,
                    length: entry.compressed_size,
                    file_size: file_len,
                },
            )?;

            if end > file_len {
                return Err(PackageError::OutOfBounds {
                    offset: entry.offset,
                    length: entry.compressed_size,
                    file_size: file_len,
                });
            }

            intervals.push((entry.offset, end, entry.identifier.as_str()));
        }

        // Add TOC interval to verify chunks do not overlap with TOC
        let toc_end_u64 = header
            .toc_offset
            .checked_add(header.toc_length)
            .ok_or_else(oob)?;
        intervals.push((header.toc_offset, toc_end_u64, "[TOC]"));

        // Sort intervals by start offset, then end offset
        intervals.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        for i in 0..intervals.len().saturating_sub(1) {
            let (curr_start, curr_end, curr_id) = intervals[i];
            let (next_start, _, next_id) = intervals[i + 1];

            if curr_end > next_start || curr_start == next_start {
                return Err(PackageError::InvalidToc(format!(
                    "Overlapping package entries: '{}' [{curr_start}..{curr_end}) and '{}' starting at {next_start}",
                    curr_id, next_id
                )));
            }
        }

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

        if entry.decompressed_size > MAX_DECOMPRESSED_CHUNK_SIZE {
            return Err(PackageError::DecompressionFailed {
                identifier: identifier.to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Declared decompressed size {} exceeds limit of {} bytes",
                        entry.decompressed_size, MAX_DECOMPRESSED_CHUNK_SIZE
                    ),
                ),
            });
        }

        let raw_payload = self.get_chunk_raw(identifier)?;

        let decompressed = match entry.compression {
            CompressionType::None => {
                if raw_payload.len() as u64 != entry.decompressed_size {
                    return Err(PackageError::DecompressionFailed {
                        identifier: identifier.to_string(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(
                                "Decompressed size mismatch for uncompressed chunk '{}': declared {}, raw {}",
                                identifier,
                                entry.decompressed_size,
                                raw_payload.len()
                            ),
                        ),
                    });
                }
                raw_payload.to_vec()
            }
            CompressionType::Zstd => {
                let initial_cap = (entry.decompressed_size as usize).min(1024 * 1024);
                let buf = crate::codec::decompress_zstd_limited(
                    raw_payload,
                    MAX_DECOMPRESSED_CHUNK_SIZE,
                    identifier,
                    initial_cap,
                )?;
                if buf.len() as u64 != entry.decompressed_size {
                    return Err(PackageError::DecompressionFailed {
                        identifier: identifier.to_string(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(
                                "Decompressed size mismatch for chunk '{}': declared {}, got {}",
                                identifier,
                                entry.decompressed_size,
                                buf.len()
                            ),
                        ),
                    });
                }
                buf
            }
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
        if identifier.is_empty() {
            return Err(PackageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Chunk identifier cannot be empty for file extraction",
            )));
        }

        let id_path = Path::new(identifier);
        for comp in id_path.components() {
            match comp {
                std::path::Component::ParentDir => {
                    return Err(PackageError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "Path traversal with '..' rejected in chunk id '{}'",
                            identifier
                        ),
                    )));
                }
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                    return Err(PackageError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Absolute path rejected in chunk id '{}'", identifier),
                    )));
                }
                _ => {}
            }
        }

        for segment in identifier.split(['/', '\\']) {
            if segment == ".." {
                return Err(PackageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Path traversal with '..' rejected in chunk id '{}'",
                        identifier
                    ),
                )));
            }
        }

        let data = self.read_chunk_decompressed(identifier)?;
        let target = target_path.as_ref();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, data)?;
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
