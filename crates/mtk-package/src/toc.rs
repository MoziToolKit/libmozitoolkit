//! Table of Contents (TOC) definitions and binary encoding.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use crate::error::{PackageError, PackageResult};
use crate::header::CompressionType;

/// Single chunk metadata entry in Table of Contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChunkEntry {
    /// Chunk type tag (e.g. `b"META"`, `b"MODL"`, `b"ATLS"`, `b"BIOM"`, `b"TXTR"`, `b"VOXL"`).
    pub(crate) chunk_type: [u8; 4],
    /// Absolute start offset in package file (must be 64-byte aligned).
    pub(crate) offset: u64,
    /// Compressed size in bytes. If uncompressed, equals `decompressed_size`.
    pub(crate) compressed_size: u64,
    /// Original decompressed size in bytes.
    pub(crate) decompressed_size: u64,
    /// Chunk compression type.
    pub(crate) compression: CompressionType,
    /// Reserved padding bytes.
    pub(crate) reserved: [u8; 7],
    /// Checksum of decompressed payload (e.g. CRC32 cast to u64).
    pub(crate) checksum: u64,
    /// Logical identifier or resource path (e.g. `"models/database"`, `"atlas/mapping"`).
    pub(crate) identifier: String,
}

impl ChunkEntry {
    /// Serializes entry into binary format.
    pub(crate) fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.chunk_type)?;
        writer.write_all(&self.offset.to_le_bytes())?;
        writer.write_all(&self.compressed_size.to_le_bytes())?;
        writer.write_all(&self.decompressed_size.to_le_bytes())?;
        writer.write_all(&[self.compression as u8])?;
        writer.write_all(&self.reserved)?;
        writer.write_all(&self.checksum.to_le_bytes())?;

        let id_bytes = self.identifier.as_bytes();
        let id_len = id_bytes.len() as u16;
        writer.write_all(&id_len.to_le_bytes())?;
        writer.write_all(id_bytes)?;

        Ok(())
    }

    /// Deserializes entry from binary stream.
    pub(crate) fn read_from<R: Read>(reader: &mut R) -> PackageResult<Self> {
        let mut chunk_type = [0u8; 4];
        reader.read_exact(&mut chunk_type)?;

        let mut buf8 = [0u8; 8];

        reader.read_exact(&mut buf8)?;
        let offset = u64::from_le_bytes(buf8);

        reader.read_exact(&mut buf8)?;
        let compressed_size = u64::from_le_bytes(buf8);

        reader.read_exact(&mut buf8)?;
        let decompressed_size = u64::from_le_bytes(buf8);

        let mut comp_byte = [0u8; 1];
        reader.read_exact(&mut comp_byte)?;
        let compression = CompressionType::from_u8(comp_byte[0]).ok_or_else(|| {
            PackageError::InvalidToc(format!("Invalid compression type: {}", comp_byte[0]))
        })?;

        let mut reserved = [0u8; 7];
        reader.read_exact(&mut reserved)?;

        reader.read_exact(&mut buf8)?;
        let checksum = u64::from_le_bytes(buf8);

        let mut buf2 = [0u8; 2];
        reader.read_exact(&mut buf2)?;
        let id_len = u16::from_le_bytes(buf2) as usize;

        let mut id_vec = vec![0u8; id_len];
        reader.read_exact(&mut id_vec)?;
        let identifier = String::from_utf8(id_vec)
            .map_err(|e| PackageError::InvalidToc(format!("Invalid UTF-8 identifier: {}", e)))?;

        Ok(Self {
            chunk_type,
            offset,
            compressed_size,
            decompressed_size,
            compression,
            reserved,
            checksum,
            identifier,
        })
    }
}

/// Table of Contents containing all chunk entries with fast name indexing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TableOfContents {
    pub(crate) entries: Vec<ChunkEntry>,
    index_by_id: HashMap<String, usize>,
}

impl TableOfContents {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            index_by_id: HashMap::new(),
        }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            index_by_id: HashMap::with_capacity(capacity),
        }
    }

    /// Adds a chunk entry and indexes it by identifier.
    pub(crate) fn add_entry(&mut self, entry: ChunkEntry) {
        let idx = self.entries.len();
        self.index_by_id.insert(entry.identifier.clone(), idx);
        self.entries.push(entry);
    }

    /// Finds a chunk entry by its logical identifier.
    pub(crate) fn get_by_identifier(&self, identifier: &str) -> Option<&ChunkEntry> {
        self.index_by_id
            .get(identifier)
            .map(|&idx| &self.entries[idx])
    }

    /// Iterates over all chunks matching a specific 4-byte chunk type tag.
    pub(crate) fn iter_by_type<'a>(
        &'a self,
        chunk_type: [u8; 4],
    ) -> impl Iterator<Item = &'a ChunkEntry> + 'a {
        self.entries
            .iter()
            .filter(move |e| e.chunk_type == chunk_type)
    }

    /// Serializes TOC to raw uncompressed bytes.
    pub(crate) fn encode_bytes(&self) -> PackageResult<Vec<u8>> {
        let mut buf = Vec::new();
        let count = self.entries.len() as u32;
        buf.write_all(&count.to_le_bytes())?;
        for entry in &self.entries {
            entry.write_to(&mut buf)?;
        }
        Ok(buf)
    }

    /// Deserializes TOC from raw uncompressed bytes.
    pub(crate) fn decode_bytes(slice: &[u8]) -> PackageResult<Self> {
        let mut cursor = Cursor::new(slice);
        let mut buf4 = [0u8; 4];
        cursor
            .read_exact(&mut buf4)
            .map_err(|e| PackageError::InvalidToc(format!("Failed to read chunk count: {}", e)))?;
        let count = u32::from_le_bytes(buf4) as usize;

        // Each entry is at least MIN_ENTRY_SIZE bytes. Reject forged counts up front so a
        // corrupt/malicious header cannot force a multi-gigabyte pre-allocation.
        const MIN_ENTRY_SIZE: usize = 4 + 8 + 8 + 8 + 1 + 7 + 8 + 2;
        let max_possible = slice.len().saturating_sub(4) / MIN_ENTRY_SIZE;
        if count > max_possible {
            return Err(PackageError::InvalidToc(format!(
                "Declared chunk count {} exceeds payload capacity {}",
                count, max_possible
            )));
        }

        let mut toc = Self::with_capacity(count);
        for _ in 0..count {
            let entry = ChunkEntry::read_from(&mut cursor)?;
            toc.add_entry(entry);
        }
        Ok(toc)
    }

    /// Encodes TOC with Zstd compression.
    pub(crate) fn encode_zstd(&self, level: i32) -> PackageResult<Vec<u8>> {
        let raw = self.encode_bytes()?;
        zstd::encode_all(Cursor::new(raw), level).map_err(|e| PackageError::CompressionFailed {
            identifier: "TOC".to_string(),
            source: e,
        })
    }

    /// Decodes TOC from Zstd compressed bytes.
    pub(crate) fn decode_zstd(compressed_slice: &[u8]) -> PackageResult<Self> {
        let decompressed = zstd::decode_all(Cursor::new(compressed_slice)).map_err(|e| {
            PackageError::DecompressionFailed {
                identifier: "TOC".to_string(),
                source: e,
            }
        })?;
        Self::decode_bytes(&decompressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toc_forged_count_rejected() {
        // A forged chunk count must not trigger a huge pre-allocation; it is rejected
        // because the payload cannot possibly hold that many entries.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 8]);

        match TableOfContents::decode_bytes(&bytes) {
            Err(PackageError::InvalidToc(_)) => {}
            other => panic!("Expected InvalidToc, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn test_toc_roundtrip_uncompressed() {
        let mut toc = TableOfContents::new();
        toc.add_entry(ChunkEntry {
            chunk_type: *b"MODL",
            offset: 64,
            compressed_size: 1024,
            decompressed_size: 4096,
            compression: CompressionType::Zstd,
            reserved: [0; 7],
            checksum: 12345678,
            identifier: "models/database".to_string(),
        });
        toc.add_entry(ChunkEntry {
            chunk_type: *b"ATLS",
            offset: 1088,
            compressed_size: 2048,
            decompressed_size: 2048,
            compression: CompressionType::None,
            reserved: [0; 7],
            checksum: 87654321,
            identifier: "atlas/chunk_001.png".to_string(),
        });

        let encoded = toc.encode_bytes().expect("Failed to encode TOC");
        let decoded = TableOfContents::decode_bytes(&encoded).expect("Failed to decode TOC");

        assert_eq!(decoded.entries.len(), 2);
        assert_eq!(
            decoded.get_by_identifier("models/database").unwrap().offset,
            64
        );
        assert_eq!(
            decoded
                .get_by_identifier("atlas/chunk_001.png")
                .unwrap()
                .checksum,
            87654321
        );
    }

    #[test]
    fn test_toc_roundtrip_zstd() {
        let mut toc = TableOfContents::new();
        toc.add_entry(ChunkEntry {
            chunk_type: *b"META",
            offset: 64,
            compressed_size: 256,
            decompressed_size: 512,
            compression: CompressionType::Zstd,
            reserved: [0; 7],
            checksum: 9999,
            identifier: "manifest".to_string(),
        });

        let compressed = toc.encode_zstd(3).expect("Failed to compress TOC");
        let decoded = TableOfContents::decode_zstd(&compressed).expect("Failed to decompress TOC");

        assert_eq!(decoded.entries.len(), 1);
        assert_eq!(decoded.get_by_identifier("manifest").unwrap().offset, 64);
    }
}
