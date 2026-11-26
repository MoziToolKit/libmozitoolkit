//! # Local Minecraft Instance Discovery & Crash-Safe Liveness
//!
//! Manages per-instance metadata manifests and advisory file locks so local
//! DCC hosts (e.g. Blender) can discover running Minecraft instances without
//! probing TCP ports or leaving stale entries behind after crashes.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

static ENDPOINT_SEQ: AtomicU64 = AtomicU64::new(1);

/// Descriptive metadata supplied by the Minecraft host for local instance discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceMetadata {
    /// Active world / save display name (e.g. "Creative City").
    pub world_name: String,
    /// Minecraft version string (e.g. "1.21.4").
    pub mc_version: String,
    /// Mod loader name (e.g. "Fabric", "NeoForge", "Forge").
    pub loader: String,
    /// Current dimension identifier (e.g. "minecraft:overworld").
    pub dimension: String,
}

/// Discovered local `LiveSyncServer` IPC instance returned to host UIs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSyncInstance {
    /// OS process ID hosting the server.
    pub pid: u32,
    /// 32-character lowercase hex server UUID.
    pub server_uuid: String,
    /// Canonical `ipc://` URL to pass into `LiveSyncSession::start`.
    pub ipc_url: String,
    /// Active world / save display name.
    pub world_name: String,
    /// Minecraft version string.
    pub mc_version: String,
    /// Mod loader name.
    pub loader: String,
    /// Current dimension identifier.
    pub dimension: String,
    /// Whether connecting over IPC requires presenting `auth_token`.
    pub requires_token: bool,
    /// Optional WebSocket bind address if WS listener is also active.
    pub ws_bind_address: Option<String>,
    /// Unix epoch timestamp in milliseconds when the instance started.
    pub started_at_ms: u64,
}

/// Holds the active advisory lock and cleans up discovery artifacts on drop.
pub(crate) struct InstanceRegistrationGuard {
    lock_file: Option<File>,
    lock_path: PathBuf,
    json_path: PathBuf,
    #[cfg(not(windows))]
    sock_path: PathBuf,
    snapshot: LocalSyncInstance,
}

impl InstanceRegistrationGuard {
    pub(crate) fn create(
        registry_dir: &Path,
        endpoint_id: &str,
        ipc_url: String,
        server_uuid: [u8; 16],
        metadata: InstanceMetadata,
        requires_token: bool,
        ws_bind_address: Option<String>,
    ) -> Result<Self, String> {
        ensure_registry_dir(registry_dir)?;

        let stem = crate::transport_ipc::sanitize_endpoint_id(endpoint_id);
        let lock_path = registry_dir.join(format!("{stem}.lock"));
        let json_path = registry_dir.join(format!("{stem}.json"));
        #[cfg(not(windows))]
        let sock_path = registry_dir.join(format!("{stem}.sock"));

        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| format!("Failed to open lock file '{}': {}", lock_path.display(), e))?;

        lock_file.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => {
                format!(
                    "IPC instance lock '{}' is already held",
                    lock_path.display()
                )
            }
            TryLockError::Error(err) => {
                format!("Failed to lock '{}': {}", lock_path.display(), err)
            }
        })?;

        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let snapshot = LocalSyncInstance {
            pid: std::process::id(),
            server_uuid: format_uuid_hex(&server_uuid),
            ipc_url,
            world_name: metadata.world_name,
            mc_version: metadata.mc_version,
            loader: metadata.loader,
            dimension: metadata.dimension,
            requires_token,
            ws_bind_address,
            started_at_ms,
        };

        write_json_atomic(&json_path, &snapshot)?;

        Ok(Self {
            lock_file: Some(lock_file),
            lock_path,
            json_path,
            #[cfg(not(windows))]
            sock_path,
            snapshot,
        })
    }

    pub(crate) fn update_metadata(&mut self, metadata: InstanceMetadata) -> Result<(), String> {
        self.snapshot.world_name = metadata.world_name;
        self.snapshot.mc_version = metadata.mc_version;
        self.snapshot.loader = metadata.loader;
        self.snapshot.dimension = metadata.dimension;
        write_json_atomic(&self.json_path, &self.snapshot)
    }

    pub(crate) fn update_ws_bind_address(
        &mut self,
        ws_bind_address: Option<String>,
    ) -> Result<(), String> {
        self.snapshot.ws_bind_address = ws_bind_address;
        write_json_atomic(&self.json_path, &self.snapshot)
    }

    pub(crate) fn update_requires_token(&mut self, requires_token: bool) -> Result<(), String> {
        self.snapshot.requires_token = requires_token;
        write_json_atomic(&self.json_path, &self.snapshot)
    }
}

impl Drop for InstanceRegistrationGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.json_path);
        #[cfg(not(windows))]
        let _ = fs::remove_file(&self.sock_path);
        // Drop the file handle before removing `.lock` so Windows does not reject deletion.
        drop(self.lock_file.take());
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Generates a process- and instance-unique logical endpoint ID (`mtk-sync-<pid>-<seq>-<uuid4>`).
pub(crate) fn generate_endpoint_id(server_uuid: &[u8; 16]) -> String {
    let pid = std::process::id();
    let seq = ENDPOINT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!(
        "mtk-sync-{pid}-{seq}-{:02x}{:02x}{:02x}{:02x}",
        server_uuid[0], server_uuid[1], server_uuid[2], server_uuid[3]
    )
}

/// Returns the default directory for IPC discovery manifests and Unix sockets.
///
/// Uses `std::env::temp_dir().join("mtk-sync")` when short enough to stay safely under
/// the Unix `sun_path` 104-byte limit, falling back to `/tmp/mtk-sync` otherwise.
pub(crate) fn default_registry_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::temp_dir().join("mtk-sync")
    }

    #[cfg(not(windows))]
    {
        // Keep Unix socket directory path short to stay well within macOS's 104-byte sun_path limit.
        let tmp = std::env::temp_dir().join("mtk-sync");
        if tmp.as_os_str().len() <= 64 {
            tmp
        } else {
            PathBuf::from("/tmp/mtk-sync")
        }
    }
}

/// Discovers currently running local `LiveSyncServer` IPC instances.
///
/// Automatically prunes stale `.json`, `.lock`, and `.sock` artifacts left behind by crashed processes.
pub fn discover_local_instances() -> Vec<LocalSyncInstance> {
    discover_local_instances_in(&default_registry_dir())
}

pub(crate) fn discover_local_instances_in(registry_dir: &Path) -> Vec<LocalSyncInstance> {
    let Ok(entries) = fs::read_dir(registry_dir) else {
        return Vec::new();
    };

    let mut live = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let lock_path = registry_dir.join(format!("{stem}.lock"));
        let sock_path = registry_dir.join(format!("{stem}.sock"));

        if is_lock_held(&lock_path) {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(instance) = serde_json::from_str::<LocalSyncInstance>(&raw) {
                    live.push(instance);
                }
            }
        } else {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(&lock_path);
            let _ = fs::remove_file(&sock_path);
        }
    }

    live.sort_by_key(|a| std::cmp::Reverse(a.started_at_ms));
    live
}

fn is_lock_held(lock_path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().read(true).write(true).open(lock_path) else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => false,
    }
}

fn ensure_registry_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| {
        format!(
            "Failed to create IPC registry dir '{}': {}",
            dir.display(),
            e
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn write_json_atomic(path: &Path, instance: &LocalSyncInstance) -> Result<(), String> {
    let json = serde_json::to_string_pretty(instance)
        .map_err(|e| format!("Failed to serialize instance manifest: {}", e))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, json)
        .map_err(|e| format!("Failed to write '{}': {}", tmp_path.display(), e))?;
    for attempt in 0..5 {
        match fs::rename(&tmp_path, path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 4 => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(format!("Failed to commit '{}': {}", path.display(), e));
            }
        }
    }
    Ok(())
}

fn format_uuid_hex(uuid: &[u8; 16]) -> String {
    let mut hex = String::with_capacity(32);
    for b in uuid {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mtk-sync-discovery-test-{}-{}-{}",
            tag,
            std::process::id(),
            ENDPOINT_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test registry dir");
        dir
    }

    #[test]
    fn discovery_lists_live_instance_updates_metadata_and_prunes_stale_entries() {
        let dir = temp_registry_dir("pruning");

        // Create a stale unlocked entry simulating a crashed process.
        let stale_stem = "mtk-sync-999999-deadbeef";
        let stale_lock = dir.join(format!("{stale_stem}.lock"));
        let stale_json = dir.join(format!("{stale_stem}.json"));
        let stale_sock = dir.join(format!("{stale_stem}.sock"));
        fs::write(&stale_lock, b"").expect("write stale lock");
        fs::write(&stale_sock, b"").expect("write stale sock");
        fs::write(
            &stale_json,
            r#"{"pid":999999,"server_uuid":"deadbeef","ipc_url":"ipc://stale","world_name":"Old","mc_version":"1.20.1","loader":"Fabric","dimension":"minecraft:overworld","requires_token":false,"ws_bind_address":null,"started_at_ms":1}"#,
        )
        .expect("write stale json");

        let mut guard = InstanceRegistrationGuard::create(
            &dir,
            "mtk-sync-live-test",
            "ipc://mtk-sync-live-test".to_string(),
            [0x12; 16],
            InstanceMetadata {
                world_name: "Alpha".to_string(),
                mc_version: "1.21.4".to_string(),
                loader: "Fabric".to_string(),
                dimension: "minecraft:overworld".to_string(),
            },
            false,
            None,
        )
        .expect("create live guard");

        let found = discover_local_instances_in(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].world_name, "Alpha");
        assert_eq!(found[0].ipc_url, "ipc://mtk-sync-live-test");
        assert!(!stale_json.exists(), "stale json must be pruned");
        assert!(!stale_lock.exists(), "stale lock must be pruned");
        assert!(!stale_sock.exists(), "stale sock must be pruned");

        guard
            .update_metadata(InstanceMetadata {
                world_name: "Beta".to_string(),
                mc_version: "1.21.4".to_string(),
                loader: "NeoForge".to_string(),
                dimension: "minecraft:the_nether".to_string(),
            })
            .expect("update metadata");

        let updated = discover_local_instances_in(&dir);
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].world_name, "Beta");
        assert_eq!(updated[0].dimension, "minecraft:the_nether");

        drop(guard);
        assert!(discover_local_instances_in(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
