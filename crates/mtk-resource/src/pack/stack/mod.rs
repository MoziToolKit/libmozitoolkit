pub mod collector;
pub mod sprite;

pub use sprite::{DiscoveredSprite, PbrCompanions};

use crate::atlas::AtlasDefinition;
use crate::error::ResourceError;
use crate::identifier::ResourceLocation;
use crate::pack::source::ResourcePack;
use std::collections::HashSet;
use std::path::Path;
use xxhash_rust::xxh64::Xxh64;

/// Ordered hierarchy of resource packs evaluated from top (highest priority) to bottom (vanilla fallback).
pub struct ResourcePackStack {
    packs: Vec<Box<dyn ResourcePack>>,
}

impl Default for ResourcePackStack {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourcePackStack {
    pub fn new() -> Self {
        Self { packs: Vec::new() }
    }

    /// Push a resource pack to the top of the stack (highest priority).
    pub fn push_pack(&mut self, pack: Box<dyn ResourcePack>) {
        self.packs.insert(0, pack);
    }

    /// Append a resource pack to the bottom of the stack (lowest priority fallback anchor).
    pub fn append_pack(&mut self, pack: Box<dyn ResourcePack>) {
        self.packs.push(pack);
    }

    /// Number of packs in the stack.
    pub fn len(&self) -> usize {
        self.packs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    /// Returns true if the stack contains at least one pack providing vanilla minecraft client assets
    /// (e.g. `assets/minecraft/textures/` or `assets/minecraft/blockstates/` or `assets/minecraft/models/`).
    pub fn has_vanilla_client_base(&self) -> bool {
        if self.packs.is_empty() {
            return false;
        }
        for pack in &self.packs {
            for file in pack.list_files("assets/minecraft/") {
                if file.starts_with("assets/minecraft/textures/")
                    || file.starts_with("assets/minecraft/blockstates/")
                    || file.starts_with("assets/minecraft/models/")
                {
                    return true;
                }
            }
        }
        false
    }

    /// Returns the slice of active resource packs in priority order.
    pub fn packs(&self) -> &[Box<dyn ResourcePack>] {
        &self.packs
    }

    /// Open a file by its explicit asset path, returning the first match from top to bottom.
    pub fn open_asset_raw(&self, asset_path: &str) -> Option<Vec<u8>> {
        for pack in &self.packs {
            if let Some(bytes) = pack.open(asset_path) {
                return Some(bytes);
            }
        }
        None
    }

    /// Open a specific categorized asset (e.g. category `"textures"`, ext `"png"`).
    pub fn open_texture_raw(&self, location: &ResourceLocation) -> Option<Vec<u8>> {
        let path = location.to_asset_path("textures", "png");
        self.open_asset_raw(&path)
    }

    /// List all unique blockstate resource locations discovered across all active packs in the stack.
    pub fn list_all_blockstate_locations(&self) -> Vec<ResourceLocation> {
        let mut seen = HashSet::new();
        let mut results = Vec::new();

        for pack in &self.packs {
            for file in pack.list_files("assets/") {
                if !file.ends_with(".json") {
                    continue;
                }
                if let Some(loc) = ResourceLocation::from_asset_path(&file, "blockstates", "json") {
                    if seen.insert(loc.clone()) {
                        results.push(loc);
                    }
                }
            }
        }

        results
    }

    /// List all unique atlas resource locations discovered across all active packs in the stack.
    pub fn list_all_atlas_locations(&self) -> Vec<ResourceLocation> {
        let mut seen = HashSet::new();
        let mut results = Vec::new();

        for pack in &self.packs {
            for file in pack.list_files("assets/") {
                if !file.ends_with(".json") {
                    continue;
                }
                if let Some(loc) = ResourceLocation::from_asset_path(&file, "atlases", "json") {
                    if seen.insert(loc.clone()) {
                        results.push(loc);
                    }
                }
            }
        }

        results
    }

    /// Read and parse an atlas definition from `assets/<namespace>/atlases/<name>.json`.
    pub fn load_atlas_definition(
        &self,
        location: &ResourceLocation,
    ) -> Result<AtlasDefinition, ResourceError> {
        let path = location.to_asset_path("atlases", "json");
        let bytes = self.open_asset_raw(&path).ok_or_else(|| {
            ResourceError::NotFound(format!("Atlas definition not found: {}", path))
        })?;
        let json_str = std::str::from_utf8(&bytes).map_err(|e| {
            ResourceError::AtlasConfig(format!("Invalid UTF-8 in atlas JSON: {}", e))
        })?;
        AtlasDefinition::parse_json(json_str).map_err(ResourceError::from)
    }

    /// Read and parse an atlas definition for a category, falling back to standard default if not found.
    pub fn load_atlas_category(&self, category: &crate::atlas::AtlasCategory) -> AtlasDefinition {
        let loc = category.atlas_location();
        let mut def = self
            .load_atlas_definition(&loc)
            .unwrap_or_else(|_| category.default_definition());

        if matches!(category, crate::atlas::AtlasCategory::Blocks) {
            let end_portal_loc = ResourceLocation::vanilla("entity/end_portal/end_portal");
            let has_end_portal = def.sources.iter().any(|s| match s {
                crate::atlas::AtlasSource::Single { resource, .. } => resource == &end_portal_loc,
                _ => false,
            });
            if !has_end_portal {
                def.sources.push(crate::atlas::AtlasSource::Single {
                    resource: end_portal_loc,
                    sprite: None,
                });
            }
        } else if matches!(category, crate::atlas::AtlasCategory::Paintings) {
            // Vanilla paintings.json defines `prefix: ""` which strips category prefix.
            // Canonicalize to `painting/` so all painting sprites are consistently
            // namespaced (e.g. `minecraft:painting/kebab`), preventing fluid collisions
            // and atlas stitching duplication.
            for source in &mut def.sources {
                if let crate::atlas::AtlasSource::Directory {
                    source: src,
                    prefix,
                } = source
                {
                    if src == "painting" && prefix.is_empty() {
                        *prefix = "painting/".to_string();
                    }
                }
            }
        }

        def
    }

    /// Open a model JSON by its identifier, resolving standard candidate asset paths.
    pub fn open_model_raw(&self, model_id: &str) -> Option<Vec<u8>> {
        for path in ResourceLocation::model_candidate_asset_paths(model_id) {
            if let Some(bytes) = self.open_asset_raw(&path) {
                return Some(bytes);
            }
        }
        None
    }

    /// Computes the stack fingerprint that keys the asset cache.
    ///
    /// The fingerprint covers each pack's identity *and its content*: a repainted texture, an
    /// added model or a repacked jar must all invalidate the cache. Hashing pack names alone let
    /// an edited pack reuse the previous cache forever.
    pub fn compute_stack_fingerprint(&self) -> String {
        let mut hasher = Xxh64::new(0);
        hasher.update(FINGERPRINT_DOMAIN.as_bytes());
        hasher.update(&(self.packs.len() as u64).to_le_bytes());
        for pack in &self.packs {
            hash_pack_content(&mut hasher, &pack.source_label(), pack.content_entries());
        }
        format!("mtk_fp_{:016x}", hasher.digest())
    }

    /// Computes the same fingerprint from pack paths, without building a stack.
    ///
    /// Hosts call this on the UI thread to decide READY/STALE before loading any archive, so it
    /// must agree exactly with [`Self::compute_stack_fingerprint`] for the equivalent stack.
    /// Directories are walked (one stat per file); archives contribute their own size and mtime.
    pub fn compute_fingerprint_from_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> String {
        let paths: Vec<&str> = paths.into_iter().collect();
        let mut hasher = Xxh64::new(0);
        hasher.update(FINGERPRINT_DOMAIN.as_bytes());
        hasher.update(&(paths.len() as u64).to_le_bytes());
        for path in paths {
            let resolved = Path::new(path);
            let canonical =
                std::fs::canonicalize(resolved).unwrap_or_else(|_| resolved.to_path_buf());
            let label = canonical.display().to_string();
            hash_pack_content(&mut hasher, &label, content_entries_from_path(&canonical));
        }
        format!("mtk_fp_{:016x}", hasher.digest())
    }
}

/// Domain separator: bump when the fingerprint's meaning changes, so old caches stop matching.
const FINGERPRINT_DOMAIN: &str = "mtk_pack_fp_v2";

/// Describes a pack path without reading any file bytes.
///
/// Must mirror [`ResourcePack::content_entries`] for the pack type the path would produce:
/// a directory is walked, a file contributes its own size and mtime.
fn content_entries_from_path(path: &Path) -> Vec<crate::pack::source::PackContentEntry> {
    use crate::pack::source::PackContentEntry;

    let Ok(meta) = std::fs::metadata(path) else {
        return Vec::new();
    };
    if meta.is_file() {
        return vec![PackContentEntry {
            path: String::new(),
            size: meta.len(),
            modified_secs: crate::pack::source::modified_secs(&meta),
        }];
    }
    if !meta.is_dir() {
        return Vec::new();
    }
    crate::pack::source::content_entries_from_dir(path)
}

/// Feeds one pack's identity and content into the fingerprint.
///
/// Shared by the loaded-stack and path-only variants so they cannot drift: the two must produce
/// the same digest for the same packs, or a host decides READY and then loads a different package.
fn hash_pack_content(
    hasher: &mut Xxh64,
    label: &str,
    mut entries: Vec<crate::pack::source::PackContentEntry>,
) {
    hasher.update(&(label.len() as u64).to_le_bytes());
    hasher.update(label.as_bytes());
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    hasher.update(&(entries.len() as u64).to_le_bytes());
    for entry in entries {
        hasher.update(&(entry.path.len() as u64).to_le_bytes());
        hasher.update(entry.path.as_bytes());
        hasher.update(&entry.size.to_le_bytes());
        hasher.update(&entry.modified_secs.to_le_bytes());
    }
}
