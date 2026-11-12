//! HELLO construction helpers shared by every session start path (RFC v3.1 §4.1).
//!
//! The auth token must ride on *every* HELLO, including the no-cache default one:
//! Blender never uses the cached fast path (`VoxelStorage` is not persisted into
//! `.blend`), so omitting the token here makes a token-protected Yefira server
//! reject an otherwise identical secret with `Authentication failed`.

use glam::IVec3;

use crate::protocol::{Packet, PROTOCOL_VERSION};

/// Returns the default subscribed visual entity list from mtk-model's canonical SSOT.
pub(crate) fn default_subscribed_visual_entities() -> Vec<String> {
    mtk_model::supported_visual_entity_ids()
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

/// Builds the HELLO used when the host has no persisted session state.
pub(crate) fn default_hello(auth_token: &str, subscribed_entities: Option<&[String]>) -> Packet {
    let subscribed = match subscribed_entities {
        Some(list) => list.to_vec(),
        None => default_subscribed_visual_entities(),
    };

    Packet::Hello {
        min_version: PROTOCOL_VERSION as u16,
        max_version: PROTOCOL_VERSION as u16,
        capabilities: 0x07, // Zstd (1) + DNA Direct Memory (2) + Visual Entities (4)
        flags: 0,
        cached_world_uuid: [0u8; 16],
        cached_epoch: 0,
        cached_seq: 0,
        cached_min: IVec3::ZERO,
        cached_size: IVec3::ZERO,
        cached_state_hash: 0,
        auth_token: auth_token.to_string(),
        cached_dimension: "minecraft:overworld".to_string(),
        subscribed_visual_entities: subscribed,
    }
}

/// Applies the session auth token and subscriptions to a host-built (cached) HELLO.
///
/// The session-level token wins when it is non-empty so a cached HELLO cannot
/// silently ship a stale secret; when the session has no token the host-provided
/// value is preserved for hosts that manage per-connection secrets themselves.
pub(crate) fn with_auth_token(
    mut hello: Packet,
    auth_token: &str,
    subscribed_entities: Option<&[String]>,
) -> Packet {
    if let Packet::Hello {
        auth_token: hello_token,
        subscribed_visual_entities,
        ..
    } = &mut hello
    {
        if !auth_token.is_empty() {
            *hello_token = auth_token.to_string();
        }
        if subscribed_visual_entities.is_empty() {
            *subscribed_visual_entities = match subscribed_entities {
                Some(list) => list.to_vec(),
                None => default_subscribed_visual_entities(),
            };
        }
    }
    hello
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
        let hello = default_hello("s3cret", None);
        assert_eq!(hello_token(&hello), "s3cret");

        match &hello {
            Packet::Hello {
                cached_state_hash,
                cached_min,
                subscribed_visual_entities,
                ..
            } => {
                assert_eq!(
                    *cached_state_hash, 0,
                    "no-cache HELLO must not claim a cache"
                );
                assert_eq!(*cached_min, IVec3::ZERO);
                assert_eq!(
                    subscribed_visual_entities,
                    &default_subscribed_visual_entities(),
                    "default HELLO must inherit supported visual entities from mtk-model SSOT"
                );
            }
            other => panic!("expected HELLO, got {other:?}"),
        }
    }

    #[test]
    fn default_hello_respects_custom_subscriptions() {
        let custom = vec!["minecraft:painting".to_string()];
        let hello = default_hello("token", Some(&custom));
        match hello {
            Packet::Hello {
                subscribed_visual_entities,
                ..
            } => {
                assert_eq!(subscribed_visual_entities, custom);
            }
            other => panic!("expected HELLO, got {other:?}"),
        }
    }

    #[test]
    fn cached_hello_inherits_session_token_and_preserves_cache() {
        let mut cached = default_hello("", None);
        if let Packet::Hello {
            cached_state_hash,
            cached_min,
            ..
        } = &mut cached
        {
            *cached_state_hash = 0xABCD;
            *cached_min = IVec3::new(1, 2, 3);
        }

        let filled = with_auth_token(cached, "token-from-session", None);
        assert_eq!(hello_token(&filled), "token-from-session");
        if let Packet::Hello {
            cached_state_hash,
            cached_min,
            ..
        } = &filled
        {
            assert_eq!(*cached_state_hash, 0xABCD, "cached fields must survive");
            assert_eq!(*cached_min, IVec3::new(1, 2, 3));
        }

        let host_managed = with_auth_token(default_hello("host-token", None), "", None);
        assert_eq!(
            hello_token(&host_managed),
            "host-token",
            "an unset session token must not clobber a host-provided one"
        );

        let overridden = with_auth_token(default_hello("stale-token", None), "session-token", None);
        assert_eq!(
            hello_token(&overridden),
            "session-token",
            "the session token is authoritative once set"
        );
    }
}
