//! # Cross-Platform Local IPC Transport Framing
//!
//! Provides non-blocking length-prefixed binary framing over `interprocess::local_socket`
//! (Unix Domain Sockets on macOS/Linux, Named Pipes on Windows).

use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub(crate) fn poll_frame(&mut self, stream: &Stream) -> Result<Option<Vec<u8>>, IpcReadError> {
        loop {
            if self.len_read < 4 {
                let Some(n) = read_ipc_chunk(stream, &mut self.len_buf[self.len_read..])? else {
                    return Ok(None);
                };
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

            let target_len = self.payload.len();
            while self.payload_read < target_len {
                let Some(n) = read_ipc_chunk(stream, &mut self.payload[self.payload_read..])?
                else {
                    return Ok(None);
                };
                self.payload_read += n;
            }

            let finished = std::mem::take(&mut self.payload);
            self.len_read = 0;
            self.payload_read = 0;
            return Ok(Some(finished));
        }
    }
}

fn read_ipc_chunk(stream: &Stream, dst: &mut [u8]) -> Result<Option<usize>, IpcReadError> {
    loop {
        match read_ipc_bytes(stream, dst) {
            Ok(0) => return Err(IpcReadError::Closed),
            Ok(n) => return Ok(Some(n)),
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                return Ok(None);
            }
            Err(e) => return Err(IpcReadError::Io(e)),
        }
    }
}

#[cfg(not(windows))]
fn read_ipc_bytes(mut stream: &Stream, buf: &mut [u8]) -> io::Result<usize> {
    stream.read(buf)
}

// On Windows, `interprocess`'s `set_nonblocking(true)` enables `PIPE_NOWAIT`, under which empty
// `ReadFileEx` returns `ERROR_NO_DATA` (mapped to `BrokenPipe` and swallowed to `Ok(0)` by
// `interprocess::downgrade_eof`), making an idle pipe indistinguishable from EOF. Keep pipe streams
// in `PIPE_WAIT` mode and non-blockingly probe available bytes with `PeekNamedPipe` before reading.
#[cfg(windows)]
fn read_ipc_bytes(mut stream: &Stream, buf: &mut [u8]) -> io::Result<usize> {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    use windows_sys::Win32::Foundation::{
        ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED,
    };
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    let Stream::NamedPipe(pipe) = stream;
    let mut total_bytes_avail: u32 = 0;
    // SAFETY: `pipe.as_handle().as_raw_handle()` borrows the open Named Pipe handle for this
    // Win32 `PeekNamedPipe` call, null pointers opt out of unused outputs per Win32 docs, and
    // `&mut total_bytes_avail` points to initialized stack storage.
    #[allow(unsafe_code)]
    let ok = unsafe {
        PeekNamedPipe(
            pipe.as_handle().as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut total_bytes_avail,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let err = io::Error::last_os_error();
        return match err.raw_os_error().and_then(|c| u32::try_from(c).ok()) {
            Some(ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA) => Ok(0),
            _ => Err(err),
        };
    }
    if total_bytes_avail == 0 {
        return Err(io::Error::from(io::ErrorKind::WouldBlock));
    }
    let to_read = buf.len().min(total_bytes_avail as usize);
    stream.read(&mut buf[..to_read])
}

/// Writes one length-prefixed frame to a local socket stream, aborting early if `running` flips false.
pub(crate) fn write_ipc_frame_cancellable(
    mut stream: &Stream,
    payload: &[u8],
    running: Option<&AtomicBool>,
) -> io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_IPC_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("IPC frame size {} out of bounds", payload.len()),
        ));
    }

    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    write_all_with_timeout(&mut stream, &frame, WRITE_TIMEOUT, running)
}

fn write_all_with_timeout(
    stream: &mut &Stream,
    buf: &[u8],
    timeout: Duration,
    running: Option<&AtomicBool>,
) -> io::Result<()> {
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
                if running.is_some_and(|r| !r.load(Ordering::Relaxed)) {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "IPC write cancelled",
                    ));
                }
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

/// Binds a local socket listener whose `.accept()` is non-blocking.
pub(crate) fn bind_ipc_listener(
    endpoint_id: &str,
    registry_dir: &Path,
) -> Result<Listener, String> {
    let name = local_socket_name_for_endpoint(endpoint_id, registry_dir)?;
    #[cfg(windows)]
    let mode = ListenerNonblockingMode::Accept;
    #[cfg(not(windows))]
    let mode = ListenerNonblockingMode::Both;

    ListenerOptions::new()
        .name(name)
        .nonblocking(mode)
        .try_overwrite(true)
        .create_sync()
        .map_err(|e| format!("Failed to bind IPC listener '{}': {}", endpoint_id, e))
}

/// Applies platform-appropriate I/O mode to a newly accepted IPC stream.
pub(crate) fn configure_accepted_ipc_stream(stream: &Stream) {
    #[cfg(windows)]
    let _ = stream.set_nonblocking(false);
    #[cfg(not(windows))]
    let _ = stream.set_nonblocking(true);
}

/// Connects a local socket stream to an `ipc://` URL.
pub(crate) fn connect_ipc_stream(url_str: &str) -> Result<Stream, String> {
    let name = resolve_ipc_name_from_url(url_str)?;
    let stream = Stream::connect(name).map_err(|e| format!("IPC connect failed: {}", e))?;
    #[cfg(not(windows))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn ipc_roundtrip_idle_poll_large_frame_and_eof() {
        let dir = std::env::temp_dir().join(format!(
            "mtk-sync-ipc-unit-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp ipc dir");

        let endpoint_id = format!(
            "mtk-test-ipc-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        );
        let listener = bind_ipc_listener(&endpoint_id, &dir).expect("bind ipc listener");
        let url = format_ipc_url(&endpoint_id, &dir, false);

        let done = Arc::new(AtomicBool::new(false));
        let done_client = done.clone();
        let url_client = url.clone();
        let client_handle = std::thread::spawn(move || {
            let stream = connect_ipc_stream(&url_client).expect("client connect");
            let mut reader = IpcFrameReader::new();
            // Empty pipe before peer writes must return Ok(None), never EOF.
            assert!(
                matches!(reader.poll_frame(&stream), Ok(None)),
                "idle pipe read must report WouldBlock (Ok(None))"
            );

            write_ipc_frame_cancellable(&stream, b"hello-ipc", None)
                .expect("client write small frame");

            let large_expected = vec![0x5Au8; 128 * 1024];
            let deadline = Instant::now() + Duration::from_secs(3);
            let received = loop {
                match reader.poll_frame(&stream) {
                    Ok(Some(frame)) => break frame,
                    Ok(None) => {
                        assert!(
                            Instant::now() < deadline,
                            "timed out waiting for large frame"
                        );
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => panic!("unexpected client poll error: {e:?}"),
                }
            };
            assert_eq!(received, large_expected);
            done_client.store(true, Ordering::Release);
        });

        let deadline = Instant::now() + Duration::from_secs(3);
        let accepted = loop {
            match listener.accept() {
                Ok(s) => break s,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "timed out waiting for accept");
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("unexpected accept error: {e}"),
            }
        };
        configure_accepted_ipc_stream(&accepted);

        let mut server_reader = IpcFrameReader::new();
        let small = loop {
            match server_reader.poll_frame(&accepted) {
                Ok(Some(frame)) => break frame,
                Ok(None) => {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for small frame"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("unexpected server poll error: {e:?}"),
            }
        };
        assert_eq!(small, b"hello-ipc");

        let large_payload = vec![0x5Au8; 128 * 1024];
        write_ipc_frame_cancellable(&accepted, &large_payload, None)
            .expect("server write large frame");

        client_handle.join().expect("client thread join");
        assert!(done.load(Ordering::Acquire));

        let closed_deadline = Instant::now() + Duration::from_secs(2);
        let saw_closed = loop {
            match server_reader.poll_frame(&accepted) {
                Err(IpcReadError::Closed) => break true,
                Ok(None) => {
                    if Instant::now() >= closed_deadline {
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                other => panic!("expected Closed after client drop, got {other:?}"),
            }
        };
        assert!(
            saw_closed,
            "server must observe IpcReadError::Closed on peer drop"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
