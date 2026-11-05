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

        // Seek to chunk sector offset
        self.file.seek(SeekFrom::Start(loc.file_offset()))?;

        // 4 bytes: length of chunk payload (big endian)
        let mut len_buf = [0u8; 4];
        self.file.read_exact(&mut len_buf)?;
        let chunk_len = u32::from_be_bytes(len_buf);

        if chunk_len == 0 {
            return Ok(None);
        }

        // 1 byte: compression type
        let mut comp_type = [0u8; 1];
        self.file.read_exact(&mut comp_type)?;

        let compressed_size = (chunk_len - 1) as usize;
        let mut compressed_data = vec![0u8; compressed_size];
        self.file.read_exact(&mut compressed_data)?;

        let mut decompressed = Vec::new();
        match comp_type[0] {
            1 => {
                // GZip
                let mut gz = GzDecoder::new(&compressed_data[..]);
                gz.read_to_end(&mut decompressed).map_err(|e| {
                    SaveError::DecompressionFailed(format!("GZip decompression error: {}", e))
                })?;
            }
            2 => {
                // Zlib / Deflate (standard Java Edition)
                let mut zlib = ZlibDecoder::new(&compressed_data[..]);
                zlib.read_to_end(&mut decompressed).map_err(|e| {
                    SaveError::DecompressionFailed(format!("Zlib decompression error: {}", e))
                })?;
            }
            3 => {
                // Uncompressed
                decompressed = compressed_data;
            }
            other => return Err(SaveError::UnsupportedCompression(other)),
        }

        Ok(Some(decompressed))
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
}
