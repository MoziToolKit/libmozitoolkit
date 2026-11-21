use super::mcmeta::{PackMetaFile, CURRENT_CLIENT_RESOURCE_FORMAT};
use crate::error::ResourceError;
use crate::identifier_pattern::ResourceFilterSection;
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
    /// `Ok(None)` means the pack genuinely does not hold that path (including paths that escape the
    /// pack root, which are treated as absent). `Err` means the path is in this pack but could not
    /// be read: the stack must report that instead of falling back to a lower-priority pack.
    fn open(&self, relative_path: &str) -> Result<Option<Vec<u8>>, ResourceError>;

    /// List all relative paths starting with the given prefix.
    fn list_files(&self, prefix: &str) -> Vec<String>;

    /// Locations this pack hides, from itself and from every lower-priority pack
    /// (`pack.mcmeta`'s `filter.block`).
    ///
    /// The stack stops looking when it reaches the filtering pack, exactly like
    /// `FallbackResourceManager.getResource`.
    fn filter(&self) -> Option<&ResourceFilterSection> {
        None
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
    /// Applicable overlay directories from `pack.mcmeta`, in declaration order (later = higher
    /// priority, as in `CompositePackResources`).
    overlays: Vec<PathBuf>,
    /// `pack.mcmeta`'s `filter.block`.
    filter: Option<ResourceFilterSection>,
}

impl DirectoryPack {
    pub fn new(name: impl Into<String>, root_dir: impl AsRef<Path>) -> Self {
        let root_dir = root_dir.as_ref().to_path_buf();
        let overlays = Self::load_overlays(&root_dir);
        let filter = load_directory_meta(&root_dir).and_then(|meta| meta.filter);
        Self {
            name: name.into(),
            root_dir,
            overlays,
            filter,
        }
    }

    /// Reads `pack.mcmeta` and resolves the overlays that apply to the current client format.
    ///
    /// A missing or malformed `pack.mcmeta` is ignored: it is optional metadata, and Minecraft
    /// treats it the same way (`Pack.readPackMetadata` logs `Failed to read ... metadata`).
    fn load_overlays(root_dir: &Path) -> Vec<PathBuf> {
        load_directory_overlays(root_dir)
    }

    /// Directories to consult, highest priority first: overlays (last declared first), then root.
    fn layers(&self) -> impl Iterator<Item = &Path> {
        self.overlays
            .iter()
            .rev()
            .map(PathBuf::as_path)
            .chain(std::iter::once(self.root_dir.as_path()))
    }

    /// Top-level directory names that belong to an overlay, so the root layer does not report them.
    fn overlay_prefixes(&self) -> Vec<String> {
        self.overlays
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| format!("{}/", name.to_string_lossy()))
            .collect()
    }

    fn normalize_path(path: &str) -> Option<PathBuf> {
        normalize_relative_path(path)
    }
}

/// Lists one layer's files under `prefix`, adding unseen relative paths to `results`.
fn list_layer_files(
    root: &Path,
    prefix: &str,
    skip_top: &[String],
    seen: &mut std::collections::HashSet<String>,
    results: &mut Vec<String>,
) {
    let target_dir = if let Some(p) = normalize_relative_path(prefix) {
        let candidate = root.join(p);
        if candidate.is_dir() {
            candidate
        } else {
            root.to_path_buf()
        }
    } else {
        root.to_path_buf()
    };

    fn walk(
        root: &Path,
        current: &Path,
        prefix: &str,
        skip_top: &[String],
        seen: &mut std::collections::HashSet<String>,
        results: &mut Vec<String>,
    ) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, prefix, skip_top, seen, results);
            } else if path.is_file() {
                let Ok(rel) = path.strip_prefix(root) else {
                    continue;
                };
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if !prefix.is_empty() && !rel_str.starts_with(prefix) {
                    continue;
                }
                if skip_top.iter().any(|skip| rel_str.starts_with(skip)) {
                    continue;
                }
                if seen.insert(rel_str.clone()) {
                    results.push(rel_str);
                }
            }
        }
    }

    walk(root, &target_dir, prefix, skip_top, seen, results);
}

/// Content entries of a directory pack, overlays included.
///
/// Shared by [`DirectoryPack::content_entries`] and the path-only fingerprint fast path: the two
/// must agree, or the host would resolve a cache key for a pack whose overlay directories it did
/// not read the same way.
pub(crate) fn directory_content_entries(root: &Path) -> Vec<PackContentEntry> {
    let overlays = load_directory_overlays(root);
    let overlay_prefixes: Vec<String> = overlays
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| format!("{}/", name.to_string_lossy()))
        .collect();

    // The root walk would otherwise report the overlay directories as ordinary subtrees.
    let mut out: Vec<PackContentEntry> = content_entries_from_dir(root)
        .into_iter()
        .filter(|entry| {
            !overlay_prefixes
                .iter()
                .any(|prefix| entry.path.starts_with(prefix))
        })
        .collect();
    for overlay in &overlays {
        out.extend(content_entries_from_dir(overlay));
    }
    out
}

/// Reads and parses a directory pack's optional `pack.mcmeta`.
///
/// Missing or malformed metadata is ignored: it is optional, and Minecraft only logs it.
fn load_directory_meta(root: &Path) -> Option<PackMetaFile> {
    let bytes = fs::read(root.join("pack.mcmeta")).ok()?;
    PackMetaFile::parse_json(&bytes)
}

/// Resolved overlay directories of a directory pack, in declaration order.
fn load_directory_overlays(root: &Path) -> Vec<PathBuf> {
    load_directory_meta(root)
        .map(|meta| meta.applicable_overlays(CURRENT_CLIENT_RESOURCE_FORMAT))
        .unwrap_or_default()
        .into_iter()
        .map(|directory| root.join(directory))
        .collect()
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

    fn filter(&self) -> Option<&ResourceFilterSection> {
        self.filter.as_ref()
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
        directory_content_entries(&self.root_dir)
    }

    fn open(&self, relative_path: &str) -> Result<Option<Vec<u8>>, ResourceError> {
        let Some(norm) = Self::normalize_path(relative_path) else {
            return Ok(None);
        };
        // Overlays first (last declared wins), then the pack root.
        for layer in self.layers() {
            let candidate = layer.join(&norm);
            match fs::read(&candidate) {
                Ok(bytes) => return Ok(Some(bytes)),
                // Not a readable file in this layer: keep looking.
                // `IsADirectory` (Unix) and `PermissionDenied` on a directory (Windows ERROR_ACCESS_DENIED)
                // cover `assets/.../stone.png` being a directory inside the pack; unreadable files still fail.
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::NotADirectory
                            | std::io::ErrorKind::IsADirectory
                    ) || (err.kind() == std::io::ErrorKind::PermissionDenied
                        && candidate.is_dir()) => {}
                Err(err) => {
                    return Err(ResourceError::ReadFailed {
                        pack: self.name.clone(),
                        path: relative_path.to_string(),
                        source: err,
                    })
                }
            }
        }
        Ok(None)
    }

    fn list_files(&self, prefix: &str) -> Vec<String> {
        let norm_prefix = prefix.replace('\\', "/");
        let skip_top = self.overlay_prefixes();
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Highest priority first so a shadowed path is reported once, from the layer that wins.
        for (index, layer) in self.layers().enumerate() {
            // Only the pack root contains the overlay directories as ordinary subtrees.
            let skip: &[String] = if index == self.overlays.len() {
                &skip_top
            } else {
                &[]
            };
            list_layer_files(layer, &norm_prefix, skip, &mut seen, &mut results);
        }
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

    fn open(&self, relative_path: &str) -> Result<Option<Vec<u8>>, ResourceError> {
        let clean = relative_path.replace('\\', "/");
        Ok(self.files.get(&clean).cloned())
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
    /// Applicable overlay entry prefixes from `pack.mcmeta`, in declaration order (later = higher
    /// priority, as in `CompositePackResources`).
    overlay_prefixes: Vec<String>,
    /// `pack.mcmeta`'s `filter.block`.
    filter: Option<ResourceFilterSection>,
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

        let overlay_prefixes = Self::load_overlays(&file_map);
        let filter = Self::load_filter(&file_map);
        Ok(Self {
            name: name.into(),
            file_map,
            archive_meta,
            source_label: Some(label),
            overlay_prefixes,
            filter,
        })
    }

    /// Load and cache from a raw byte slice.
    pub fn from_bytes(name: impl Into<String>, bytes: &[u8]) -> Result<Self, ResourceError> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        let file_map = Self::load_archive(&mut archive)?;

        let overlay_prefixes = Self::load_overlays(&file_map);
        let filter = Self::load_filter(&file_map);
        Ok(Self {
            name: name.into(),
            file_map,
            archive_meta: None,
            source_label: None,
            overlay_prefixes,
            filter,
        })
    }

    /// Reads `pack.mcmeta` out of the archive and resolves the applicable overlays.
    ///
    /// Missing or malformed metadata is ignored, matching `Pack.readPackMetadata`.
    fn load_overlays(file_map: &HashMap<String, Vec<u8>>) -> Vec<String> {
        let Some(bytes) = file_map.get("pack.mcmeta") else {
            return Vec::new();
        };
        let Some(meta) = PackMetaFile::parse_json(bytes) else {
            return Vec::new();
        };
        meta.applicable_overlays(CURRENT_CLIENT_RESOURCE_FORMAT)
            .into_iter()
            .map(|directory| format!("{directory}/"))
            .collect()
    }

    /// Reads `pack.mcmeta`'s `filter.block` out of the archive.
    fn load_filter(file_map: &HashMap<String, Vec<u8>>) -> Option<ResourceFilterSection> {
        PackMetaFile::parse_json(file_map.get("pack.mcmeta")?)?.filter
    }

    /// Entry prefixes to consult, highest priority first: overlays (last declared first), then the
    /// archive root (`""`).
    fn layers(&self) -> impl Iterator<Item = &str> {
        self.overlay_prefixes
            .iter()
            .rev()
            .map(String::as_str)
            .chain(std::iter::once(""))
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

    fn filter(&self) -> Option<&ResourceFilterSection> {
        self.filter.as_ref()
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

    fn open(&self, relative_path: &str) -> Result<Option<Vec<u8>>, ResourceError> {
        if normalize_relative_path(relative_path).is_none() {
            return Ok(None);
        }
        let clean = relative_path.replace('\\', "/");
        for prefix in self.layers() {
            if let Some(bytes) = self.file_map.get(&format!("{prefix}{clean}")) {
                return Ok(Some(bytes.clone()));
            }
        }
        Ok(None)
    }

    fn list_files(&self, prefix: &str) -> Vec<String> {
        let norm_prefix = prefix.replace('\\', "/");
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (index, layer) in self.layers().enumerate() {
            // The archive root contains the overlay directories as ordinary entries.
            let is_root = index == self.overlay_prefixes.len();
            for key in self.file_map.keys() {
                let Some(rel) = key.strip_prefix(layer) else {
                    continue;
                };
                if is_root
                    && self
                        .overlay_prefixes
                        .iter()
                        .any(|overlay| key.starts_with(overlay.as_str()))
                {
                    continue;
                }
                if !norm_prefix.is_empty() && !rel.starts_with(&norm_prefix) {
                    continue;
                }
                if seen.insert(rel.to_string()) {
                    results.push(rel.to_string());
                }
            }
        }
        results
    }
}
