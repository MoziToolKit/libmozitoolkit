//! HELLO construction helpers shared by every session start path (RFC v3.1 §4.1).
//!
//! The auth token must ride on *every* HELLO, including the no-cache default one:
//! Blender never uses the cached fast path (`VoxelStorage` is not persisted into
//! `.blend`), so omitting the token here makes a token-protected Yefira server
//! reject an otherwise identical secret with `Authentication failed`.

use glam::IVec3;

use super::state::SessionState;
use crate::protocol::{Packet, PROTOCOL_VERSION};

/// Builds the HELLO used when the host has no persisted session state.
pub(crate) fn default_hello(auth_token: &str) -> Packet {
    Packet::Hello {
        min_version: PROTOCOL_VERSION as u16,
        max_version: PROTOCOL_VERSION as u16,
        capabilities: 0x03, // Zstd (1) + DNA Direct Memory (2)
        flags: 0,
        cached_world_uuid: [0u8; 16],
        cached_epoch: 0,
        cached_seq: 0,
        cached_min: IVec3::ZERO,
        cached_size: IVec3::ZERO,
        cached_state_hash: 0,
        auth_token: auth_token.to_string(),
        cached_dimension: "minecraft:overworld".to_string(),
    }
}

/// Applies the session auth token to a host-built (cached) HELLO.
///
/// The session-level token wins when it is non-empty so a cached HELLO cannot
/// silently ship a stale secret; when the session has no token the host-provided
/// value is preserved for hosts that manage per-connection secrets themselves.
pub(crate) fn with_auth_token(mut hello: Packet, auth_token: &str) -> Packet {
    if let Packet::Hello {
        auth_token: hello_token,
        ..
    } = &mut hello
    {
        if !auth_token.is_empty() {
            *hello_token = auth_token.to_string();
        }
    }
    hello
}

/// Builds the HELLO packet for a (re)connection attempt.
///
/// When the session already holds a valid server-confirmed snapshot state (non-zero
/// `server_uuid`, `current_state_hash`, and `size`), reconnecting uses those live
/// coordinates and sequence instead of re-sending a stale `initial_hello`.
pub(crate) fn reconnect_hello(initial_hello: &Packet, state: &SessionState) -> Packet {
    if state.server_uuid != [0u8; 16] && state.current_state_hash != 0 && state.size != IVec3::ZERO
    {
        if let Packet::Hello {
            min_version,
            max_version,
            capabilities,
            flags,
            auth_token,
            ..
        } = initial_hello
        {
            return Packet::Hello {
                min_version: *min_version,
                max_version: *max_version,
                capabilities: *capabilities,
                flags: *flags,
                cached_world_uuid: state.server_uuid,
                cached_epoch: state.epoch,
                cached_seq: state.sequence,
                cached_min: state.min_pos,
                cached_size: state.size,
                cached_state_hash: state.current_state_hash,
                auth_token: auth_token.clone(),
                cached_dimension: state.dimension.clone(),
            };
        }
    }
    initial_hello.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello_token(packet: &Packet) -> &str {
        match packet {
            Packet::Hello { auth_token, .. } => auth_token,
            other => panic!("expected HELLO, got {other:?}"),
        }
    }

    #[test]
    fn default_hello_carries_auth_token() {
        let hello = default_hello("s3cret");
        assert_eq!(hello_token(&hello), "s3cret");

        match &hello {
            Packet::Hello {
                cached_state_hash,
                cached_min,
                capabilities,
                ..
            } => {
                assert_eq!(
                    *cached_state_hash, 0,
                    "no-cache HELLO must not claim a cache"
                );
                assert_eq!(*cached_min, IVec3::ZERO);
                assert_eq!(
                    *capabilities & 0x04,
                    0,
                    "protocol 4.0 no longer advertises the visual entity stream"
                );
            }
            other => panic!("expected HELLO, got {other:?}"),
        }
    }

    #[test]
    fn reconnect_hello_uses_live_session_state_when_available() {
        let initial = default_hello("tok-1");
        let empty_state = SessionState::new();
        assert_eq!(reconnect_hello(&initial, &empty_state), initial);

        let mut live_state = SessionState::new();
        live_state.server_uuid = [0xAB; 16];
        live_state.epoch = 4;
        live_state.sequence = 17;
        live_state.min_pos = IVec3::new(-16, 64, 32);
        live_state.size = IVec3::new(16, 16, 16);
        live_state.current_state_hash = 0x1234_5678_9ABC_DEF0;
        live_state.dimension = "minecraft:the_nether".to_string();

        match reconnect_hello(&initial, &live_state) {
            Packet::Hello {
                cached_world_uuid,
                cached_epoch,
                cached_seq,
                cached_min,
                cached_size,
                cached_state_hash,
                auth_token,
                cached_dimension,
                ..
            } => {
                assert_eq!(cached_world_uuid, [0xAB; 16]);
                assert_eq!(cached_epoch, 4);
                assert_eq!(cached_seq, 17);
                assert_eq!(cached_min, IVec3::new(-16, 64, 32));
                assert_eq!(cached_size, IVec3::new(16, 16, 16));
                assert_eq!(cached_state_hash, 0x1234_5678_9ABC_DEF0);
                assert_eq!(auth_token, "tok-1");
                assert_eq!(cached_dimension, "minecraft:the_nether");
            }
            other => panic!("expected HELLO, got {other:?}"),
        }
    }
}
