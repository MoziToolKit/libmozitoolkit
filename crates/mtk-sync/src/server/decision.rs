//! # Authoritative Handshake Adjudication (RFC v3.1 Section 4.1 & 4.2)

use crate::protocol::{Packet, ProtocolError, SyncDecision, PROTOCOL_VERSION};
use glam::IVec3;

/// Current server state context against which a client's `HelloPacket` is evaluated.
#[derive(Debug, Clone)]
pub struct HandshakeContext {
    pub server_uuid: [u8; 16],
    pub epoch: u64,
    pub sequence: u64,
    pub min_pos: IVec3,
    pub size: IVec3,
    pub state_hash: u64,
    pub dimension: String,
    pub expected_auth_token: Option<String>,
    pub is_loopback: bool,
    pub allow_loopback_without_token: bool,
}

/// Result of evaluating a client's handshake request.
#[derive(Debug, Clone)]
pub struct HandshakeResult {
    pub decision: SyncDecision,
    pub welcome_packet: Packet,
}

/// Evaluates a client `Hello` packet against the server's current state and produces a `WELCOME` packet.
pub fn decide_handshake(
    hello_pkt: &Packet,
    ctx: &HandshakeContext,
) -> Result<HandshakeResult, ProtocolError> {
    let (
        min_version,
        max_version,
        _capabilities,
        _flags,
        cached_world_uuid,
        cached_epoch,
        cached_seq,
        cached_min,
        cached_size,
        cached_state_hash,
        auth_token,
        cached_dimension,
    ) = match hello_pkt {
        Packet::Hello {
            min_version,
            max_version,
            capabilities,
            flags,
            cached_world_uuid,
            cached_epoch,
            cached_seq,
            cached_min,
            cached_size,
            cached_state_hash,
            auth_token,
            cached_dimension,
            ..
        } => (
            *min_version,
            *max_version,
            *capabilities,
            *flags,
            *cached_world_uuid,
            *cached_epoch,
            *cached_seq,
            *cached_min,
            *cached_size,
            *cached_state_hash,
            auth_token,
            cached_dimension,
        ),
        _ => {
            return Err(ProtocolError::UnknownOpcode(0));
        }
    };

    // 1. Version negotiation check
    if min_version > PROTOCOL_VERSION as u16 || max_version < PROTOCOL_VERSION as u16 {
        return Ok(HandshakeResult {
            decision: SyncDecision::Rejected,
            welcome_packet: Packet::Welcome {
                negotiated_version: PROTOCOL_VERSION as u16,
                server_uuid: ctx.server_uuid,
                epoch: ctx.epoch,
                server_seq: ctx.sequence,
                current_min: ctx.min_pos,
                current_size: ctx.size,
                current_state_hash: ctx.state_hash,
                decision: SyncDecision::Rejected,
                message_or_dimension: "Unsupported protocol version".to_string(),
            },
        });
    }

    // 2. Auth token check (both sides trimmed)
    if let Some(expected) = &ctx.expected_auth_token {
        let expected_trimmed = expected.trim();
        if !expected_trimmed.is_empty() {
            let provided_trimmed = auth_token.trim();
            let is_loopback_bypass =
                ctx.is_loopback && ctx.allow_loopback_without_token && provided_trimmed.is_empty();
            if !is_loopback_bypass && expected_trimmed != provided_trimmed {
                return Ok(HandshakeResult {
                    decision: SyncDecision::Rejected,
                    welcome_packet: Packet::Welcome {
                        negotiated_version: PROTOCOL_VERSION as u16,
                        server_uuid: ctx.server_uuid,
                        epoch: ctx.epoch,
                        server_seq: ctx.sequence,
                        current_min: ctx.min_pos,
                        current_size: ctx.size,
                        current_state_hash: ctx.state_hash,
                        decision: SyncDecision::Rejected,
                        message_or_dimension: "Authentication failed".to_string(),
                    },
                });
            }
        }
    }

    // 3. Cache verification
    let is_zero_uuid = cached_world_uuid.iter().all(|&b| b == 0);
    let uuid_matches = !is_zero_uuid && cached_world_uuid == ctx.server_uuid;
    let epoch_matches = cached_epoch == ctx.epoch;
    let seq_matches = cached_seq == ctx.sequence;
    let bounds_match = cached_min == ctx.min_pos && cached_size == ctx.size;
    let dimension_matches = cached_dimension.trim() == ctx.dimension.trim();
    let hash_matches = ctx.state_hash != 0 && cached_state_hash == ctx.state_hash;

    let decision = if uuid_matches
        && epoch_matches
        && seq_matches
        && bounds_match
        && dimension_matches
        && hash_matches
    {
        SyncDecision::VerifiedSkipRebuild
    } else {
        SyncDecision::FullBaselineRequired
    };

    let welcome_packet = Packet::Welcome {
        negotiated_version: PROTOCOL_VERSION as u16,
        server_uuid: ctx.server_uuid,
        epoch: ctx.epoch,
        server_seq: ctx.sequence,
        current_min: ctx.min_pos,
        current_size: ctx.size,
        current_state_hash: ctx.state_hash,
        decision,
        message_or_dimension: ctx.dimension.clone(),
    };

    Ok(HandshakeResult {
        decision,
        welcome_packet,
    })
}
