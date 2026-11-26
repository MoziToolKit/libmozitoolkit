//! # Cross-Platform Local IPC Transport Framing
//!
//! Provides non-blocking length-prefixed binary framing over `interprocess::local_socket`
//! (Unix Domain Sockets on macOS/Linux, Named Pipes on Windows).

use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(not(windows))]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{
    prelude::*, Listener, ListenerNonblockingMode, ListenerOptions, Name, Stream,
};
use url::Url;

/// Maximum binary packet size permitted over IPC (512 KiB, aligned with WebSocket config).
pub(crate) const MAX_IPC_FRAME_SIZE: usize = 512 * 1024;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) enum IpcReadError {
    Closed,
    InvalidFrameSize(usize),
    Io(io::Error),
}

/// Incremental non-blocking frame reader for `[u32 LE length][payload]` packets.
#[derive(Debug, Default)]
pub(crate) struct IpcFrameReader {
    len_buf: [u8; 4],
    len_read: usize,
    payload: Vec<u8>,
    payload_read: usize,
}

impl IpcFrameReader {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Attempts to read one complete frame without blocking.
    ///
    /// Returns `Ok(None)` when no complete frame is available yet (`WouldBlock`).
    pub(crate) fn poll_frame(
        &mut self,
        mut stream: &Stream,
    ) -> Result<Option<Vec<u8>>, IpcReadError> {
        loop {
            if self.len_read < 4 {
                match stream.read(&mut self.len_buf[self.len_read..]) {
                    Ok(0) => return Err(IpcReadError::Closed),
                    Ok(n) => {
                        self.len_read += n;
                        if self.len_read < 4 {
                            continue;
                        }
                        let frame_len = u32::from_le_bytes(self.len_buf) as usize;
                        if frame_len == 0 || frame_len > MAX_IPC_FRAME_SIZE {
                            return Err(IpcReadError::InvalidFrameSize(frame_len));
                        }
                        self.payload.resize(frame_len, 0);
                        self.payload_read = 0;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(ref e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::TimedOut =>
                    {
                        return Ok(None);
                    }
                    Err(e) => return Err(IpcReadError::Io(e)),
                }
            }

            let target_len = self.payload.len();
            while self.payload_read < target_len {
                match stream.read(&mut self.payload[self.payload_read..]) {
                    Ok(0) => return Err(IpcReadError::Closed),
                    Ok(n) => self.payload_read += n,
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(ref e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::TimedOut =>
                    {
                        return Ok(None);
                    }
                    Err(e) => return Err(IpcReadError::Io(e)),
                }
            }

            let finished = std::mem::take(&mut self.payload);
            self.len_read = 0;
            self.payload_read = 0;
            return Ok(Some(finished));
        }
    }
}

/// Atomically writes one length-prefixed frame to a non-blocking local socket stream.
pub(crate) fn write_ipc_frame(mut stream: &Stream, payload: &[u8]) -> io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_IPC_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("IPC frame size {} out of bounds", payload.len()),
        ));
    }

    let len_prefix = (payload.len() as u32).to_le_bytes();
    write_all_with_timeout(&mut stream, &len_prefix, WRITE_TIMEOUT)?;
    write_all_with_timeout(&mut stream, payload, WRITE_TIMEOUT)?;
    Ok(())
}

fn write_all_with_timeout(stream: &mut &Stream, buf: &[u8], timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    let mut written = 0usize;
    while written < buf.len() {
        match stream.write(&buf[written..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "IPC peer closed write stream",
                ));
            }
            Ok(n) => written += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "IPC frame write timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Resolves a logical endpoint ID (`mtk-sync-...`) or `ipc://` URL into an OS local socket `Name`.
pub(crate) fn resolve_ipc_name_from_url(url_str: &str) -> Result<Name<'static>, String> {
    let parsed =
        Url::parse(url_str).map_err(|e| format!("Invalid IPC URL '{}': {}", url_str, e))?;
    if parsed.scheme() != "ipc" {
        return Err(format!("Unsupported IPC scheme '{}'", parsed.scheme()));
    }

    if let Some(host) = parsed.host_str() {
        if !host.is_empty() && (parsed.path().is_empty() || parsed.path() == "/") {
            let default_dir = crate::discovery::default_registry_dir();
            return local_socket_name_for_endpoint(host, &default_dir);
        }
    }

    let raw_path = parsed.path();
    if raw_path.is_empty() || raw_path == "/" {
        return Err(format!("IPC URL '{}' has empty endpoint", url_str));
    }

    #[cfg(windows)]
    {
        let trimmed = raw_path.trim_start_matches('/');
        trimmed
            .to_string()
            .to_ns_name::<GenericNamespaced>()
            .map_err(|e| format!("Invalid Windows named pipe '{}': {}", trimmed, e))
    }

    #[cfg(not(windows))]
    {
        raw_path
            .to_string()
            .to_fs_name::<GenericFilePath>()
            .map_err(|e| format!("Invalid Unix socket path '{}': {}", raw_path, e))
    }
}

/// Builds the OS local socket `Name` for a logical endpoint ID within `registry_dir`.
pub(crate) fn local_socket_name_for_endpoint(
    endpoint_id: &str,
    registry_dir: &Path,
) -> Result<Name<'static>, String> {
    let sanitized = sanitize_endpoint_id(endpoint_id);
    #[cfg(windows)]
    {
        let _ = registry_dir;
        sanitized
            .to_ns_name::<GenericNamespaced>()
            .map_err(|e| format!("Failed to create Windows named pipe name: {}", e))
    }

    #[cfg(not(windows))]
    {
        let sock_path = registry_dir.join(format!("{sanitized}.sock"));
        sock_path
            .clone()
            .to_fs_name::<GenericFilePath>()
            .map_err(|e| {
                format!(
                    "Failed to create Unix socket name '{}': {}",
                    sock_path.display(),
                    e
                )
            })
    }
}

/// Formats the canonical `ipc://` URL clients pass to `LiveSyncSession::start`.
pub(crate) fn format_ipc_url(
    endpoint_id: &str,
    registry_dir: &Path,
    is_default_dir: bool,
) -> String {
    let sanitized = sanitize_endpoint_id(endpoint_id);
    #[cfg(windows)]
    {
        let _ = (registry_dir, is_default_dir);
        format!("ipc://{sanitized}")
    }

    #[cfg(not(windows))]
    {
        if is_default_dir {
            format!("ipc://{sanitized}")
        } else {
            let sock_path = registry_dir.join(format!("{sanitized}.sock"));
            format!("ipc://{}", sock_path.display())
        }
    }
}

/// Binds a non-blocking local socket listener for the given endpoint ID.
pub(crate) fn bind_ipc_listener(
    endpoint_id: &str,
    registry_dir: &Path,
) -> Result<Listener, String> {
    let name = local_socket_name_for_endpoint(endpoint_id, registry_dir)?;
    ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Both)
        .try_overwrite(true)
        .create_sync()
        .map_err(|e| format!("Failed to bind IPC listener '{}': {}", endpoint_id, e))
}

/// Connects a non-blocking local socket stream to an `ipc://` URL.
pub(crate) fn connect_ipc_stream(url_str: &str) -> Result<Stream, String> {
    let name = resolve_ipc_name_from_url(url_str)?;
    let stream = Stream::connect(name).map_err(|e| format!("IPC connect failed: {}", e))?;
    stream
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set IPC nonblocking mode: {}", e))?;
    Ok(stream)
}

pub(crate) fn sanitize_endpoint_id(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "mtk-sync".to_string()
    } else {
        cleaned
    }
}
