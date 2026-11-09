//! # Protocol Binary Decoder (RFC v3.1)
//!
//! Parses WebSocket incoming binary frames into typed `Packet` instances.

use glam::IVec3;
use xxhash_rust::xxh64::Xxh64;

use super::decode_baseline::*;
use crate::protocol::constants::*;
use crate::protocol::error::ProtocolError;
use crate::protocol::packet::*;

/// Computes the xxHash64 checksum for delta batch payload.
pub fn compute_delta_batch_hash(changes: &[DeltaChangeEntry]) -> u64 {
    let mut hasher = Xxh64::new(0);
    for entry in changes {
        hasher.update(&entry.rel_pos.x.to_le_bytes());
        hasher.update(&entry.rel_pos.y.to_le_bytes());
        hasher.update(&entry.rel_pos.z.to_le_bytes());
        hasher.update(&entry.state_id.to_le_bytes());
    }
    hasher.digest()
}

/// Helper to read a little-endian u16-length prefixed UTF-8 string.
pub(crate) fn read_str<'a>(
    buf: &'a [u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<&'a str, ProtocolError> {
    if *offset + 2 > buf.len() {
        return Err(ProtocolError::TruncatedPayload { field });
    }
    let len = u16::from_le_bytes([buf[*offset], buf[*offset + 1]]) as usize;
    *offset += 2;
    if *offset + len > buf.len() {
        return Err(ProtocolError::TruncatedPayload { field });
    }
    let s = std::str::from_utf8(&buf[*offset..*offset + len]).map_err(|e| {
        ProtocolError::Utf8Error {
            field,
            detail: e.to_string(),
        }
    })?;
    *offset += len;
    Ok(s)
}

#[inline]
pub(crate) fn decode_entity_payload(bytes: &[u8]) -> Option<mtk_model::BlockEntityData> {
    if bytes.is_empty() {
        return None;
    }
    serde_json::from_slice::<mtk_model::BlockEntityData>(bytes).ok()
}

/// Decodes binary buffer into a strongly typed `Packet`.
pub fn decode_packet(buf: &[u8]) -> Result<Packet, ProtocolError> {
    if buf.len() < HEADER_SIZE {
        return Err(ProtocolError::PacketTooShort {
            expected: HEADER_SIZE,
            actual: buf.len(),
        });
    }

    if buf[0] != PROTOCOL_MAGIC[0] || buf[1] != PROTOCOL_MAGIC[1] {
        return Err(ProtocolError::InvalidMagic(buf[0], buf[1]));
    }

    let version = buf[2];
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(version));
    }

    let opcode_byte = buf[3];
    let opcode = Opcode::from_u8(opcode_byte).ok_or(ProtocolError::UnknownOpcode(opcode_byte))?;

    let mut offset = HEADER_SIZE;

    match opcode {
        Opcode::Hello => decode_hello(buf, &mut offset),
        Opcode::Welcome => decode_welcome(buf, &mut offset),
        Opcode::BaselineBegin => decode_baseline_begin(buf, &mut offset),
        Opcode::BaselineChunk => decode_baseline_chunk(buf, &mut offset),
        Opcode::BaselineEnd => decode_baseline_end(buf, &mut offset),
        Opcode::BaselineAck => decode_baseline_ack(buf, &mut offset),
        Opcode::DeltaOp => decode_delta_op(buf, &mut offset),
        Opcode::PaletteUpdate => decode_palette_update(buf, &mut offset),
        Opcode::Cancel => decode_cancel(buf, &mut offset),
        Opcode::ResyncRequest => decode_resync_request(buf, &mut offset),
        Opcode::Ping => decode_ping(buf, &mut offset),
        Opcode::Pong => decode_pong(buf, &mut offset),
        Opcode::Error => decode_error(buf, &mut offset),
        Opcode::BaselineEntities => decode_baseline_entities(buf, &mut offset),
        Opcode::DeltaEntity => decode_delta_entity(buf, &mut offset),
    }
}

fn decode_hello(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 76 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "Hello fixed fields",
        });
    }
    let min_version = u16::from_le_bytes(buf[*offset..*offset + 2].try_into().unwrap());
    let max_version = u16::from_le_bytes(buf[*offset + 2..*offset + 4].try_into().unwrap());
    let capabilities = u32::from_le_bytes(buf[*offset + 4..*offset + 8].try_into().unwrap());
    let flags = u32::from_le_bytes(buf[*offset + 8..*offset + 12].try_into().unwrap());
    let mut cached_world_uuid = [0u8; 16];
    cached_world_uuid.copy_from_slice(&buf[*offset + 12..*offset + 28]);
    let cached_epoch = u64::from_le_bytes(buf[*offset + 28..*offset + 36].try_into().unwrap());
    let cached_seq = u64::from_le_bytes(buf[*offset + 36..*offset + 44].try_into().unwrap());
    let c_min_x = i32::from_le_bytes(buf[*offset + 44..*offset + 48].try_into().unwrap());
    let c_min_y = i32::from_le_bytes(buf[*offset + 48..*offset + 52].try_into().unwrap());
    let c_min_z = i32::from_le_bytes(buf[*offset + 52..*offset + 56].try_into().unwrap());
    let c_size_x = i32::from_le_bytes(buf[*offset + 56..*offset + 60].try_into().unwrap());
    let c_size_y = i32::from_le_bytes(buf[*offset + 60..*offset + 64].try_into().unwrap());
    let c_size_z = i32::from_le_bytes(buf[*offset + 64..*offset + 68].try_into().unwrap());
    let cached_state_hash = u64::from_le_bytes(buf[*offset + 68..*offset + 76].try_into().unwrap());
    *offset += 76;

    let auth_token = read_str(buf, offset, "Hello auth_token")?.to_string();
    let cached_dimension = read_str(buf, offset, "Hello cached_dimension")?.to_string();

    Ok(Packet::Hello {
        min_version,
        max_version,
        capabilities,
        flags,
        cached_world_uuid,
        cached_epoch,
        cached_seq,
        cached_min: IVec3::new(c_min_x, c_min_y, c_min_z),
        cached_size: IVec3::new(c_size_x, c_size_y, c_size_z),
        cached_state_hash,
        auth_token,
        cached_dimension,
    })
}

fn decode_welcome(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 67 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "Welcome fixed fields",
        });
    }
    let negotiated_version = u16::from_le_bytes(buf[*offset..*offset + 2].try_into().unwrap());
    let mut server_uuid = [0u8; 16];
    server_uuid.copy_from_slice(&buf[*offset + 2..*offset + 18]);
    let epoch = u64::from_le_bytes(buf[*offset + 18..*offset + 26].try_into().unwrap());
    let server_seq = u64::from_le_bytes(buf[*offset + 26..*offset + 34].try_into().unwrap());
    let min_x = i32::from_le_bytes(buf[*offset + 34..*offset + 38].try_into().unwrap());
    let min_y = i32::from_le_bytes(buf[*offset + 38..*offset + 42].try_into().unwrap());
    let min_z = i32::from_le_bytes(buf[*offset + 42..*offset + 46].try_into().unwrap());
    let size_x = i32::from_le_bytes(buf[*offset + 46..*offset + 50].try_into().unwrap());
    let size_y = i32::from_le_bytes(buf[*offset + 50..*offset + 54].try_into().unwrap());
    let size_z = i32::from_le_bytes(buf[*offset + 54..*offset + 58].try_into().unwrap());
    let current_state_hash =
        u64::from_le_bytes(buf[*offset + 58..*offset + 66].try_into().unwrap());
    let dec_byte = buf[*offset + 66];
    let decision = SyncDecision::from_u8(dec_byte).ok_or(ProtocolError::InvalidEnum {
        field: "SyncDecision",
        val: dec_byte as u32,
    })?;
    *offset += 67;

    let message_or_dimension = read_str(buf, offset, "Welcome message_or_dimension")?.to_string();

    Ok(Packet::Welcome {
        negotiated_version,
        server_uuid,
        epoch,
        server_seq,
        current_min: IVec3::new(min_x, min_y, min_z),
        current_size: IVec3::new(size_x, size_y, size_z),
        current_state_hash,
        decision,
        message_or_dimension,
    })
}

fn decode_delta_op(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 30 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "DeltaOp header",
        });
    }
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let seq_id = u64::from_le_bytes(buf[*offset + 8..*offset + 16].try_into().unwrap());
    let palette_revision = u32::from_le_bytes(buf[*offset + 16..*offset + 20].try_into().unwrap());
    let count = u16::from_le_bytes(buf[*offset + 20..*offset + 22].try_into().unwrap()) as usize;
    let batch_xxhash64 = u64::from_le_bytes(buf[*offset + 22..*offset + 30].try_into().unwrap());
    *offset += 30;

    if count > MAX_DELTA_BATCH_CHANGES {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Delta Changes per Batch",
            limit: MAX_DELTA_BATCH_CHANGES,
            actual: count,
        });
    }

    let entry_size = 14; // 3 * 4 + 2
    if *offset + count * entry_size > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "DeltaOp entries",
        });
    }

    let mut changes = Vec::with_capacity(count);
    for _ in 0..count {
        let rx = i32::from_le_bytes(buf[*offset..*offset + 4].try_into().unwrap());
        let ry = i32::from_le_bytes(buf[*offset + 4..*offset + 8].try_into().unwrap());
        let rz = i32::from_le_bytes(buf[*offset + 8..*offset + 12].try_into().unwrap());
        let state_id = u16::from_le_bytes(buf[*offset + 12..*offset + 14].try_into().unwrap());
        *offset += 14;
        changes.push(DeltaChangeEntry {
            rel_pos: IVec3::new(rx, ry, rz),
            state_id,
        });
    }

    let computed = compute_delta_batch_hash(&changes);
    if computed != batch_xxhash64 {
        return Err(ProtocolError::ChecksumMismatch {
            field: "DeltaOp batch entries",
            expected: batch_xxhash64,
            actual: computed,
        });
    }

    Ok(Packet::DeltaOp {
        epoch,
        seq_id,
        palette_revision,
        count: count as u16,
        batch_xxhash64,
        changes,
    })
}

fn decode_palette_update(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 26 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "PaletteUpdate header",
        });
    }
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let seq_id = u64::from_le_bytes(buf[*offset + 8..*offset + 16].try_into().unwrap());
    let base_revision = u32::from_le_bytes(buf[*offset + 16..*offset + 20].try_into().unwrap());
    let new_revision = u32::from_le_bytes(buf[*offset + 20..*offset + 24].try_into().unwrap());
    let count = u16::from_le_bytes(buf[*offset + 24..*offset + 26].try_into().unwrap()) as usize;
    *offset += 26;

    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if *offset + 2 > buf.len() {
            return Err(ProtocolError::TruncatedPayload {
                field: "PaletteUpdate state_id",
            });
        }
        let id = u16::from_le_bytes(buf[*offset..*offset + 2].try_into().unwrap());
        *offset += 2;
        let s = read_str(buf, offset, "PaletteUpdate state string")?.to_string();
        entries.push((id, s));
    }

    Ok(Packet::PaletteUpdate {
        epoch,
        seq_id,
        base_revision,
        new_revision,
        entries,
    })
}

fn decode_cancel(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 25 > buf.len() {
        return Err(ProtocolError::TruncatedPayload { field: "Cancel" });
    }
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let mut snapshot_uuid = [0u8; 16];
    snapshot_uuid.copy_from_slice(&buf[*offset + 8..*offset + 24]);
    let r_byte = buf[*offset + 24];
    let reason = CancelReason::from_u8(r_byte).ok_or(ProtocolError::InvalidEnum {
        field: "CancelReason",
        val: r_byte as u32,
    })?;
    *offset += 25;

    Ok(Packet::Cancel {
        epoch,
        snapshot_uuid,
        reason,
    })
}

fn decode_resync_request(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 25 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "ResyncRequest",
        });
    }
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let missing_from_seq = u64::from_le_bytes(buf[*offset + 8..*offset + 16].try_into().unwrap());
    let missing_to_seq = u64::from_le_bytes(buf[*offset + 16..*offset + 24].try_into().unwrap());
    let r_byte = buf[*offset + 24];
    let reason = ResyncReason::from_u8(r_byte).ok_or(ProtocolError::InvalidEnum {
        field: "ResyncReason",
        val: r_byte as u32,
    })?;
    *offset += 25;

    Ok(Packet::ResyncRequest {
        epoch,
        missing_from_seq,
        missing_to_seq,
        reason,
    })
}

fn decode_ping(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 8 > buf.len() {
        return Err(ProtocolError::TruncatedPayload { field: "Ping" });
    }
    let timestamp = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    Ok(Packet::Ping { timestamp })
}

fn decode_pong(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 8 > buf.len() {
        return Err(ProtocolError::TruncatedPayload { field: "Pong" });
    }
    let timestamp = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    Ok(Packet::Pong { timestamp })
}

fn decode_error(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 10 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "Error fixed fields",
        });
    }
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let code_val = u16::from_le_bytes(buf[*offset + 8..*offset + 10].try_into().unwrap());
    let error_code = ErrorCode::from_u16(code_val).ok_or(ProtocolError::InvalidEnum {
        field: "ErrorCode",
        val: code_val as u32,
    })?;
    *offset += 10;
    let message = read_str(buf, offset, "Error message")?.to_string();

    Ok(Packet::Error {
        epoch,
        error_code,
        message,
    })
}

fn decode_delta_entity(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 32 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "DeltaEntity header",
        });
    }
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let seq_id = u64::from_le_bytes(buf[*offset + 8..*offset + 16].try_into().unwrap());
    let rx = u32::from_le_bytes(buf[*offset + 16..*offset + 20].try_into().unwrap()) as i32;
    let ry = u32::from_le_bytes(buf[*offset + 20..*offset + 24].try_into().unwrap()) as i32;
    let rz = u32::from_le_bytes(buf[*offset + 24..*offset + 28].try_into().unwrap()) as i32;
    let payload_len =
        u32::from_le_bytes(buf[*offset + 28..*offset + 32].try_into().unwrap()) as usize;
    *offset += 32;

    let entity = if payload_len > 0 {
        if *offset + payload_len > buf.len() {
            return Err(ProtocolError::TruncatedPayload {
                field: "DeltaEntity payload",
            });
        }
        let payload_slice = &buf[*offset..*offset + payload_len];
        decode_entity_payload(payload_slice)
    } else {
        None
    };

    Ok(Packet::DeltaEntity {
        epoch,
        seq_id,
        rel_pos: IVec3::new(rx, ry, rz),
        entity,
    })
}
