//! Host-agnostic active `.mtkcache` resolver.
//!
//! Blender and C4D both need to pick the cache package that matches the active
//! resource-pack stack fingerprint. Keeping the scan/validation policy here
//! avoids re-implementing glob/mtime/header rules in every host.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::reader::AssetCacheReader;

fn mtime(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn newest(mut candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.sort_by_key(|p| mtime(p));
    candidates.pop()
}

/// Resolve the active cache package in `cache_dir`.
///
/// * `fingerprint = Some(fp)`: prefer exact direct names, then scan all
///   `.mtkcache` files and validate their package header fingerprint. Returns
///   the newest matching package, or `None` if none match.
/// * `fingerprint = None`: returns the newest `.mtkcache` package.
pub fn resolve_active_cache_package(
    cache_dir: impl AsRef<Path>,
    fingerprint: Option<&str>,
) -> Option<PathBuf> {
    let dir = cache_dir.as_ref();
    if !dir.is_dir() {
        return None;
    }

    if let Some(fp) = fingerprint.filter(|value| !value.trim().is_empty()) {
        for name in [
            format!("{fp}.mtkcache"),
            format!("mtk_fp_{fp}.mtkcache"),
            fp.to_string(),
        ] {
            let candidate = dir.join(name);
            if AssetCacheReader::is_valid_cache_file(&candidate, fp) {
                return Some(candidate);
            }
        }

        let matching: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "mtkcache"))
            .filter(|path| AssetCacheReader::is_valid_cache_file(path, fp))
            .collect();
        return newest(matching);
    }

    let all: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "mtkcache"))
        .filter(|path| path.is_file())
        .collect();
    newest(all)
}

/// Evaluates the readiness/freshness status of precompiled caches against an active stack and cache directory.
/// Returns one of: "READY", "MISSING", "STALE", "NO_PACKS", "NO_VANILLA".
pub fn evaluate_cache_status(
    cache_dir: impl AsRef<Path>,
    stack_pack_count: usize,
    has_vanilla: bool,
    expected_fingerprint: &str,
) -> &'static str {
    if stack_pack_count == 0 {
        return "NO_PACKS";
    }
    if !has_vanilla {
        return "NO_VANILLA";
    }

    let dir = cache_dir.as_ref();
    let has_any_package = if dir.is_dir() {
        std::fs::read_dir(dir)
            .ok()
            .map(|mut rd| {
                rd.any(|ent| {
                    ent.ok().map(|e| e.path()).is_some_and(|p| {
                        p.extension().is_some_and(|ext| ext == "mtkcache") && p.is_file()
                    })
                })
            })
            .unwrap_or(false)
    } else {
        false
    };

    let fp = expected_fingerprint.trim();
    if fp.is_empty() {
        return if has_any_package { "STALE" } else { "MISSING" };
    }

    if resolve_active_cache_package(dir, Some(fp)).is_some() {
        return "READY";
    }

    if has_any_package {
        "STALE"
    } else {
        "MISSING"
    }
}
