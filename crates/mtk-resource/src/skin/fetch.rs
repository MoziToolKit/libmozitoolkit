//! Online network fetching and profile decoding for player skin textures.

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
use std::io::Read;

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
use super::index::{
    clean_player_name, clean_uuid, compute_sha256_hex, is_mirror_fallback_hash,
    is_mojang_online_uuid,
};
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
use super::mirror::{query_mirror_skin, read_png_body};
use super::resolver::SkinResolver;

impl SkinResolver {
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    pub(crate) fn fetch_network_with_url(
        &self,
        skin_url: Option<&str>,
        uuid: Option<&str>,
        name: Option<&str>,
    ) -> Option<(Option<String>, Vec<u8>)> {
        // Fast-fail if the circuit breaker is tripped (e.g. offline or DNS down)
        if self.circuit.is_tripped() {
            return None;
        }

        // 1. Direct URL download
        if let Some(url) = skin_url {
            let bytes = self.http_get_png(url)?;
            let hash = compute_sha256_hex(&bytes);
            if is_mirror_fallback_hash(&hash) {
                return None;
            }
            return Some((Some(url.to_string()), bytes));
        }

        let clean_u = uuid.map(clean_uuid).filter(|u| u.len() == 32);
        let clean_n = name.map(clean_player_name).filter(|n| !n.is_empty());

        let is_v4_uuid = clean_u
            .as_ref()
            .map(|u| is_mojang_online_uuid(u))
            .unwrap_or(false);

        // 2. High-speed Mirror First (avoids Mojang rate limits and 3-hop cascade)
        // If UUID is an online v4 UUID, query UUID first, then name.
        // If UUID is an offline/synthetic UUID (e.g. v3) and name is present, query name first!
        if is_v4_uuid {
            if let Some(ref u) = clean_u {
                if let Some((url, bytes)) = query_mirror_skin(&self.agent, &self.circuit, u) {
                    return Some((Some(url), bytes));
                }
            }
            if let Some(ref n) = clean_n {
                if let Some((url, bytes)) = query_mirror_skin(&self.agent, &self.circuit, n) {
                    return Some((Some(url), bytes));
                }
            }
        } else {
            if let Some(ref n) = clean_n {
                if let Some((url, bytes)) = query_mirror_skin(&self.agent, &self.circuit, n) {
                    return Some((Some(url), bytes));
                }
            }
            if let Some(ref u) = clean_u {
                if let Some((url, bytes)) = query_mirror_skin(&self.agent, &self.circuit, u) {
                    return Some((Some(url), bytes));
                }
            }
        }

        // 3. Fallback: Mojang Session Server & Profile API
        if is_v4_uuid {
            if let Some(ref u) = clean_u {
                let session_url = format!(
                    "https://sessionserver.mojang.com/session/minecraft/profile/{}",
                    u
                );
                if let Some(url) = self.query_mojang_session(&session_url) {
                    if let Some(bytes) = self.http_get_png(&url) {
                        return Some((Some(url), bytes));
                    }
                }
            }
            if let Some(ref n) = clean_n {
                let profile_url = format!("https://api.mojang.com/users/profiles/minecraft/{}", n);
                if let Some(resolved_uuid) = self.query_mojang_uuid(&profile_url) {
                    let session_url = format!(
                        "https://sessionserver.mojang.com/session/minecraft/profile/{}",
                        resolved_uuid
                    );
                    if let Some(url) = self.query_mojang_session(&session_url) {
                        if let Some(bytes) = self.http_get_png(&url) {
                            return Some((Some(url), bytes));
                        }
                    }
                }
            }
        } else {
            if let Some(ref n) = clean_n {
                let profile_url = format!("https://api.mojang.com/users/profiles/minecraft/{}", n);
                if let Some(resolved_uuid) = self.query_mojang_uuid(&profile_url) {
                    let session_url = format!(
                        "https://sessionserver.mojang.com/session/minecraft/profile/{}",
                        resolved_uuid
                    );
                    if let Some(url) = self.query_mojang_session(&session_url) {
                        if let Some(bytes) = self.http_get_png(&url) {
                            return Some((Some(url), bytes));
                        }
                    }
                }
            }
            if let Some(ref u) = clean_u {
                let session_url = format!(
                    "https://sessionserver.mojang.com/session/minecraft/profile/{}",
                    u
                );
                if let Some(url) = self.query_mojang_session(&session_url) {
                    if let Some(bytes) = self.http_get_png(&url) {
                        return Some((Some(url), bytes));
                    }
                }
            }
        }

        None
    }

    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn http_get_png(&self, url: &str) -> Option<Vec<u8>> {
        if self.circuit.is_tripped() {
            return None;
        }
        let res = self
            .agent
            .get(url)
            .header("User-Agent", "MoziToolKit/2.0")
            .call();

        let resp = match res {
            Ok(r) => {
                self.circuit.record_success();
                r
            }
            Err(ureq::Error::StatusCode(_status)) => {
                self.circuit.record_success();
                return None;
            }
            Err(_e) => {
                self.circuit.record_transport_failure();
                return None;
            }
        };

        if resp.status().as_u16() != 200 {
            return None;
        }

        read_png_body(resp)
    }

    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn query_mojang_session(&self, url: &str) -> Option<String> {
        if self.circuit.is_tripped() {
            return None;
        }
        let res = self
            .agent
            .get(url)
            .header("User-Agent", "MoziToolKit/2.0")
            .call();

        let resp = match res {
            Ok(r) => {
                self.circuit.record_success();
                r
            }
            Err(ureq::Error::StatusCode(_status)) => {
                self.circuit.record_success();
                return None;
            }
            Err(_e) => {
                self.circuit.record_transport_failure();
                return None;
            }
        };

        let mut bytes = Vec::new();
        resp.into_body()
            .into_reader()
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .ok()?;
        let json_val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let props = json_val.get("properties")?.as_array()?;
        for p in props {
            if p.get("name")?.as_str()? == "textures" {
                let base64_val = p.get("value")?.as_str()?;
                return extract_skin_url_from_property_base64(base64_val);
            }
        }
        None
    }

    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn query_mojang_uuid(&self, url: &str) -> Option<String> {
        if self.circuit.is_tripped() {
            return None;
        }
        let res = self
            .agent
            .get(url)
            .header("User-Agent", "MoziToolKit/2.0")
            .call();

        let resp = match res {
            Ok(r) => {
                self.circuit.record_success();
                r
            }
            Err(ureq::Error::StatusCode(_status)) => {
                self.circuit.record_success();
                return None;
            }
            Err(_e) => {
                self.circuit.record_transport_failure();
                return None;
            }
        };

        let mut bytes = Vec::new();
        resp.into_body()
            .into_reader()
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .ok()?;
        let json_val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let id = json_val.get("id")?.as_str()?;
        Some(id.to_string())
    }
}

/// Decodes base64 payload and extracts textures.SKIN.url.
pub fn extract_skin_url_from_property_base64(base64_str: &str) -> Option<String> {
    let decoded = decode_base64(base64_str.as_bytes())?;
    let json_str = std::str::from_utf8(&decoded).ok()?;
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    val.get("textures")?
        .get("SKIN")?
        .get("url")?
        .as_str()
        .map(|s| s.to_string())
}

pub fn decode_base64(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in bytes {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}
