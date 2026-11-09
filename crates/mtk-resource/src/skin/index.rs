//! On-disk and in-memory index mapping player usernames, UUIDs, and URLs to content hashes.
//!
//! Enforces canonical content-hash file naming (`<sha256>.png`), deduplicates identical skins,
//! and migrates legacy cache files (`player_*.png` and `uuid_*.png`) automatically.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Individual skin record stored in `index.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SkinRecord {
    /// 64-hex SHA256 content hash of the PNG file (or custom CDN hash).
    pub(crate) hash: String,
    /// Texture URL if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) skin_url: Option<String>,
    /// Associated player username (e.g. "jaxlocke").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) player_name: Option<String>,
    /// Associated online/canonical UUID (32-hex lowercase).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) uuid: Option<String>,
    /// Unix timestamp in seconds when this skin was last verified/downloaded.
    #[serde(default)]
    pub(crate) last_checked: u64,
}

fn default_index_version() -> u32 {
    1
}

/// Disk and in-memory index mapping player usernames, UUIDs, and URLs to skin content hashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SkinIndex {
    #[serde(default = "default_index_version")]
    pub(crate) version: u32,
    /// Metadata for each unique skin keyed by content hash.
    #[serde(default)]
    pub(crate) skins: HashMap<String, SkinRecord>,
    /// Lookup from lowercase player username (e.g. "jaxlocke") to 64-hex content hash.
    #[serde(default)]
    pub(crate) players: HashMap<String, String>,
    /// Lookup from clean 32-hex lowercase UUID (no dashes, no prefix) to 64-hex content hash.
    #[serde(default)]
    pub(crate) uuids: HashMap<String, String>,
}

impl Default for SkinIndex {
    fn default() -> Self {
        Self {
            version: 1,
            skins: HashMap::new(),
            players: HashMap::new(),
            uuids: HashMap::new(),
        }
    }
}

/// Normalizes a UUID string by stripping `uuid_` / `uuid:` prefixes, dashes, and lowercasing.
pub fn clean_uuid(raw_uuid: &str) -> String {
    let s = raw_uuid
        .strip_prefix("uuid_")
        .or_else(|| raw_uuid.strip_prefix("uuid:"))
        .unwrap_or(raw_uuid);
    s.replace('-', "").to_ascii_lowercase()
}

/// Normalizes a player name by stripping `player_` / `player:` prefixes, trimming, and lowercasing.
pub fn clean_player_name(raw_name: &str) -> String {
    let s = raw_name
        .strip_prefix("player_")
        .or_else(|| raw_name.strip_prefix("player:"))
        .unwrap_or(raw_name);
    s.trim().to_ascii_lowercase()
}

/// Computes the standard lowercase 64-hex SHA256 string for arbitrary bytes.
pub(crate) fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Known SHA-256 content hashes of default fallback skins returned by mirror services
/// when a player or UUID is not found.
pub(crate) const CRAFTHEAD_ALEX_HASH: &str =
    "c5b845b5a11c0bbd4c3ed65107cf3ed363fe85b17358abfa2b19a9e1350d8a86";
pub(crate) const CRAFTHEAD_STEVE_HASH: &str =
    "e9aba3cc087f29abca87f8ca4d85783f72e86b55a57efec7f100ea4998d1309a";
pub(crate) const MINOTAR_FALLBACK_HASH: &str =
    "d67aeb25ab7ae83f56d7e8cda1308c21cf30a9390b3d1178c2217cf688bcdfa3";

/// Returns true if the given hash is a known mirror fallback skin.
pub(crate) fn is_mirror_fallback_hash(hash: &str) -> bool {
    matches!(
        hash,
        CRAFTHEAD_ALEX_HASH | CRAFTHEAD_STEVE_HASH | MINOTAR_FALLBACK_HASH
    )
}

/// Returns true if a 32-hex UUID is formatted as a Mojang online Version 4 UUID.
/// (13th hex character is '4').
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
pub(crate) fn is_mojang_online_uuid(clean_uuid: &str) -> bool {
    clean_uuid.len() == 32 && clean_uuid.as_bytes().get(12) == Some(&b'4')
}

/// Returns current Unix epoch timestamp in seconds.
pub(crate) fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl SkinIndex {
    /// Attempts to locate an existing skin content hash given URL, UUID, or player name.
    pub(crate) fn find_hash(
        &self,
        skin_url: Option<&str>,
        uuid: Option<&str>,
        name: Option<&str>,
    ) -> Option<String> {
        if let Some(url) = skin_url {
            let clean = url.trim_end_matches('/');
            if let Some(last_seg) = clean.rsplit('/').next() {
                if last_seg.len() == 64 && last_seg.chars().all(|c| c.is_ascii_hexdigit()) {
                    let h = last_seg.to_ascii_lowercase();
                    if !is_mirror_fallback_hash(&h) {
                        return Some(h);
                    }
                }
            }
            let cdn_key = format!(
                "cdn_{:016x}",
                xxhash_rust::xxh64::xxh64(clean.as_bytes(), 0)
            );
            if self.skins.contains_key(&cdn_key) {
                return Some(cdn_key);
            }
        }

        if let Some(u) = uuid {
            let clean_u = clean_uuid(u);
            if let Some(h) = self.uuids.get(&clean_u) {
                if !is_mirror_fallback_hash(h) {
                    return Some(h.clone());
                }
            }
        }

        if let Some(n) = name {
            let clean_n = clean_player_name(n);
            if let Some(h) = self.players.get(&clean_n) {
                if !is_mirror_fallback_hash(h) {
                    return Some(h.clone());
                }
            }
        }

        None
    }

    /// Records or updates a skin mapping in the index.
    pub(crate) fn record_mapping(
        &mut self,
        hash: &str,
        skin_url: Option<&str>,
        uuid: Option<&str>,
        name: Option<&str>,
        timestamp: u64,
    ) {
        let clean_u = uuid.map(clean_uuid).filter(|s| !s.is_empty());
        let clean_n = name.map(clean_player_name).filter(|s| !s.is_empty());

        if let Some(ref u) = clean_u {
            self.uuids.insert(u.clone(), hash.to_string());
        }
        if let Some(ref n) = clean_n {
            self.players.insert(n.clone(), hash.to_string());
        }

        let entry = self
            .skins
            .entry(hash.to_string())
            .or_insert_with(|| SkinRecord {
                hash: hash.to_string(),
                skin_url: skin_url.map(|s| s.to_string()),
                player_name: clean_n.clone(),
                uuid: clean_u.clone(),
                last_checked: timestamp,
            });

        entry.last_checked = timestamp;
        if skin_url.is_some() {
            entry.skin_url = skin_url.map(|s| s.to_string());
        }
        if clean_n.is_some() {
            entry.player_name = clean_n;
        }
        if clean_u.is_some() {
            entry.uuid = clean_u;
        }
    }

    /// Loads `index.json` from `dir`, scans for legacy files, migrates duplicates to `<hash>.png`,
    /// and persists changes if any migration occurred.
    pub(crate) fn load_and_migrate(dir: &Path) -> Self {
        let index_path = dir.join("index.json");
        let mut index = if let Ok(bytes) = fs::read(&index_path) {
            serde_json::from_slice::<SkinIndex>(&bytes).unwrap_or_default()
        } else {
            SkinIndex::default()
        };

        let mut modified = false;

        // Purge any poison caused by previous mirror fallback downloads
        let mut poisoned_hashes = Vec::new();
        for h in index.skins.keys() {
            if is_mirror_fallback_hash(h) {
                poisoned_hashes.push(h.clone());
            }
        }
        for ph in &poisoned_hashes {
            index.skins.remove(ph);
            index.players.retain(|_, v| v != ph);
            index.uuids.retain(|_, v| v != ph);
            let p = dir.join(format!("{}.png", ph));
            let _ = fs::remove_file(p);
            modified = true;
        }

        let before_p_len = index.players.len();
        index.players.retain(|_, v| !is_mirror_fallback_hash(v));
        if index.players.len() != before_p_len {
            modified = true;
        }

        let before_u_len = index.uuids.len();
        index.uuids.retain(|_, v| !is_mirror_fallback_hash(v));
        if index.uuids.len() != before_u_len {
            modified = true;
        }

        let now = current_unix_timestamp();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("png") {
                    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

                    // 1. Standard 64-hex hash file
                    if file_name.len() == 64 && file_name.chars().all(|c| c.is_ascii_hexdigit()) {
                        let hash = file_name.to_ascii_lowercase();
                        if is_mirror_fallback_hash(&hash) {
                            let _ = fs::remove_file(&path);
                            modified = true;
                            continue;
                        }
                        if !index.skins.contains_key(&hash) {
                            index.skins.insert(
                                hash.clone(),
                                SkinRecord {
                                    hash,
                                    skin_url: None,
                                    player_name: None,
                                    uuid: None,
                                    last_checked: now,
                                },
                            );
                            modified = true;
                        }
                    }
                    // 2. Legacy `player_<name>.png` -> compute sha256, write `<hash>.png`, remove legacy
                    else if file_name.starts_with("player_") {
                        let name = clean_player_name(file_name);
                        if let Ok(bytes) = fs::read(&path) {
                            if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
                                let hash = compute_sha256_hex(&bytes);
                                if is_mirror_fallback_hash(&hash) {
                                    let _ = fs::remove_file(&path);
                                    modified = true;
                                    continue;
                                }
                                let target_path = dir.join(format!("{}.png", hash));
                                let _ = fs::write(&target_path, &bytes);
                                let _ = fs::remove_file(&path);
                                index.record_mapping(&hash, None, None, Some(&name), now);
                                modified = true;
                            }
                        }
                    }
                    // 3. Legacy `uuid_<uuid>.png` -> compute sha256, write `<hash>.png`, remove legacy
                    else if file_name.starts_with("uuid_") {
                        let u = clean_uuid(file_name);
                        if let Ok(bytes) = fs::read(&path) {
                            if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
                                let hash = compute_sha256_hex(&bytes);
                                if is_mirror_fallback_hash(&hash) {
                                    let _ = fs::remove_file(&path);
                                    modified = true;
                                    continue;
                                }
                                let target_path = dir.join(format!("{}.png", hash));
                                let _ = fs::write(&target_path, &bytes);
                                let _ = fs::remove_file(&path);
                                index.record_mapping(&hash, None, Some(&u), None, now);
                                modified = true;
                            }
                        }
                    }
                }
            }
        }

        if modified {
            let _ = index.save_to_disk(dir);
        }

        index
    }

    /// Persists index data to `<dir>/index.json`.
    pub(crate) fn save_to_disk(&self, dir: &Path) -> Result<(), std::io::Error> {
        let index_path = dir.join("index.json");
        let json_str = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(index_path, json_str)
    }
}
