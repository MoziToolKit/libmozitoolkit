//! Multi-tier resolver for player and custom head skin textures.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
use super::circuit::SkinCircuitBreaker;
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
use super::index::{clean_player_name, clean_uuid, is_mirror_fallback_hash, is_mojang_online_uuid};
use super::index::{compute_sha256_hex, current_unix_timestamp, SkinIndex};
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
use super::mirror::{query_mirror_skin, read_png_body};
use super::mod_api::derive_skin_key;

/// Checks PNG magic signature (89 50 4E 47 0D 0A 1A 0A).
#[inline]
pub fn is_valid_png_header(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n"
}

/// Multi-tier resolver for player and custom head skin textures.
#[derive(Clone)]
pub struct SkinResolver {
    cache_dir: Option<PathBuf>,
    pub index: Arc<RwLock<SkinIndex>>,
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    pub circuit: SkinCircuitBreaker,
    memory_cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    in_flight: Arc<RwLock<HashSet<String>>>,
    failed_keys: Arc<RwLock<HashMap<String, u64>>>,
    timeout_secs: u64,
    validation_interval_secs: u64,
    negative_cache_ttl_secs: u64,
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    agent: Arc<ureq::Agent>,
}

impl std::fmt::Debug for SkinResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("SkinResolver");
        d.field("cache_dir", &self.cache_dir)
            .field("timeout_secs", &self.timeout_secs)
            .field("validation_interval_secs", &self.validation_interval_secs)
            .field("negative_cache_ttl_secs", &self.negative_cache_ttl_secs);
        #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
        d.field("circuit", &self.circuit);
        d.finish()
    }
}

impl Default for SkinResolver {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SkinResolver {
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn build_agent(timeout_secs: u64) -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .try_proxy_from_env(true)
            .build()
    }

    /// Creates a new resolver with an optional on-disk cache directory.
    pub fn new(cache_dir: Option<PathBuf>) -> Self {
        let index = if let Some(ref dir) = cache_dir {
            let _ = fs::create_dir_all(dir);
            SkinIndex::load_and_migrate(dir)
        } else {
            SkinIndex::default()
        };

        let timeout_secs = 5;

        Self {
            cache_dir,
            index: Arc::new(RwLock::new(index)),
            #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
            circuit: SkinCircuitBreaker::default(),
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(HashSet::new())),
            failed_keys: Arc::new(RwLock::new(HashMap::new())),
            timeout_secs,
            validation_interval_secs: 3600, // 1 hour validation TTL
            negative_cache_ttl_secs: 300,   // 5 minutes negative cache cooldown
            #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
            agent: Arc::new(Self::build_agent(timeout_secs)),
        }
    }

    /// Configures the circuit breaker threshold and cooldown period in seconds.
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    pub fn with_circuit_breaker(mut self, threshold: u32, cooldown_secs: u64) -> Self {
        self.circuit = SkinCircuitBreaker::new(threshold, cooldown_secs);
        self
    }

    /// Returns true if the circuit breaker is currently tripped (fast-fail mode).
    #[inline]
    pub fn is_circuit_tripped(&self) -> bool {
        #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
        {
            self.circuit.is_tripped()
        }
        #[cfg(any(not(feature = "net"), target_arch = "wasm32"))]
        {
            false
        }
    }

    /// Sets the HTTP request timeout in seconds.
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
        {
            self.agent = Arc::new(Self::build_agent(timeout_secs));
        }
        self
    }

    /// Sets the online skin validation interval TTL in seconds.
    pub fn with_validation_interval(mut self, secs: u64) -> Self {
        self.validation_interval_secs = secs;
        self
    }

    /// Sets the negative cache failure cooldown TTL in seconds.
    pub fn with_negative_cache_ttl(mut self, secs: u64) -> Self {
        self.negative_cache_ttl_secs = secs;
        self
    }

    #[cfg(test)]
    pub(crate) fn backdate_negative_cache_entry_for_test(&self, key: &str, secs: u64) {
        if let Ok(mut failures) = self.failed_keys.write() {
            if let Some(fail_time) = failures.get_mut(key) {
                *fail_time = fail_time.saturating_sub(secs);
            }
        }
    }

    fn is_negative_cached(&self, key: &str, hash_opt: Option<&str>) -> bool {
        if let Ok(failures) = self.failed_keys.read() {
            let now = current_unix_timestamp();
            if let Some(&fail_time) = failures.get(key) {
                if now.saturating_sub(fail_time) < self.negative_cache_ttl_secs {
                    return true;
                }
            }
            if let Some(h) = hash_opt {
                if let Some(&fail_time) = failures.get(h) {
                    if now.saturating_sub(fail_time) < self.negative_cache_ttl_secs {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Injects a decoded or mock skin PNG directly into the in-memory cache.
    pub fn register_memory_skin(&self, key: &str, png_bytes: Vec<u8>) {
        if let Ok(mut mem) = self.memory_cache.write() {
            mem.insert(key.to_string(), png_bytes);
        }
    }

    /// Resolves the canonical 64-hex content hash if known in index or URL.
    pub fn resolve_skin_hash(
        &self,
        skin_url: Option<&str>,
        uuid: Option<&str>,
        name: Option<&str>,
    ) -> Option<String> {
        if let Ok(idx) = self.index.read() {
            idx.find_hash(skin_url, uuid, name)
        } else {
            None
        }
    }

    /// Fast, non-blocking check to retrieve skin bytes in 0 seconds if already in memory or disk.
    ///
    /// Checks memory cache, index-mapped content hash (`<hash>.png`), and performs on-the-fly
    /// legacy file migration if an unmigrated legacy file is discovered.
    pub fn try_resolve_cached(
        &self,
        skin_url: Option<&str>,
        uuid: Option<&str>,
        name: Option<&str>,
    ) -> Option<Vec<u8>> {
        let key = derive_skin_key(skin_url, uuid, name);
        let hash_opt = self.resolve_skin_hash(skin_url, uuid, name);

        // 1. Check in-memory cache by hash or derived key
        if let Ok(mem) = self.memory_cache.read() {
            if let Some(ref h) = hash_opt {
                if let Some(bytes) = mem.get(h) {
                    return Some(bytes.clone());
                }
            }
            if let Some(bytes) = mem.get(&key) {
                return Some(bytes.clone());
            }
        }

        // 2. Check negative cache
        if self.is_negative_cached(&key, hash_opt.as_deref()) {
            return None;
        }

        // 3. Check disk cache
        if let Some(ref dir) = self.cache_dir {
            // Check by canonical content hash first (<hash>.png)
            if let Some(ref h) = hash_opt {
                let path = dir.join(format!("{}.png", h));
                if let Ok(bytes) = fs::read(&path) {
                    if is_valid_png_header(&bytes) {
                        if let Ok(mut mem) = self.memory_cache.write() {
                            mem.insert(h.clone(), bytes.clone());
                            mem.insert(key, bytes.clone());
                        }
                        return Some(bytes);
                    }
                }
            }

            // Fallback check by key name (e.g. legacy file)
            let path = dir.join(format!("{}.png", key));
            if let Ok(bytes) = fs::read(&path) {
                if is_valid_png_header(&bytes) {
                    let h = compute_sha256_hex(&bytes);
                    let target_path = dir.join(format!("{}.png", h));
                    let _ = fs::write(&target_path, &bytes);
                    let _ = fs::remove_file(&path);
                    if let Ok(mut idx) = self.index.write() {
                        let now = current_unix_timestamp();
                        idx.record_mapping(&h, skin_url, uuid, name, now);
                        let _ = idx.save_to_disk(dir);
                    }
                    if let Ok(mut mem) = self.memory_cache.write() {
                        mem.insert(h, bytes.clone());
                        mem.insert(key, bytes.clone());
                    }
                    return Some(bytes);
                }
            }
        }

        None
    }

    /// Synchronous resolution: checks cache, performs network fetch if needed, and caches results.
    pub fn resolve_skin_png(
        &self,
        skin_url: Option<&str>,
        uuid: Option<&str>,
        name: Option<&str>,
    ) -> Option<Vec<u8>> {
        let key = derive_skin_key(skin_url, uuid, name);

        // 1. Non-blocking cache check (0 seconds)
        if let Some(bytes) = self.try_resolve_cached(skin_url, uuid, name) {
            return Some(bytes);
        }

        // 2. Network resolution
        #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
        {
            if let Some((remote_url, bytes)) = self.fetch_network_with_url(skin_url, uuid, name) {
                // Clear from failed_keys on success
                if let Ok(mut failures) = self.failed_keys.write() {
                    failures.remove(&key);
                }

                let hash = compute_sha256_hex(&bytes);
                let now = current_unix_timestamp();

                // Save to disk cache under canonical <hash>.png
                if let Some(ref dir) = self.cache_dir {
                    let target_path = dir.join(format!("{}.png", hash));
                    let _ = fs::write(target_path, &bytes);
                    if let Ok(mut idx) = self.index.write() {
                        idx.record_mapping(&hash, remote_url.as_deref(), uuid, name, now);
                        let _ = idx.save_to_disk(dir);
                    }
                }

                // Save to memory cache
                if let Ok(mut mem) = self.memory_cache.write() {
                    mem.insert(hash, bytes.clone());
                    mem.insert(key, bytes.clone());
                }
                return Some(bytes);
            }
        }

        // 3. Mark failure in negative cache to avoid spamming network
        if let Ok(mut failures) = self.failed_keys.write() {
            failures.insert(key, current_unix_timestamp());
        }

        None
    }

    /// Queues a non-blocking background fetch for a skin if not already cached or pending.
    pub fn queue_fetch_background<F>(
        &self,
        skin_url: Option<String>,
        uuid: Option<String>,
        name: Option<String>,
        on_complete: F,
    ) where
        F: FnOnce(String, Option<Vec<u8>>) + Send + 'static,
    {
        let key = derive_skin_key(skin_url.as_deref(), uuid.as_deref(), name.as_deref());

        // Don't re-queue if already in memory cache or known failure
        if let Ok(mem) = self.memory_cache.read() {
            if let Some(bytes) = mem.get(&key) {
                on_complete(key, Some(bytes.clone()));
                return;
            }
        }
        if self.is_negative_cached(&key, None) {
            on_complete(key, None);
            return;
        }

        // Dedup multiple requests in-flight
        if let Ok(mut flights) = self.in_flight.write() {
            if flights.contains(&key) {
                return;
            }
            flights.insert(key.clone());
        }

        let resolver = self.clone();
        std::thread::spawn(move || {
            let res =
                resolver.resolve_skin_png(skin_url.as_deref(), uuid.as_deref(), name.as_deref());
            if let Ok(mut flights) = resolver.in_flight.write() {
                flights.remove(&key);
            }
            on_complete(key, res);
        });
    }

    /// Checks online status in the background if cached data is stale or unverified,
    /// updating `index.json` and notifying the callback if the remote skin has changed.
    pub fn check_and_queue_online_validation<F>(
        &self,
        uuid: Option<String>,
        name: Option<String>,
        on_updated: F,
    ) where
        F: FnOnce(String, Vec<u8>) + Send + 'static,
    {
        #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
        {
            if uuid.is_none() && name.is_none() {
                return;
            }

            let needs_check = {
                if let Ok(idx) = self.index.read() {
                    if let Some(h) = idx.find_hash(None, uuid.as_deref(), name.as_deref()) {
                        if let Some(rec) = idx.skins.get(&h) {
                            let now = current_unix_timestamp();
                            now.saturating_sub(rec.last_checked) >= self.validation_interval_secs
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    false
                }
            };

            if !needs_check {
                return;
            }

            let key = derive_skin_key(None, uuid.as_deref(), name.as_deref());
            if let Ok(mut flights) = self.in_flight.write() {
                if flights.contains(&key) {
                    return;
                }
                flights.insert(key.clone());
            }

            let resolver = self.clone();
            std::thread::spawn(move || {
                resolver.perform_online_validation(
                    uuid.as_deref(),
                    name.as_deref(),
                    &key,
                    on_updated,
                );
                if let Ok(mut flights) = resolver.in_flight.write() {
                    flights.remove(&key);
                }
            });
        }
        #[cfg(any(not(feature = "net"), target_arch = "wasm32"))]
        {
            let _ = (uuid, name, on_updated);
        }
    }

    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn perform_online_validation<F>(
        &self,
        uuid: Option<&str>,
        name: Option<&str>,
        key: &str,
        on_updated: F,
    ) where
        F: FnOnce(String, Vec<u8>),
    {
        let cached_hash = self.resolve_skin_hash(None, uuid, name);

        if let Some((remote_url, bytes)) = self.fetch_network_with_url(None, uuid, name) {
            let remote_hash = compute_sha256_hex(&bytes);
            let now = current_unix_timestamp();

            if let Some(ref current_h) = cached_hash {
                if current_h == &remote_hash {
                    // Skin unchanged: update timestamp and persist index
                    if let Ok(mut idx) = self.index.write() {
                        if let Some(rec) = idx.skins.get_mut(current_h) {
                            rec.last_checked = now;
                        }
                        if let Some(ref dir) = self.cache_dir {
                            let _ = idx.save_to_disk(dir);
                        }
                    }
                    return;
                }
            }

            // Skin updated online: write new file, update index, and notify callback
            if let Some(ref dir) = self.cache_dir {
                let target_path = dir.join(format!("{}.png", remote_hash));
                let _ = fs::write(target_path, &bytes);
                if let Ok(mut idx) = self.index.write() {
                    idx.record_mapping(&remote_hash, remote_url.as_deref(), uuid, name, now);
                    let _ = idx.save_to_disk(dir);
                }
            }

            if let Ok(mut mem) = self.memory_cache.write() {
                mem.insert(remote_hash, bytes.clone());
                mem.insert(key.to_string(), bytes.clone());
            }

            on_updated(key.to_string(), bytes);
        }
    }

    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn fetch_network_with_url(
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
            .set("User-Agent", "MoziToolKit/2.0")
            .call();

        let resp = match res {
            Ok(r) => {
                self.circuit.record_success();
                r
            }
            Err(ureq::Error::Status(_status, _r)) => {
                self.circuit.record_success();
                return None;
            }
            Err(ureq::Error::Transport(_e)) => {
                self.circuit.record_transport_failure();
                return None;
            }
        };

        if resp.status() != 200 {
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
            .set("User-Agent", "MoziToolKit/2.0")
            .call();

        let resp = match res {
            Ok(r) => {
                self.circuit.record_success();
                r
            }
            Err(ureq::Error::Status(_status, _r)) => {
                self.circuit.record_success();
                return None;
            }
            Err(ureq::Error::Transport(_e)) => {
                self.circuit.record_transport_failure();
                return None;
            }
        };

        use std::io::Read;
        let mut bytes = Vec::new();
        resp.into_reader()
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
            .set("User-Agent", "MoziToolKit/2.0")
            .call();

        let resp = match res {
            Ok(r) => {
                self.circuit.record_success();
                r
            }
            Err(ureq::Error::Status(_status, _r)) => {
                self.circuit.record_success();
                return None;
            }
            Err(ureq::Error::Transport(_e)) => {
                self.circuit.record_transport_failure();
                return None;
            }
        };

        use std::io::Read;
        let mut bytes = Vec::new();
        resp.into_reader()
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
