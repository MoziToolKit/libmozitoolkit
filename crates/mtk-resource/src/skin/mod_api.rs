//! Base skin path and key derivation utilities.

use super::index::{clean_player_name, clean_uuid, is_mojang_online_uuid};
use xxhash_rust::xxh64::xxh64;

/// Upgrades legacy Mojang `http://textures.minecraft.net/` URLs to `https://`.
pub(crate) fn normalize_skin_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://textures.minecraft.net/") {
        format!("https://textures.minecraft.net/{rest}")
    } else {
        url.to_string()
    }
}

/// Builds ordered candidate HTTPS URLs for resolving a player skin via direct texture URL or mirrors.
pub fn candidate_skin_urls(
    skin_url: Option<&str>,
    uuid: Option<&str>,
    name: Option<&str>,
) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(url) = skin_url.map(str::trim).filter(|u| !u.is_empty()) {
        urls.push(normalize_skin_url(url));
    }

    let clean_u = uuid.map(clean_uuid).filter(|u| u.len() == 32);
    let clean_n = name.map(clean_player_name).filter(|n| !n.is_empty());
    let is_v4_uuid = clean_u.as_deref().is_some_and(is_mojang_online_uuid);

    let ordered_ids: [Option<&str>; 2] = if is_v4_uuid {
        [clean_u.as_deref(), clean_n.as_deref()]
    } else {
        [clean_n.as_deref(), clean_u.as_deref()]
    };

    for id in ordered_ids.into_iter().flatten() {
        let encoded = encode_path_segment(id);
        urls.push(format!("https://crafthead.net/skin/{encoded}"));
        urls.push(format!("https://minotar.net/skin/{encoded}"));
    }

    urls
}

fn encode_path_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(&mut out, "%{byte:02X}");
        }
    }
    out
}

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
