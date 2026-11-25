use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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
/// Compression byte bitflag indicating the chunk payload is stored in an external `.mcc` file.
const EXTERNAL_STREAM_FLAG: u8 = 0x80;

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

/// Source for optional `.mcc` external oversized chunk streams.
#[derive(Debug)]
pub(crate) enum ExternalChunkSource<'a> {
    Dir(PathBuf),
    Memory(&'a HashMap<(i32, i32), Vec<u8>>),
}

/// Anvil MCA region reader with direct sector seeking and zero unneeded decompression.
#[derive(Debug)]
pub(crate) struct RegionFile<'a, R: Read + Seek> {
    external: ExternalChunkSource<'a>,
    reader: R,
    stream_len: u64,
    locations: [ChunkLocation; REGION_TOTAL_CHUNKS],
}

impl RegionFile<'static, File> {
    /// Opens an Anvil `.mca` region file on disk and loads its 4096-byte chunk location header.
    pub(crate) fn open(path: &Path) -> Result<Self, SaveError> {
        let file = File::open(path)?;
        let dir = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let label = path.display().to_string();
        Self::from_reader(file, &label, ExternalChunkSource::Dir(dir))
    }
}

impl<'a> RegionFile<'a, Cursor<&'a [u8]>> {
    /// Opens an Anvil `.mca` region from an in-memory byte slice and optional `.mcc` lookup map.
    pub(crate) fn from_bytes(
        bytes: &'a [u8],
        label: &str,
        external_chunks: &'a HashMap<(i32, i32), Vec<u8>>,
    ) -> Result<Self, SaveError> {
        Self::from_reader(
            Cursor::new(bytes),
            label,
            ExternalChunkSource::Memory(external_chunks),
        )
    }
}

impl<'a, R: Read + Seek> RegionFile<'a, R> {
    fn from_reader(
        mut reader: R,
        label: &str,
        external: ExternalChunkSource<'a>,
    ) -> Result<Self, SaveError> {
        let stream_len = reader.seek(SeekFrom::End(0))?;
        if stream_len < REGION_HEADER_SIZE as u64 {
            return Err(SaveError::InvalidRegionHeader(format!(
                "Region {label} size ({stream_len} bytes) is smaller than header size ({REGION_HEADER_SIZE} bytes)"
            )));
        }
        reader.seek(SeekFrom::Start(0))?;

        let mut header = [0u8; REGION_HEADER_SIZE];
        reader.read_exact(&mut header).map_err(|e| {
            SaveError::InvalidRegionHeader(format!("Header read failed for {label}: {e}"))
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

        Ok(Self {
            external,
            reader,
            stream_len,
            locations,
        })
    }

    /// Computes the linear chunk index (0..1024) within a region from world chunk coordinates.
    #[inline]
    pub(crate) fn chunk_index(chunk_x: i32, chunk_z: i32) -> usize {
        let rel_x = chunk_x.rem_euclid(REGION_CHUNKS_AXIS as i32) as usize;
        let rel_z = chunk_z.rem_euclid(REGION_CHUNKS_AXIS as i32) as usize;
        rel_x + rel_z * REGION_CHUNKS_AXIS
    }

    /// Checks if a chunk at world chunk coordinates `(chunk_x, chunk_z)` exists in this region.
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

        let stream_len = self.stream_len;
        let sector_end = (loc.sector_offset as u64)
            .checked_add(loc.sector_count as u64)
            .ok_or_else(|| {
                SaveError::InvalidRegionHeader("Sector offset calculation overflow".into())
            })?
            .checked_mul(SECTOR_SIZE as u64)
            .ok_or_else(|| {
                SaveError::InvalidRegionHeader("Sector byte offset calculation overflow".into())
            })?;

        if sector_end > stream_len {
            return Err(SaveError::InvalidRegionHeader(format!(
                "Chunk ({chunk_x}, {chunk_z}) sector allocation extends past EOF ({sector_end} > {stream_len})"
            )));
        }

        // Seek to chunk sector offset
        self.reader.seek(SeekFrom::Start(loc.file_offset()))?;

        // 4 bytes: length of chunk payload (big endian)
        let mut len_buf = [0u8; 4];
        self.reader.read_exact(&mut len_buf).map_err(|e| {
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
        if chunk_payload_end > stream_len {
            return Err(SaveError::InvalidChunkData(format!(
                "Chunk payload extends past stream length ({chunk_payload_end} > {stream_len})"
            )));
        }

        // 1 byte: compression type (high bit 0x80 indicates external `.mcc` chunk stream)
        let mut comp_type = [0u8; 1];
        self.reader.read_exact(&mut comp_type)?;
        let comp = comp_type[0];

        if (comp & EXTERNAL_STREAM_FLAG) != 0 {
            let inner_comp = comp & !EXTERNAL_STREAM_FLAG;
            return self
                .read_external_chunk(chunk_x, chunk_z, inner_comp)
                .map(Some);
        }

        let compressed_size = (chunk_len - 1) as usize;
        let mut compressed_data = vec![0u8; compressed_size];
        self.reader.read_exact(&mut compressed_data)?;

        Self::decompress_payload(comp, &compressed_data).map(Some)
    }

    fn read_external_chunk(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        inner_comp: u8,
    ) -> Result<Vec<u8>, SaveError> {
        let read_cap = if inner_comp == 3 {
            MAX_DECOMPRESSED_CHUNK_SIZE as u64
        } else {
            (MAX_DECOMPRESSED_CHUNK_SIZE as u64) + 1024 * 1024
        };

        match &self.external {
            ExternalChunkSource::Dir(dir) => {
                let ext_path = dir.join(format!("c.{chunk_x}.{chunk_z}.mcc"));
                if !ext_path.is_file() {
                    return Err(SaveError::InvalidChunkData(format!(
                        "External chunk file missing: {}",
                        ext_path.display()
                    )));
                }
                let file = File::open(&ext_path)?;
                let mut raw = Vec::new();
                file.take(read_cap + 1).read_to_end(&mut raw)?;
                if (raw.len() as u64) > read_cap {
                    return Err(SaveError::InvalidChunkData(format!(
                        "External chunk {} exceeds {} byte size limit",
                        ext_path.display(),
                        read_cap
                    )));
                }
                Self::decompress_payload(inner_comp, &raw)
            }
            ExternalChunkSource::Memory(chunks) => {
                let raw = chunks.get(&(chunk_x, chunk_z)).ok_or_else(|| {
                    SaveError::InvalidChunkData(format!(
                        "External chunk c.{chunk_x}.{chunk_z}.mcc missing from memory input"
                    ))
                })?;
                if (raw.len() as u64) > read_cap {
                    return Err(SaveError::InvalidChunkData(format!(
                        "External chunk c.{chunk_x}.{chunk_z}.mcc exceeds {read_cap} byte size limit"
                    )));
                }
                Self::decompress_payload(inner_comp, raw)
            }
        }
    }

    fn decompress_payload(comp_type: u8, data: &[u8]) -> Result<Vec<u8>, SaveError> {
        match comp_type {
            1 => Self::decompress_limited(GzDecoder::new(data), "GZip"),
            2 => Self::decompress_limited(ZlibDecoder::new(data), "Zlib"),
            3 => {
                if data.len() > MAX_DECOMPRESSED_CHUNK_SIZE {
                    return Err(SaveError::InvalidChunkData(format!(
                        "Uncompressed chunk size {} exceeds 64 MiB limit",
                        data.len()
                    )));
                }
                Ok(data.to_vec())
            }
            4 => crate::lz4::decompress_lz4_block_stream(data, MAX_DECOMPRESSED_CHUNK_SIZE),
            other => Err(SaveError::UnsupportedCompression(other)),
        }
    }

    fn decompress_limited<RD: Read>(mut reader: RD, codec: &str) -> Result<Vec<u8>, SaveError> {
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

    fn make_temp_dir(tag: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("mtk_save_{tag}_{}_{}", std::process::id(), unique));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_single_chunk_mca(comp_type: u8, payload: &[u8]) -> Vec<u8> {
        let declared_len = (payload.len() + 1) as u32;
        let sectors_needed = (payload.len() + 5).div_ceil(SECTOR_SIZE) as u8;
        let mut bytes = vec![0u8; SECTOR_SIZE * (2 + sectors_needed as usize)];
        bytes[0..3].copy_from_slice(&[0, 0, 2]);
        bytes[3] = sectors_needed;
        let data_offset = SECTOR_SIZE * 2;
        bytes[data_offset..data_offset + 4].copy_from_slice(&declared_len.to_be_bytes());
        bytes[data_offset + 4] = comp_type;
        bytes[data_offset + 5..data_offset + 5 + payload.len()].copy_from_slice(payload);
        bytes
    }

    fn with_temp_mca<F>(tag: &str, mca_bytes: &[u8], f: F)
    where
        F: FnOnce(&Path, &Path),
    {
        let dir = make_temp_dir(tag);
        let path = dir.join("r.0.0.mca");
        std::fs::write(&path, mca_bytes).unwrap();
        f(&dir, &path);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_region_chunk_indexing() {
        assert_eq!(RegionFile::<File>::chunk_index(0, 0), 0);
        assert_eq!(RegionFile::<File>::chunk_index(1, 0), 1);
        assert_eq!(RegionFile::<File>::chunk_index(0, 1), 32);
        assert_eq!(RegionFile::<File>::chunk_index(31, 31), 1023);

        // Negative coordinates wrap using Euclidean remainder
        assert_eq!(RegionFile::<File>::chunk_index(-1, -1), 31 + 31 * 32);
        assert_eq!(RegionFile::<File>::chunk_index(-32, 0), 0);
    }

    #[test]
    fn test_region_file_reads_gzip_chunk() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = b"gzip-compressed chunk payload".repeat(16);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&payload).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() + 5 < SECTOR_SIZE);

        let mca = build_single_chunk_mca(1, &compressed);
        with_temp_mca("gzip", &mca, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let decoded = region
                .read_chunk_decompressed(0, 0)
                .unwrap()
                .expect("gzip chunk should be present");
            assert_eq!(decoded, payload);
        });
    }

    #[test]
    fn test_region_file_reads_lz4_chunk() {
        let payload = b"lz4-compressed chunk payload".repeat(12);
        let compressed = crate::lz4::test_helpers::encode_raw_lz4_stream(&payload);
        let mca = build_single_chunk_mca(4, &compressed);
        with_temp_mca("lz4", &mca, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let decoded = region
                .read_chunk_decompressed(0, 0)
                .unwrap()
                .expect("lz4 chunk should be present");
            assert_eq!(decoded, payload);
        });
    }

    #[test]
    fn test_region_file_reads_external_mcc_chunk() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload_zlib = b"external zlib mcc payload".repeat(20);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&payload_zlib).unwrap();
        let zlib_bytes = encoder.finish().unwrap();

        let mca_zlib = build_single_chunk_mca(EXTERNAL_STREAM_FLAG | 2, &[]);
        with_temp_mca("mcc_zlib", &mca_zlib, |dir, path| {
            std::fs::write(dir.join("c.0.0.mcc"), &zlib_bytes).unwrap();
            let mut region = RegionFile::open(path).unwrap();
            let decoded = region
                .read_chunk_decompressed(0, 0)
                .unwrap()
                .expect("external zlib chunk should be present");
            assert_eq!(decoded, payload_zlib);
        });

        let payload_lz4 = b"external lz4 mcc payload".repeat(20);
        let lz4_bytes = crate::lz4::test_helpers::encode_raw_lz4_stream(&payload_lz4);
        let mca_lz4 = build_single_chunk_mca(EXTERNAL_STREAM_FLAG | 4, &[]);
        with_temp_mca("mcc_lz4", &mca_lz4, |dir, path| {
            std::fs::write(dir.join("c.0.0.mcc"), &lz4_bytes).unwrap();
            let mut region = RegionFile::open(path).unwrap();
            let decoded = region
                .read_chunk_decompressed(0, 0)
                .unwrap()
                .expect("external lz4 chunk should be present");
            assert_eq!(decoded, payload_lz4);
        });
    }

    #[test]
    fn test_region_file_missing_external_mcc_returns_err() {
        let mca = build_single_chunk_mca(EXTERNAL_STREAM_FLAG | 2, &[]);
        with_temp_mca("mcc_missing", &mca, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let result = region.read_chunk_decompressed(0, 0);
            assert!(
                matches!(result, Err(SaveError::InvalidChunkData(_))),
                "Expected InvalidChunkData for missing .mcc, got: {result:?}"
            );
        });
    }

    #[test]
    fn test_region_location_pointing_past_eof_returns_err() {
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 100]);
        bytes[3] = 1;

        with_temp_mca("past_eof", &bytes, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let result = region.read_chunk_decompressed(0, 0);
            assert!(
                matches!(result, Err(SaveError::InvalidRegionHeader(_))),
                "Expected InvalidRegionHeader, got: {result:?}"
            );
        });
    }

    #[test]
    fn test_region_location_invalid_low_sector_offset_returns_err() {
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 1]);
        bytes[3] = 1;

        with_temp_mca("low_sector", &bytes, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let result = region.read_chunk_decompressed(0, 0);
            assert!(
                matches!(result, Err(SaveError::InvalidRegionHeader(_))),
                "Expected InvalidRegionHeader, got: {result:?}"
            );
        });
    }

    #[test]
    fn test_region_chunk_length_exceeding_sector_capacity_returns_err() {
        let mut bytes = vec![0u8; SECTOR_SIZE * 3];
        bytes[0..3].copy_from_slice(&[0, 0, 2]);
        bytes[3] = 1;
        let data_offset = SECTOR_SIZE * 2;
        bytes[data_offset..data_offset + 4].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());

        with_temp_mca("huge_len", &bytes, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let result = region.read_chunk_decompressed(0, 0);
            assert!(
                matches!(result, Err(SaveError::InvalidChunkData(_))),
                "Expected InvalidChunkData, got: {result:?}"
            );
        });
    }

    #[test]
    fn test_region_decompressed_gzip_exceeding_cap_returns_err() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let bomb_payload = vec![0u8; 65 * 1024 * 1024];
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&bomb_payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let mca = build_single_chunk_mca(1, &compressed);
        with_temp_mca("gz_bomb", &mca, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let result = region.read_chunk_decompressed(0, 0);
            assert!(
                matches!(result, Err(SaveError::DecompressionFailed(_))),
                "Expected DecompressionFailed, got: {result:?}"
            );
        });
    }

    #[test]
    fn test_region_decompressed_zlib_exceeding_cap_returns_err() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let bomb_payload = vec![0u8; 65 * 1024 * 1024];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&bomb_payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let mca = build_single_chunk_mca(2, &compressed);
        with_temp_mca("zlib_bomb", &mca, |_, path| {
            let mut region = RegionFile::open(path).unwrap();
            let result = region.read_chunk_decompressed(0, 0);
            assert!(
                matches!(result, Err(SaveError::DecompressionFailed(_))),
                "Expected DecompressionFailed, got: {result:?}"
            );
        });
    }

    #[test]
    fn test_malformed_location_table_file_too_small_returns_err() {
        with_temp_mca("too_small", &[0u8; 100], |_, path| {
            let result = RegionFile::open(path);
            assert!(
                matches!(result, Err(SaveError::InvalidRegionHeader(_))),
                "Expected InvalidRegionHeader, got: {result:?}"
            );
        });
    }
}
