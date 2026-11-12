//! # Live Sync Python Bindings (RFC v3.1 / v3.2)
//!
//! Provides `LiveSyncSession` (client) and `LiveSyncServer` (authoritative server) to Python callers.

pub mod client;
pub mod server;

pub(crate) use client::PyLiveSyncSession;
pub(crate) use server::PyLiveSyncServer;

pub(crate) fn parse_uuid_hex(raw: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let trimmed = raw.trim();
    let without_prefix = if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        &trimmed[2..]
    } else {
        trimmed
    };
    let clean: Vec<u8> = without_prefix.bytes().filter(|b| *b != b'-').collect();
    if clean.len() != 32 {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(clean[i * 2]);
        let lo = hex_nibble(clean[i * 2 + 1]);
        match (hi, lo) {
            (Some(h), Some(l)) => *slot = (h << 4) | l,
            _ => return [0u8; 16],
        }
    }
    out
}

#[inline]
fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
