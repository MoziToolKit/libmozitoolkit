use crate::error::ResourceError;
use std::collections::HashMap;
use std::fs;
#[cfg(feature = "zip")]
use std::io::Read;
use std::path::{Path, PathBuf};

fn is_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn normalize_relative_path(path: &str) -> Option<PathBuf> {
    if is_windows_drive_prefix(path) || path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    let clean = path.replace('\\', "/");
    let path_obj = Path::new(&clean);
    let mut normalized = PathBuf::new();
    let mut has_normal = false;
    for comp in path_obj.components() {
        match comp {
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::ParentDir => return None,
            std::path::Component::Normal(c) => {
                let s = c.to_string_lossy();
                if is_windows_drive_prefix(&s) {
                    return None;
                }
                has_normal = true;
                normalized.push(c);
            }
            std::path::Component::CurDir => {}
        }
    }
    if has_normal {
        Some(normalized)
    } else {
        None
    }
}

/// Trait defining a readable Minecraft resource pack source.
pub trait ResourcePack: Send + Sync {
    /// Friendly name or identifier of this pack.
    fn name(&self) -> &str;

    /// Open and read the raw bytes of a file inside the pack given its relative path
    /// (e.g. `"assets/minecraft/textures/block/stone.png"`).
    ///
    /// A path that escapes the pack root is treated as absent.
    fn open(&self, relative_path: &str) -> Option<Vec<u8>>;

    /// List all relative paths starting with the given prefix.
    fn list_files(&self, prefix: &str) -> Vec<String>;

    /// Check if a relative path exists in this pack.
    fn has_file(&self, relative_path: &str) -> bool {
        self.open(relative_path).is_some()
    }

    /// Cheap, deterministic identity of the pack's *content*.
    ///
    /// The stack fingerprint feeds cache invalidation, so it must change when a pack is edited in
    /// place. Renaming or reordering packs is not enough: a user who repaints a texture and keeps
    /// the same file name must get a rebuilt cache. Implementations therefore report per-entry
    /// `(path, size, modified_secs)`, never the file bytes.
    fn content_entries(&self) -> Vec<PackContentEntry> {
        self.list_files("")
            .into_iter()
            .map(|path| PackContentEntry {
                path,
                size: 0,
                modified_secs: 0,
            })
            .collect()
    }

    /// Absolute location of this pack on disk, when it has one.
    ///
    /// Part of the fingerprint so two different packs that happen to share a display name do not
    /// share a cache.
    fn source_label(&self) -> String {
        self.name().to_string()
    }
}

/// Modification time of a file in whole seconds since the Unix epoch (0 when unavailable).
pub(crate) fn modified_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One file inside a pack, described without reading its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackContentEntry {
    /// Path relative to the pack root, using `/` separators.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Modification time in seconds since the Unix epoch (0 when unknown).
    pub modified_secs: i64,
}

/// A resource pack backed by an unzipped directory on the local filesystem.
#[derive(Debug)]
pub struct DirectoryPack {
    name: String,
    root_dir: PathBuf,
}

impl DirectoryPack {
    pub fn new(name: impl Into<String>, root_dir: impl AsRef<Path>) -> Self {
        Self {
            name: name.into(),
            root_dir: root_dir.as_ref().to_path_buf(),
        }
    }

    fn normalize_path(path: &str) -> Option<PathBuf> {
        normalize_relative_path(path)
    }
}

/// Walks `root` and reports one [`PackContentEntry`] per regular file.
///
/// Single source of truth for directory-pack content: `DirectoryPack::content_entries` and
/// `ResourcePackStack::compute_fingerprint_from_paths` must produce identical entries, otherwise
/// the cache fingerprint computed from paths can disagree with the one computed from the loaded
/// stack and serve a package built from different files.
pub(crate) fn content_entries_from_dir(root: &Path) -> Vec<PackContentEntry> {
    fn walk(root: &Path, current: &Path, out: &mut Vec<PackContentEntry>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(root, &path, out);
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            out.push(PackContentEntry {
                path: rel.to_string_lossy().replace('\\', "/"),
                size: meta.len(),
                modified_secs: modified_secs(&meta),
            });
        }
    }

    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

impl ResourcePack for DirectoryPack {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_label(&self) -> String {
        // Canonicalize so the host's path-only fingerprint (`Path.resolve()` on its side) agrees
        // with the loaded stack; on macOS `/var` and `/private/var` are the same pack.
        std::fs::canonicalize(&self.root_dir)
            .unwrap_or_else(|_| self.root_dir.clone())
            .display()
            .to_string()
    }

    fn content_entries(&self) -> Vec<PackContentEntry> {
        content_entries_from_dir(&self.root_dir)
    }

    fn open(&self, relative_path: &str) -> Option<Vec<u8>> {
        let norm = Self::normalize_path(relative_path)?;
        fs::read(self.root_dir.join(norm)).ok()
    }

    fn list_files(&self, prefix: &str) -> Vec<String> {
        let norm_prefix = prefix.replace('\\', "/");
        let mut results = Vec::new();
        let target_dir = if let Some(p) = Self::normalize_path(&norm_prefix) {
            let candidate = self.root_dir.join(p);
            if candidate.is_dir() {
                candidate
            } else {
                self.root_dir.clone()
            }
        } else {
            self.root_dir.clone()
        };

        fn walk_dir(root: &Path, current: &Path, prefix_filter: &str, results: &mut Vec<String>) {
            if let Ok(entries) = fs::read_dir(current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk_dir(root, &path, prefix_filter, results);
                    } else if path.is_file() {
                        if let Ok(rel) = path.strip_prefix(root) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            if rel_str.starts_with(prefix_filter) || prefix_filter.is_empty() {
                                results.push(rel_str);
                            }
                        }
                    }
                }
            }
        }

        walk_dir(&self.root_dir, &target_dir, &norm_prefix, &mut results);
        results
    }
}

/// An in-memory resource pack, ideal for unit tests and synthetic assets.
#[derive(Default, Clone)]
pub struct MemoryPack {
    name: String,
    files: HashMap<String, Vec<u8>>,
}

impl MemoryPack {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            files: HashMap::new(),
        }
    }

    pub fn insert(&mut self, path: impl Into<String>, data: impl Into<Vec<u8>>) {
        let clean = path.into().replace('\\', "/");
        self.files.insert(clean, data.into());
    }
}

impl ResourcePack for MemoryPack {
    fn name(&self) -> &str {
        &self.name
    }

    fn content_entries(&self) -> Vec<PackContentEntry> {
        let mut out: Vec<PackContentEntry> = self
            .files
            .iter()
            .map(|(path, bytes)| PackContentEntry {
                path: path.clone(),
                size: bytes.len() as u64,
                modified_secs: 0,
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    fn open(&self, relative_path: &str) -> Option<Vec<u8>> {
        let clean = relative_path.replace('\\', "/");
        self.files.get(&clean).cloned()
    }

    fn list_files(&self, prefix: &str) -> Vec<String> {
        let norm_prefix = prefix.replace('\\', "/");
        self.files
            .keys()
            .filter(|k| k.starts_with(&norm_prefix))
            .cloned()
            .collect()
    }
}

/// A resource pack backed by a `.zip` or `.jar` archive file.
#[cfg(feature = "zip")]
#[derive(Debug)]
pub struct ZipPack {
    name: String,
    file_map: HashMap<String, Vec<u8>>,
    /// Size and mtime of the source archive, when it was loaded from disk.
    archive_meta: Option<(u64, i64)>,
    /// Absolute archive path, when it was loaded from disk.
    source_label: Option<String>,
}

#[cfg(feature = "zip")]
impl ZipPack {
    /// Maximum allowed uncompressed size for a single entry inside an archive (256 MiB).
    pub const MAX_ENTRY_SIZE: u64 = 256 * 1024 * 1024;

    /// Load and cache all files from a ZIP or JAR archive into memory.
    pub fn from_file(
        name: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<Self, ResourceError> {
        let path = path.as_ref();
        let archive_meta = fs::metadata(path)
            .ok()
            .map(|meta| (meta.len(), modified_secs(&meta)));
        let file = fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let file_map = Self::load_archive(&mut archive)?;
        let label = std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string();

        Ok(Self {
            name: name.into(),
            file_map,
            archive_meta,
            source_label: Some(label),
        })
    }

    /// Load and cache from a raw byte slice.
    pub fn from_bytes(name: impl Into<String>, bytes: &[u8]) -> Result<Self, ResourceError> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        let file_map = Self::load_archive(&mut archive)?;

        Ok(Self {
            name: name.into(),
            file_map,
            archive_meta: None,
            source_label: None,
        })
    }

    fn load_archive<R: Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
    ) -> Result<HashMap<String, Vec<u8>>, ResourceError> {
        let mut file_map = HashMap::new();

        for i in 0..archive.len() {
            let mut file_entry = archive.by_index(i)?;
            if file_entry.is_file() {
                let raw_name = file_entry.name()?.replace('\\', "/");
                if normalize_relative_path(&raw_name).is_none() {
                    continue;
                }

                let declared_size = file_entry.size();
                if declared_size > Self::MAX_ENTRY_SIZE {
                    return Err(ResourceError::ZipEntryTooLarge {
                        name: raw_name,
                        size: declared_size,
                        max: Self::MAX_ENTRY_SIZE,
                    });
                }

                let mut buf = Vec::with_capacity(declared_size as usize);
                let mut take = (&mut file_entry).take(Self::MAX_ENTRY_SIZE + 1);
                take.read_to_end(&mut buf)?;

                if buf.len() as u64 > Self::MAX_ENTRY_SIZE {
                    return Err(ResourceError::ZipEntryTooLarge {
                        name: raw_name,
                        size: buf.len() as u64,
                        max: Self::MAX_ENTRY_SIZE,
                    });
                }

                file_map.insert(raw_name, buf);
            }
        }

        Ok(file_map)
    }
}

#[cfg(feature = "zip")]
impl ResourcePack for ZipPack {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_label(&self) -> String {
        self.source_label
            .clone()
            .unwrap_or_else(|| self.name.clone())
    }

    fn content_entries(&self) -> Vec<PackContentEntry> {
        // A file-backed archive is identified by the archive's own size and mtime: one stat
        // invalidates the cache for any repack, and the extracted entry list stays irrelevant.
        if let Some((size, modified_secs)) = self.archive_meta {
            return vec![PackContentEntry {
                path: String::new(),
                size,
                modified_secs,
            }];
        }
        // In-memory archives have no stat, so fall back to the entry table.
        let mut out: Vec<PackContentEntry> = self
            .file_map
            .iter()
            .map(|(path, bytes)| PackContentEntry {
                path: path.clone(),
                size: bytes.len() as u64,
                modified_secs: 0,
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    fn open(&self, relative_path: &str) -> Option<Vec<u8>> {
        normalize_relative_path(relative_path)?;
        let clean = relative_path.replace('\\', "/");
        self.file_map.get(&clean).cloned()
    }

    fn list_files(&self, prefix: &str) -> Vec<String> {
        let norm_prefix = prefix.replace('\\', "/");
        self.file_map
            .keys()
            .filter(|k| k.starts_with(&norm_prefix))
            .cloned()
            .collect()
    }
}
