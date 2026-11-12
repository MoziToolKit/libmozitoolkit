//! # Visual Entities Decoders (RFC v3.2)
//!
//! Binary decoders for visual entities in baseline and incremental delta streams.

use super::decode::{
    read_f32_le, read_f64_le, read_slice, read_str, read_u32_le, read_u64_le, read_u8,
};
use crate::protocol::constants::*;
use crate::protocol::error::ProtocolError;
use crate::protocol::packet::{Packet, VisualEntityDelta};

/// Reads a single VisualEntity from the buffer at `*offset` and advances it.
pub fn read_visual_entity(
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

pub(crate) fn decode_baseline_visual_entities(
    buf: &[u8],
    offset: &mut usize,
) -> Result<Packet, ProtocolError> {
    let epoch = read_u64_le(buf, offset, "BaselineVisualEntities epoch")?;
    let snapshot_seq = read_u64_le(buf, offset, "BaselineVisualEntities snapshot_seq")?;
    let entities_hash = read_u64_le(buf, offset, "BaselineVisualEntities entities_hash")?;
    let count = read_u32_le(buf, offset, "BaselineVisualEntities count")? as usize;

    if count > MAX_VISUAL_ENTITIES {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Visual Entities Count",
            limit: MAX_VISUAL_ENTITIES,
            actual: count,
        });
    }

    let min_entry_size = 71;
    let entries_len =
        count
            .checked_mul(min_entry_size)
            .ok_or(ProtocolError::ResourceLimitExceeded {
                resource: "Visual Entities Count",
                limit: MAX_VISUAL_ENTITIES,
                actual: count,
            })?;
    if *offset + entries_len > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineVisualEntities entries",
        });
    }

    let max_possible = (buf.len().saturating_sub(*offset)) / min_entry_size;
    let mut entities = Vec::with_capacity(count.min(max_possible));
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

pub(crate) fn decode_delta_visual_entity(
    buf: &[u8],
    offset: &mut usize,
) -> Result<Packet, ProtocolError> {
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
