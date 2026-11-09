//! Streaming package builder and writer.

use std::fs::{self, File};
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::error::{PackageError, PackageResult};
use crate::header::{CompressionType, PackageHeader, PackageProfile, HEADER_SIZE};
use crate::toc::{ChunkEntry, TableOfContents};

/// Chunk compression and serialization options.
#[derive(Debug, Clone)]
pub struct ChunkWriteOptions {
    pub(crate) compression: CompressionType,
    pub zstd_level: i32,
}

impl Default for ChunkWriteOptions {
    fn default() -> Self {
        Self {
            compression: CompressionType::Zstd,
            zstd_level: 3,
        }
    }
}

impl ChunkWriteOptions {
    /// No compression (recommended for PNG/JPEG images).
    pub fn raw() -> Self {
        Self {
            compression: CompressionType::None,
            zstd_level: 0,
        }
    }

    /// Fast Zstd compression (level 3, default).
    pub fn zstd_fast() -> Self {
        Self {
            compression: CompressionType::Zstd,
            zstd_level: 3,
        }
    }
}

/// Streaming writer for MTK binary package files.
pub struct MtkPackageWriter<W: Write + Seek> {
    writer: W,
    header: PackageHeader,
    toc: TableOfContents,
    current_offset: u64,
}

impl<W: Write + Seek> MtkPackageWriter<W> {
    /// Creates a new package writer from a seekable stream.
    pub fn new(
        mut writer: W,
        profile: PackageProfile,
        fingerprint: [u8; 16],
    ) -> PackageResult<Self> {
        let epoch_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let header = match profile {
            PackageProfile::AssetCache => PackageHeader::new_asset_cache(fingerprint, epoch_now),
            PackageProfile::SceneInterchange => {
                PackageHeader::new_scene_interchange(fingerprint, epoch_now)
            }
        };

        // Write placeholder for 64-byte header
        let placeholder = [0u8; HEADER_SIZE];
        writer.write_all(&placeholder)?;

        Ok(Self {
            writer,
            header,
            toc: TableOfContents::new(),
            current_offset: HEADER_SIZE as u64,
        })
    }

    /// Adds a chunk to the package. All chunks are automatically aligned to 64-byte boundaries.
    pub fn add_chunk(
        &mut self,
        chunk_type: [u8; 4],
        identifier: impl Into<String>,
        decompressed_data: &[u8],
        options: &ChunkWriteOptions,
    ) -> PackageResult<()> {
        let id = identifier.into();

        // 1. Ensure 64-byte alignment
        let remainder = self.current_offset % 64;
        if remainder != 0 {
            let padding_needed = (64 - remainder) as usize;
            let padding = vec![0u8; padding_needed];
            self.writer.write_all(&padding)?;
            self.current_offset += padding_needed as u64;
        }

        let chunk_start_offset = self.current_offset;
        let decompressed_size = decompressed_data.len() as u64;

        // 2. Compute CRC32 checksum on decompressed payload
        let checksum = crc32fast::hash(decompressed_data) as u64;

        // 3. Compress data according to options
        let (compressed_bytes, actual_compression) = match options.compression {
            CompressionType::None => (decompressed_data.to_vec(), CompressionType::None),
            CompressionType::Zstd => {
                let compressed =
                    zstd::encode_all(Cursor::new(decompressed_data), options.zstd_level).map_err(
                        |e| PackageError::CompressionFailed {
                            identifier: id.clone(),
                            source: e,
                        },
                    )?;
                // If compressed size is larger than uncompressed, fallback to raw
                if compressed.len() >= decompressed_data.len() {
                    (decompressed_data.to_vec(), CompressionType::None)
                } else {
                    (compressed, CompressionType::Zstd)
                }
            }
        };

        let compressed_size = compressed_bytes.len() as u64;

        // 4. Write payload
        self.writer.write_all(&compressed_bytes)?;
        self.current_offset += compressed_size;

        // 5. Record TOC entry
        self.toc.add_entry(ChunkEntry {
            chunk_type,
            offset: chunk_start_offset,
            compressed_size,
            decompressed_size,
            compression: actual_compression,
            reserved: [0; 7],
            checksum,
            identifier: id,
        });

        Ok(())
    }

    /// Appends string data as a chunk (encoded as UTF-8).
    pub fn add_str_chunk(
        &mut self,
        chunk_type: [u8; 4],
        identifier: impl Into<String>,
        str_data: &str,
        options: &ChunkWriteOptions,
    ) -> PackageResult<()> {
        self.add_chunk(chunk_type, identifier, str_data.as_bytes(), options)
    }

    /// Appends JSON-serializable data as a chunk.
    pub fn add_json_chunk<T: serde::Serialize>(
        &mut self,
        chunk_type: [u8; 4],
        identifier: impl Into<String>,
        value: &T,
        options: &ChunkWriteOptions,
    ) -> PackageResult<()> {
        let json_bytes =
            serde_json::to_vec(value).map_err(|e| PackageError::Serialization(e.to_string()))?;
        self.add_chunk(chunk_type, identifier, &json_bytes, options)
    }

    /// Finalizes the package: writes TOC, updates and writes final 64-byte header.
    pub fn finish(mut self) -> PackageResult<PackageHeader> {
        // 1. Align TOC to 64-byte boundary
        let remainder = self.current_offset % 64;
        if remainder != 0 {
            let padding_needed = (64 - remainder) as usize;
            let padding = vec![0u8; padding_needed];
            self.writer.write_all(&padding)?;
            self.current_offset += padding_needed as u64;
        }

        let toc_offset = self.current_offset;

        // 2. Encode TOC (Zstd compressed if flag is set)
        let toc_bytes = if self.header.flags.is_toc_zstd() {
            self.toc.encode_zstd(3)?
        } else {
            self.toc.encode_bytes()?
        };

        let toc_length = toc_bytes.len() as u64;
        self.writer.write_all(&toc_bytes)?;
        self.current_offset += toc_length;

        // 3. Update header fields
        self.header.toc_offset = toc_offset;
        self.header.toc_length = toc_length;
        self.header.chunk_count = self.toc.entries.len() as u32;

        // 4. Seek to 0 and write finalized 64-byte header
        self.writer.seek(SeekFrom::Start(0))?;
        let header_bytes = self.header.to_bytes();
        self.writer.write_all(&header_bytes)?;
        self.writer.flush()?;

        Ok(self.header)
    }
}

/// A file-backed package writer that stages the package in a temporary sibling
/// file and publishes it atomically when [`finish`](Self::finish) succeeds.
///
/// This is the portable way to rebuild a package that may still be open for
/// reading:
///
/// - POSIX: readers that already mapped the old inode keep a valid snapshot and
///   `rename` atomically swaps the directory entry.
/// - Windows: the destination is never truncated while a mapped view exists
///   (`ERROR_USER_MAPPED_FILE`); the replacement is fully written before the
///   old file is touched. Callers must still drop *their own* mapped views
///   before calling `finish`, because Windows cannot replace an open file.
///
/// A writer that is dropped without reaching `finish` removes its temporary
/// file, leaving any previous package untouched.
pub struct MtkPackageFileWriter {
    inner: Option<MtkPackageWriter<File>>,
    sync_file: Option<File>,
    temp_path: PathBuf,
    final_path: PathBuf,
    committed: bool,
}

/// Process-unique suffix so concurrent writers never collide on temp names.
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl MtkPackageFileWriter {
    /// Creates a writer that publishes to `path` when [`finish`](Self::finish) succeeds.
    pub fn create(
        path: impl AsRef<Path>,
        profile: PackageProfile,
        fingerprint: [u8; 16],
    ) -> PackageResult<Self> {
        let final_path = path.as_ref().to_path_buf();
        if let Some(parent) = final_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let temp_path = temporary_sibling(&final_path);
        // Only our own stale temp from a crashed run can collide (same pid+seq).
        let _ = fs::remove_file(&temp_path);

        let file = File::create(&temp_path)?;
        let sync_file = file.try_clone().ok();
        let inner = match MtkPackageWriter::new(file, profile, fingerprint) {
            Ok(writer) => writer,
            Err(err) => {
                let _ = fs::remove_file(&temp_path);
                return Err(err);
            }
        };

        Ok(Self {
            inner: Some(inner),
            sync_file,
            temp_path,
            final_path,
            committed: false,
        })
    }

    fn inner_mut(&mut self) -> PackageResult<&mut MtkPackageWriter<File>> {
        self.inner.as_mut().ok_or(PackageError::WriterClosed)
    }

    /// Adds a chunk to the package. See [`MtkPackageWriter::add_chunk`].
    pub fn add_chunk(
        &mut self,
        chunk_type: [u8; 4],
        identifier: impl Into<String>,
        decompressed_data: &[u8],
        options: &ChunkWriteOptions,
    ) -> PackageResult<()> {
        self.inner_mut()?
            .add_chunk(chunk_type, identifier, decompressed_data, options)
    }

    /// Appends string data as a chunk. See [`MtkPackageWriter::add_str_chunk`].
    pub fn add_str_chunk(
        &mut self,
        chunk_type: [u8; 4],
        identifier: impl Into<String>,
        str_data: &str,
        options: &ChunkWriteOptions,
    ) -> PackageResult<()> {
        self.inner_mut()?
            .add_str_chunk(chunk_type, identifier, str_data, options)
    }

    /// Appends JSON-serializable data as a chunk. See [`MtkPackageWriter::add_json_chunk`].
    pub fn add_json_chunk<T: serde::Serialize>(
        &mut self,
        chunk_type: [u8; 4],
        identifier: impl Into<String>,
        value: &T,
        options: &ChunkWriteOptions,
    ) -> PackageResult<()> {
        self.inner_mut()?
            .add_json_chunk(chunk_type, identifier, value, options)
    }

    /// Finalizes the package, flushes it to disk and atomically publishes it.
    pub fn finish(mut self) -> PackageResult<PackageHeader> {
        let inner = self.inner.take().ok_or(PackageError::WriterClosed)?;
        let header = inner.finish()?;

        // Flush the OS write-back cache before the rename so a crash cannot
        // publish a file whose contents were never persisted.
        if let Some(sync_file) = self.sync_file.take() {
            sync_file.sync_all()?;
        }

        replace_file(&self.temp_path, &self.final_path)?;
        self.committed = true;
        Ok(header)
    }
}

impl Drop for MtkPackageFileWriter {
    fn drop(&mut self) {
        if !self.committed {
            // Best effort: never leave a partially written package behind.
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

fn temporary_sibling(final_path: &Path) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package.mtkcache");
    let temp_name = format!(".{}.{}.{}.tmp", file_name, std::process::id(), sequence);
    match final_path.parent() {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

fn replace_file(temp_path: &Path, final_path: &Path) -> PackageResult<()> {
    // POSIX `rename` and Windows `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` both
    // replace an existing destination atomically when no conflicting handle is
    // open. On Windows an open mapped view makes this fail; the caller is
    // responsible for releasing its own reader first.
    //
    // A short retry also covers transient Windows sharing violations from
    // antivirus/indexers that briefly open freshly written files.
    let mut last_error = None;
    for attempt in 0..3u64 {
        match fs::rename(temp_path, final_path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(20 * (attempt + 1)));
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| std::io::Error::other("rename failed without error"))
        .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_and_toc_alignment() {
        let mut cursor = Cursor::new(Vec::new());
        let mut writer =
            MtkPackageWriter::new(&mut cursor, PackageProfile::AssetCache, [0xAA; 16]).unwrap();

        writer
            .add_chunk(
                *b"TEST",
                "test/1",
                b"hello world",
                &ChunkWriteOptions::raw(),
            )
            .unwrap();
        writer
            .add_chunk(
                *b"TEST",
                "test/2",
                b"another chunk with odd length: 12345",
                &ChunkWriteOptions::zstd_fast(),
            )
            .unwrap();

        for entry in &writer.toc.entries {
            assert_eq!(
                entry.offset % 64,
                0,
                "Entry {} offset {} not 64-byte aligned",
                entry.identifier,
                entry.offset
            );
        }

        let header = writer.finish().unwrap();
        assert_eq!(header.toc_offset % 64, 0);
    }

    #[test]
    fn test_replace_file_non_existent_does_not_panic() {
        let temp = Path::new("/non/existent/path/temp.tmp");
        let dest = Path::new("/non/existent/path/dest.bin");
        let res = replace_file(temp, dest);
        assert!(res.is_err());
    }
}
