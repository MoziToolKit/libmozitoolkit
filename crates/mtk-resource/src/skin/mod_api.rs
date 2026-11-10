//! Base skin path and key derivation utilities.

use super::index::{clean_player_name, clean_uuid};
use xxhash_rust::xxh64::xxh64;

/// Derives a stable, filesystem-safe canonical cache key for a skin reference.
pub fn derive_skin_key(skin_url: Option<&str>, uuid: Option<&str>, name: Option<&str>) -> String {
    if let Some(url) = skin_url {
        let clean = url.trim_end_matches('/');
        if let Some(last_seg) = clean.rsplit('/').next() {
            // Standard Mojang 64-hex SHA256 texture hash
            if last_seg.len() == 64 && last_seg.chars().all(|c| c.is_ascii_hexdigit()) {
                return last_seg.to_ascii_lowercase();
            }
        }
        // Custom CDN or URL with query parameters
        let h = xxh64(clean.as_bytes(), 0);
        return format!("cdn_{:016x}", h);
    }

    if let Some(u) = uuid {
        let clean_u = clean_uuid(u);
        if !clean_u.is_empty() {
            return format!("uuid_{}", clean_u);
        }
    }

    if let Some(n) = name {
        let clean_n = clean_player_name(n);
        if !clean_n.is_empty() {
            return format!("player_{}", clean_n);
        }
    }

    "default_steve".to_string()
}
