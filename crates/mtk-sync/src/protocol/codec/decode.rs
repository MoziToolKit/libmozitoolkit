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

#[inline]
pub(crate) fn read_array<const N: usize>(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<[u8; N], ProtocolError> {
    let end = offset
        .checked_add(N)
        .ok_or(ProtocolError::TruncatedPayload { field })?;
    let slice = buf
        .get(*offset..end)
        .ok_or(ProtocolError::TruncatedPayload { field })?;
    *offset = end;
    slice
        .try_into()
        .map_err(|_| ProtocolError::TruncatedPayload { field })
}

#[inline]
pub(crate) fn read_slice<'a>(
    buf: &'a [u8],
    offset: &mut usize,
    len: usize,
    field: &'static str,
) -> Result<&'a [u8], ProtocolError> {
    let end = offset
        .checked_add(len)
        .ok_or(ProtocolError::TruncatedPayload { field })?;
    let slice = buf
        .get(*offset..end)
        .ok_or(ProtocolError::TruncatedPayload { field })?;
    *offset = end;
    Ok(slice)
}

#[inline]
pub(crate) fn read_u8(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<u8, ProtocolError> {
    let val = buf
        .get(*offset)
        .copied()
        .ok_or(ProtocolError::TruncatedPayload { field })?;
    *offset += 1;
    Ok(val)
}

#[inline]
pub(crate) fn read_u16_le(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<u16, ProtocolError> {
    read_array::<2>(buf, offset, field).map(u16::from_le_bytes)
}

#[inline]
pub(crate) fn read_i32_le(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<i32, ProtocolError> {
    read_array::<4>(buf, offset, field).map(i32::from_le_bytes)
}

#[inline]
pub(crate) fn read_u32_le(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<u32, ProtocolError> {
    read_array::<4>(buf, offset, field).map(u32::from_le_bytes)
}

#[inline]
pub(crate) fn read_f32_le(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<f32, ProtocolError> {
    read_array::<4>(buf, offset, field).map(f32::from_le_bytes)
}

#[inline]
pub(crate) fn read_f64_le(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<f64, ProtocolError> {
    read_array::<8>(buf, offset, field).map(f64::from_le_bytes)
}

#[inline]
pub(crate) fn read_u64_le(
    buf: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<u64, ProtocolError> {
    read_array::<8>(buf, offset, field).map(u64::from_le_bytes)
}

/// Helper to read a little-endian u16-length prefixed UTF-8 string.
pub(crate) fn read_str<'a>(
    buf: &'a [u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<&'a str, ProtocolError> {
    let len = read_u16_le(buf, offset, field)? as usize;
    let slice = read_slice(buf, offset, len, field)?;
    std::str::from_utf8(slice).map_err(|e| ProtocolError::Utf8Error {
        field,
        detail: e.to_string(),
    })
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
        Opcode::BaselineVisualEntities => decode_baseline_visual_entities(buf, &mut offset),
        Opcode::DeltaVisualEntity => decode_delta_visual_entity(buf, &mut offset),
    }
}

fn decode_hello(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 76 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "Hello fixed fields",
        });
    }
    let min_version = read_u16_le(buf, offset, "Hello min_version")?;
    let max_version = read_u16_le(buf, offset, "Hello max_version")?;
    let capabilities = read_u32_le(buf, offset, "Hello capabilities")?;
    let flags = read_u32_le(buf, offset, "Hello flags")?;
    let cached_world_uuid = read_array::<16>(buf, offset, "Hello cached_world_uuid")?;
    let cached_epoch = read_u64_le(buf, offset, "Hello cached_epoch")?;
    let cached_seq = read_u64_le(buf, offset, "Hello cached_seq")?;
    let c_min_x = read_i32_le(buf, offset, "Hello cached_min.x")?;
    let c_min_y = read_i32_le(buf, offset, "Hello cached_min.y")?;
    let c_min_z = read_i32_le(buf, offset, "Hello cached_min.z")?;
    let c_size_x = read_i32_le(buf, offset, "Hello cached_size.x")?;
    let c_size_y = read_i32_le(buf, offset, "Hello cached_size.y")?;
    let c_size_z = read_i32_le(buf, offset, "Hello cached_size.z")?;
    let cached_state_hash = read_u64_le(buf, offset, "Hello cached_state_hash")?;

    let auth_token = read_str(buf, offset, "Hello auth_token")?.to_string();
    let cached_dimension = read_str(buf, offset, "Hello cached_dimension")?.to_string();

    let mut subscribed_visual_entities = Vec::new();
    if (capabilities & CAP_SUPPORTS_VISUAL_ENTITIES) != 0 && *offset + 2 <= buf.len() {
        let count = read_u16_le(buf, offset, "Hello subscribed count")? as usize;
        for _ in 0..count {
            let id = read_str(buf, offset, "Hello subscribed entity id")?.to_string();
            subscribed_visual_entities.push(id);
        }
    }

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
        subscribed_visual_entities,
    })
}

fn decode_welcome(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 67 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "Welcome fixed fields",
        });
    }
    let negotiated_version = read_u16_le(buf, offset, "Welcome negotiated_version")?;
    let server_uuid = read_array::<16>(buf, offset, "Welcome server_uuid")?;
    let epoch = read_u64_le(buf, offset, "Welcome epoch")?;
    let server_seq = read_u64_le(buf, offset, "Welcome server_seq")?;
    let min_x = read_i32_le(buf, offset, "Welcome min_x")?;
    let min_y = read_i32_le(buf, offset, "Welcome min_y")?;
    let min_z = read_i32_le(buf, offset, "Welcome min_z")?;
    let size_x = read_i32_le(buf, offset, "Welcome size_x")?;
    let size_y = read_i32_le(buf, offset, "Welcome size_y")?;
    let size_z = read_i32_le(buf, offset, "Welcome size_z")?;
    let current_state_hash = read_u64_le(buf, offset, "Welcome current_state_hash")?;
    let dec_byte = read_u8(buf, offset, "Welcome decision")?;
    let decision = SyncDecision::from_u8(dec_byte).ok_or(ProtocolError::InvalidEnum {
        field: "SyncDecision",
        val: dec_byte as u32,
    })?;

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
    let epoch = read_u64_le(buf, offset, "DeltaOp epoch")?;
    let seq_id = read_u64_le(buf, offset, "DeltaOp seq_id")?;
    let palette_revision = read_u32_le(buf, offset, "DeltaOp palette_revision")?;
    let count = read_u16_le(buf, offset, "DeltaOp count")? as usize;
    let batch_xxhash64 = read_u64_le(buf, offset, "DeltaOp batch_xxhash64")?;

    if count > MAX_DELTA_BATCH_CHANGES {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Delta Changes per Batch",
            limit: MAX_DELTA_BATCH_CHANGES,
            actual: count,
        });
    }

    let entry_size = 14; // 3 * 4 + 2
    let entries_len =
        count
            .checked_mul(entry_size)
            .ok_or(ProtocolError::ResourceLimitExceeded {
                resource: "Delta Changes per Batch",
                limit: MAX_DELTA_BATCH_CHANGES,
                actual: count,
            })?;
    if *offset + entries_len > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "DeltaOp entries",
        });
    }

    let mut changes = Vec::with_capacity(count);
    for _ in 0..count {
        let rx = read_i32_le(buf, offset, "DeltaOp entry rx")?;
        let ry = read_i32_le(buf, offset, "DeltaOp entry ry")?;
        let rz = read_i32_le(buf, offset, "DeltaOp entry rz")?;
        let state_id = read_u16_le(buf, offset, "DeltaOp entry state_id")?;
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
    let epoch = read_u64_le(buf, offset, "PaletteUpdate epoch")?;
    let seq_id = read_u64_le(buf, offset, "PaletteUpdate seq_id")?;
    let base_revision = read_u32_le(buf, offset, "PaletteUpdate base_revision")?;
    let new_revision = read_u32_le(buf, offset, "PaletteUpdate new_revision")?;
    let count = read_u16_le(buf, offset, "PaletteUpdate count")? as usize;

    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let id = read_u16_le(buf, offset, "PaletteUpdate state_id")?;
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
    let epoch = read_u64_le(buf, offset, "Cancel epoch")?;
    let snapshot_uuid = read_array::<16>(buf, offset, "Cancel snapshot_uuid")?;
    let r_byte = read_u8(buf, offset, "Cancel reason")?;
    let reason = CancelReason::from_u8(r_byte).ok_or(ProtocolError::InvalidEnum {
        field: "CancelReason",
        val: r_byte as u32,
    })?;

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
    let epoch = read_u64_le(buf, offset, "ResyncRequest epoch")?;
    let missing_from_seq = read_u64_le(buf, offset, "ResyncRequest missing_from_seq")?;
    let missing_to_seq = read_u64_le(buf, offset, "ResyncRequest missing_to_seq")?;
    let r_byte = read_u8(buf, offset, "ResyncRequest reason")?;
    let reason = ResyncReason::from_u8(r_byte).ok_or(ProtocolError::InvalidEnum {
        field: "ResyncReason",
        val: r_byte as u32,
    })?;

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
    let timestamp = read_u64_le(buf, offset, "Ping timestamp")?;
    Ok(Packet::Ping { timestamp })
}

fn decode_pong(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 8 > buf.len() {
        return Err(ProtocolError::TruncatedPayload { field: "Pong" });
    }
    let timestamp = read_u64_le(buf, offset, "Pong timestamp")?;
    Ok(Packet::Pong { timestamp })
}

fn decode_error(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 10 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "Error fixed fields",
        });
    }
    let epoch = read_u64_le(buf, offset, "Error epoch")?;
    let code_val = read_u16_le(buf, offset, "Error code_val")?;
    let error_code = ErrorCode::from_u16(code_val).ok_or(ProtocolError::InvalidEnum {
        field: "ErrorCode",
        val: code_val as u32,
    })?;
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
    let epoch = read_u64_le(buf, offset, "DeltaEntity epoch")?;
    let seq_id = read_u64_le(buf, offset, "DeltaEntity seq_id")?;
    let rx = read_i32_le(buf, offset, "DeltaEntity rx")?;
    let ry = read_i32_le(buf, offset, "DeltaEntity ry")?;
    let rz = read_i32_le(buf, offset, "DeltaEntity rz")?;
    let payload_len = read_u32_le(buf, offset, "DeltaEntity payload_len")? as usize;

    let entity = if payload_len > 0 {
        let payload_slice = read_slice(buf, offset, payload_len, "DeltaEntity payload")?;
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

fn read_visual_entity(
    buf: &[u8],
    offset: &mut usize,
) -> Result<mtk_model::EntityInstance, ProtocolError> {
    let most = read_u64_le(buf, offset, "VisualEntity uuid_most")?;
    let least = read_u64_le(buf, offset, "VisualEntity uuid_least")?;
    let uuid_str = crate::protocol::format_uuid(most, least);

    let x = read_f64_le(buf, offset, "VisualEntity pos.x")?;
    let y = read_f64_le(buf, offset, "VisualEntity pos.y")?;
    let z = read_f64_le(buf, offset, "VisualEntity pos.z")?;

    let pitch = read_f32_le(buf, offset, "VisualEntity rot.pitch")?;
    let yaw = read_f32_le(buf, offset, "VisualEntity rot.yaw")?;
    let roll = read_f32_le(buf, offset, "VisualEntity rot.roll")?;

    let sx = read_f32_le(buf, offset, "VisualEntity scale.x")?;
    let sy = read_f32_le(buf, offset, "VisualEntity scale.y")?;
    let sz = read_f32_le(buf, offset, "VisualEntity scale.z")?;

    let facing_byte = read_u8(buf, offset, "VisualEntity facing")?;
    let facing = crate::protocol::facing_byte_to_direction(facing_byte);

    let id = read_str(buf, offset, "VisualEntity id")?.to_string();

    let payload_len = read_u32_le(buf, offset, "VisualEntity payload_len")? as usize;
    let payload = read_slice(buf, offset, payload_len, "VisualEntity payload")?;

    let kind: mtk_model::EntityKindData = if !payload.is_empty() {
        serde_json::from_slice(payload).unwrap_or_else(|_| mtk_model::EntityKindData::Generic {
            custom_id: id.clone(),
            payload: payload.to_vec(),
        })
    } else {
        mtk_model::EntityKindData::Generic {
            custom_id: id.clone(),
            payload: Vec::new(),
        }
    };

    Ok(mtk_model::EntityInstance {
        uuid: Some(uuid_str),
        id,
        custom_name: None,
        position: glam::DVec3::new(x, y, z),
        facing,
        rotation: glam::Vec3::new(pitch, yaw, roll),
        scale: glam::Vec3::new(sx, sy, sz),
        kind,
    })
}

fn decode_baseline_visual_entities(
    buf: &[u8],
    offset: &mut usize,
) -> Result<Packet, ProtocolError> {
    let epoch = read_u64_le(buf, offset, "BaselineVisualEntities epoch")?;
    let snapshot_seq = read_u64_le(buf, offset, "BaselineVisualEntities snapshot_seq")?;
    let entities_hash = read_u64_le(buf, offset, "BaselineVisualEntities entities_hash")?;
    let count = read_u32_le(buf, offset, "BaselineVisualEntities count")? as usize;

    let mut entities = Vec::with_capacity(count);
    for _ in 0..count {
        entities.push(read_visual_entity(buf, offset)?);
    }

    Ok(Packet::BaselineVisualEntities {
        epoch,
        snapshot_seq,
        entities_hash,
        entities,
    })
}

fn decode_delta_visual_entity(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    let epoch = read_u64_le(buf, offset, "DeltaVisualEntity epoch")?;
    let seq_id = read_u64_le(buf, offset, "DeltaVisualEntity seq_id")?;
    let action = read_u8(buf, offset, "DeltaVisualEntity action")?;

    let delta = if action == DELTA_VISUAL_ACTION_REMOVE {
        let most = read_u64_le(buf, offset, "DeltaVisualEntity uuid_most")?;
        let least = read_u64_le(buf, offset, "DeltaVisualEntity uuid_least")?;
        let uuid = crate::protocol::format_uuid(most, least);
        VisualEntityDelta::Remove { uuid }
    } else if action == DELTA_VISUAL_ACTION_UPSERT {
        let entity = read_visual_entity(buf, offset)?;
        VisualEntityDelta::Upsert(entity)
    } else {
        return Err(ProtocolError::InvalidEnum {
            field: "DeltaVisualEntity action",
            val: action as u32,
        });
    };

    Ok(Packet::DeltaVisualEntity {
        epoch,
        seq_id,
        delta,
    })
}
