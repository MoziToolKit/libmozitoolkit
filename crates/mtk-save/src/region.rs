use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::{GzDecoder, ZlibDecoder};

use crate::error::SaveError;

/// Number of chunks along one axis of a 32x32 Anvil region file.
pub(crate) const REGION_CHUNKS_AXIS: usize = 32;
/// Total number of chunks in a single region file (32 * 32 = 1024).
pub(crate) const REGION_TOTAL_CHUNKS: usize = REGION_CHUNKS_AXIS * REGION_CHUNKS_AXIS;
/// Size of the Anvil location header in bytes (1024 entries * 4 bytes = 4096).
pub(crate) const REGION_HEADER_SIZE: usize = 4096;
/// Size of one Anvil sector in bytes (4 KiB).
pub(crate) const SECTOR_SIZE: usize = 4096;
/// Maximum allowed size of decompressed chunk data (64 MiB).
pub(crate) const MAX_DECOMPRESSED_CHUNK_SIZE: usize = 64 * 1024 * 1024;

/// Location entry inside an Anvil region file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChunkLocation {
    /// Offset in 4 KiB sectors from the start of the file.
    pub sector_offset: u32,
    /// Number of 4 KiB sectors allocated for this chunk.
    pub sector_count: u8,
}

impl ChunkLocation {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.sector_offset == 0 || self.sector_count == 0
    }

    #[inline]
    pub fn file_offset(&self) -> u64 {
        (self.sector_offset as u64) * (SECTOR_SIZE as u64)
    }
}

/// Anvil MCA region file reader with direct sector seeking and zero unneeded decompression.
#[derive(Debug)]
pub(crate) struct RegionFile {
    file: File,
    locations: [ChunkLocation; REGION_TOTAL_CHUNKS],
}

impl RegionFile {
    /// Opens an Anvil `.mca` region file and loads its 4096-byte chunk location header.
    pub(crate) fn open(path: &Path) -> Result<Self, SaveError> {
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        if file_len < REGION_HEADER_SIZE as u64 {
            return Err(SaveError::InvalidRegionHeader(format!(
                "Region file {} size ({} bytes) is smaller than header size ({} bytes)",
                path.display(),
                file_len,
                REGION_HEADER_SIZE
            )));
        }

        let mut header = [0u8; REGION_HEADER_SIZE];
        file.read_exact(&mut header).map_err(|e| {
            SaveError::InvalidRegionHeader(format!(
                "Header read failed for {}: {}",
                path.display(),
                e
            ))
        })?;

        let mut locations = [ChunkLocation {
            sector_offset: 0,
            sector_count: 0,
        }; REGION_TOTAL_CHUNKS];
        for i in 0..REGION_TOTAL_CHUNKS {
            let offset_idx = i * 4;
            let sector_offset = ((header[offset_idx] as u32) << 16)
                | ((header[offset_idx + 1] as u32) << 8)
                | (header[offset_idx + 2] as u32);
            let sector_count = header[offset_idx + 3];
            locations[i] = ChunkLocation {
                sector_offset,
                sector_count,
            };
        }

        Ok(Self { file, locations })
    }

    /// Computes the linear chunk index (0..1024) within a region from world chunk coordinates.
    #[inline]
    pub(crate) fn chunk_index(chunk_x: i32, chunk_z: i32) -> usize {
        let rel_x = chunk_x.rem_euclid(REGION_CHUNKS_AXIS as i32) as usize;
        let rel_z = chunk_z.rem_euclid(REGION_CHUNKS_AXIS as i32) as usize;
        rel_x + rel_z * REGION_CHUNKS_AXIS
    }

    /// Checks if a chunk at world chunk coordinates `(chunk_x, chunk_z)` exists in this region file.
    #[inline]
    pub(crate) fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        let idx = Self::chunk_index(chunk_x, chunk_z);
        !self.locations[idx].is_empty()
    }

    /// Gets the chunk location entry for world chunk coordinates `(chunk_x, chunk_z)`.
    #[inline]
    pub(crate) fn chunk_location(&self, chunk_x: i32, chunk_z: i32) -> ChunkLocation {
        let idx = Self::chunk_index(chunk_x, chunk_z);
        self.locations[idx]
    }

    /// Reads and decompresses chunk payload bytes for world chunk `(chunk_x, chunk_z)`.
    ///
    /// Returns `Ok(None)` if the chunk is not generated or empty.
    pub(crate) fn read_chunk_decompressed(
        &mut self,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Result<Option<Vec<u8>>, SaveError> {
        let loc = self.chunk_location(chunk_x, chunk_z);
        if loc.is_empty() {
            return Ok(None);
        }

        if loc.sector_offset < 2 {
            return Err(SaveError::InvalidRegionHeader(format!(
                "Chunk ({chunk_x}, {chunk_z}) has invalid sector offset: {}",
                loc.sector_offset
            )));
        }

        let file_len = self.file.metadata()?.len();
        let sector_end = (loc.sector_offset as u64)
            .checked_add(loc.sector_count as u64)
            .ok_or_else(|| {
                SaveError::InvalidRegionHeader("Sector offset calculation overflow".into())
            })?
            .checked_mul(SECTOR_SIZE as u64)
            .ok_or_else(|| {
                SaveError::InvalidRegionHeader("Sector byte offset calculation overflow".into())
            })?;

        if sector_end > file_len {
            return Err(SaveError::InvalidRegionHeader(format!(
                "Chunk ({chunk_x}, {chunk_z}) sector allocation extends past EOF ({sector_end} > {file_len})"
            )));
        }

        // Seek to chunk sector offset
        self.file.seek(SeekFrom::Start(loc.file_offset()))?;

        // 4 bytes: length of chunk payload (big endian)
        let mut len_buf = [0u8; 4];
        self.file.read_exact(&mut len_buf).map_err(|e| {
            SaveError::InvalidChunkData(format!(
                "Failed to read chunk length for ({chunk_x}, {chunk_z}): {e}"
            ))
        })?;
        let chunk_len = u32::from_be_bytes(len_buf);

        if chunk_len == 0 {
            return Ok(None);
        }

        let allocated_bytes = (loc.sector_count as u64) * (SECTOR_SIZE as u64);
        let max_payload_len = allocated_bytes.saturating_sub(4);
        if (chunk_len as u64) > max_payload_len {
            return Err(SaveError::InvalidChunkData(format!(
                "Chunk length {chunk_len} exceeds allocated sector capacity ({max_payload_len} bytes in {} sectors)",
                loc.sector_count
            )));
        }

        let chunk_payload_end = loc
            .file_offset()
            .checked_add(4)
            .and_then(|v| v.checked_add(chunk_len as u64))
            .ok_or_else(|| {
                SaveError::InvalidChunkData("Chunk payload offset calculation overflow".into())
            })?;
        if chunk_payload_end > file_len {
            return Err(SaveError::InvalidChunkData(format!(
                "Chunk payload extends past file length ({chunk_payload_end} > {file_len})"
            )));
        }

        // 1 byte: compression type
        let mut comp_type = [0u8; 1];
        self.file.read_exact(&mut comp_type)?;

        let compressed_size = (chunk_len - 1) as usize;
        let mut compressed_data = vec![0u8; compressed_size];
        self.file.read_exact(&mut compressed_data)?;

        let decompressed = match comp_type[0] {
            1 => {
                // GZip
                let gz = GzDecoder::new(&compressed_data[..]);
                Self::decompress_limited(gz, "GZip")?
            }
            2 => {
                // Zlib / Deflate (standard Java Edition)
                let zlib = ZlibDecoder::new(&compressed_data[..]);
                Self::decompress_limited(zlib, "Zlib")?
            }
            3 => {
                // Uncompressed
                if compressed_data.len() > MAX_DECOMPRESSED_CHUNK_SIZE {
                    return Err(SaveError::InvalidChunkData(format!(
                        "Uncompressed chunk size {} exceeds 64 MiB limit",
                        compressed_data.len()
                    )));
                }
                compressed_data
            }
            other => return Err(SaveError::UnsupportedCompression(other)),
        };

        Ok(Some(decompressed))
    }

    fn decompress_limited<R: Read>(mut reader: R, codec: &str) -> Result<Vec<u8>, SaveError> {
        let mut decompressed = Vec::new();
        let mut take = (&mut reader).take((MAX_DECOMPRESSED_CHUNK_SIZE as u64) + 1);
        take.read_to_end(&mut decompressed).map_err(|e| {
            SaveError::DecompressionFailed(format!("{codec} decompression error: {e}"))
        })?;
        if decompressed.len() > MAX_DECOMPRESSED_CHUNK_SIZE {
            return Err(SaveError::DecompressionFailed(format!(
                "{codec} decompressed chunk exceeded 64 MiB limit (got {} bytes)",
                decompressed.len()
            )));
        }
        Ok(decompressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_chunk_indexing() {
        assert_eq!(RegionFile::chunk_index(0, 0), 0);
        assert_eq!(RegionFile::chunk_index(1, 0), 1);
        assert_eq!(RegionFile::chunk_index(0, 1), 32);
        assert_eq!(RegionFile::chunk_index(31, 31), 1023);

        // Negative coordinates wrap using Euclidean remainder
        assert_eq!(RegionFile::chunk_index(-1, -1), 31 + 31 * 32);
        assert_eq!(RegionFile::chunk_index(-32, 0), 0);
    }

    #[test]
    fn test_region_file_reads_gzip_chunk() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        let payload = b"gzip-compressed chunk payload".repeat(16);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&payload).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() + 5 < SECTOR_SIZE);

        // 4 KiB location header + one data sector. Chunk (0,0) starts at sector 2.
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 2]);
        bytes[3] = 1;
        let data_offset = SECTOR_SIZE * 2;
        let declared_len = (compressed.len() + 1) as u32;
        bytes[data_offset..data_offset + 4].copy_from_slice(&declared_len.to_be_bytes());
        bytes[data_offset + 4] = 1; // GZip
        bytes[data_offset + 5..data_offset + 5 + compressed.len()].copy_from_slice(&compressed);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_gzip_{}_{}.mca",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();

        let mut region = RegionFile::open(&path).unwrap();
        let decoded = region
            .read_chunk_decompressed(0, 0)
            .unwrap()
            .expect("gzip chunk should be present");
        assert_eq!(decoded, payload);
        drop(region);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_region_location_pointing_past_eof_returns_err() {
        use std::time::{SystemTime, UNIX_EPOCH};

        // 3 sectors total (12 KiB). Chunk (0,0) location points to sector 100.
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 100]);
        bytes[3] = 1;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_past_eof_{}_{}.mca",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();

        let mut region = RegionFile::open(&path).unwrap();
        let result = region.read_chunk_decompressed(0, 0);
        assert!(
            matches!(result, Err(SaveError::InvalidRegionHeader(_))),
            "Expected InvalidRegionHeader, got: {:?}",
            result
        );
        drop(region);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_region_location_invalid_low_sector_offset_returns_err() {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Chunk (0,0) points to sector 1 (timestamp table), which is < 2.
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 1]);
        bytes[3] = 1;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_low_sector_{}_{}.mca",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();

        let mut region = RegionFile::open(&path).unwrap();
        let result = region.read_chunk_decompressed(0, 0);
        assert!(
            matches!(result, Err(SaveError::InvalidRegionHeader(_))),
            "Expected InvalidRegionHeader, got: {:?}",
            result
        );
        drop(region);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_region_chunk_length_exceeding_sector_capacity_returns_err() {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Chunk (0,0) starts at sector 2 with sector_count 1 (4 KiB max).
        // File has 3 sectors (12 KiB).
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 2]);
        bytes[3] = 1;

        let data_offset = SECTOR_SIZE * 2;
        // Hostile chunk length: 0x7FFF_FFFF (2 GiB)
        let hostile_len: u32 = 0x7FFF_FFFF;
        bytes[data_offset..data_offset + 4].copy_from_slice(&hostile_len.to_be_bytes());

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_huge_len_{}_{}.mca",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();

        let mut region = RegionFile::open(&path).unwrap();
        let result = region.read_chunk_decompressed(0, 0);
        assert!(
            matches!(result, Err(SaveError::InvalidChunkData(_))),
            "Expected InvalidChunkData, got: {:?}",
            result
        );
        drop(region);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_region_decompressed_gzip_exceeding_cap_returns_err() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        // 65 MiB of zeros compresses to ~65 KiB
        let bomb_payload = vec![0u8; 65 * 1024 * 1024];
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&bomb_payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let sectors_needed = (compressed.len() + 5).div_ceil(SECTOR_SIZE) as u8;
        let total_sectors = 2 + sectors_needed as usize;
        let mut bytes = vec![0u8; SECTOR_SIZE * total_sectors];
        bytes[0..3].copy_from_slice(&[0, 0, 2]);
        bytes[3] = sectors_needed;

        let data_offset = SECTOR_SIZE * 2;
        let declared_len = (compressed.len() + 1) as u32;
        bytes[data_offset..data_offset + 4].copy_from_slice(&declared_len.to_be_bytes());
        bytes[data_offset + 4] = 1; // GZip
        bytes[data_offset + 5..data_offset + 5 + compressed.len()].copy_from_slice(&compressed);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_gz_bomb_{}_{}.mca",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();

        let mut region = RegionFile::open(&path).unwrap();
        let result = region.read_chunk_decompressed(0, 0);
        assert!(
            matches!(result, Err(SaveError::DecompressionFailed(_))),
            "Expected DecompressionFailed, got: {:?}",
            result
        );
        drop(region);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_region_decompressed_zlib_exceeding_cap_returns_err() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        // 65 MiB of zeros
        let bomb_payload = vec![0u8; 65 * 1024 * 1024];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&bomb_payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let sectors_needed = (compressed.len() + 5).div_ceil(SECTOR_SIZE) as u8;
        let total_sectors = 2 + sectors_needed as usize;
        let mut bytes = vec![0u8; SECTOR_SIZE * total_sectors];
        bytes[0..3].copy_from_slice(&[0, 0, 2]);
        bytes[3] = sectors_needed;

        let data_offset = SECTOR_SIZE * 2;
        let declared_len = (compressed.len() + 1) as u32;
        bytes[data_offset..data_offset + 4].copy_from_slice(&declared_len.to_be_bytes());
        bytes[data_offset + 4] = 2; // Zlib
        bytes[data_offset + 5..data_offset + 5 + compressed.len()].copy_from_slice(&compressed);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_zlib_bomb_{}_{}.mca",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();

        let mut region = RegionFile::open(&path).unwrap();
        let result = region.read_chunk_decompressed(0, 0);
        assert!(
            matches!(result, Err(SaveError::DecompressionFailed(_))),
            "Expected DecompressionFailed, got: {:?}",
            result
        );
        drop(region);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_malformed_location_table_file_too_small_returns_err() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mtk_save_too_small_{}_{}.mca",
            std::process::id(),
            unique
        ));
        // Only 100 bytes (header requires 4096 bytes)
        std::fs::write(&path, [0u8; 100]).unwrap();

        let result = RegionFile::open(&path);
        assert!(
            matches!(result, Err(SaveError::InvalidRegionHeader(_))),
            "Expected InvalidRegionHeader, got: {:?}",
            result
        );
        std::fs::remove_file(&path).unwrap();
    }
}
