//! # Baseline Packet Decoders (RFC v3.1)
//!
//! Binary decoders for full snapshot streaming: BaselineBegin, BaselineChunk,
//! BaselineEnd, BaselineAck, and BaselineEntities.

use glam::IVec3;
use xxhash_rust::xxh64::Xxh64;

use super::decode::{
    decode_entity_payload, read_array, read_i32_le, read_slice, read_str, read_u16_le, read_u32_le,
    read_u64_le, read_u8,
};
use crate::protocol::constants::*;
use crate::protocol::error::ProtocolError;
use crate::protocol::packet::Packet;

pub(crate) fn decode_baseline_begin(
    buf: &[u8],
    offset: &mut usize,
) -> Result<Packet, ProtocolError> {
    if *offset + 91 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineBegin fixed fields",
        });
    }
    let epoch = read_u64_le(buf, offset, "BaselineBegin epoch")?;
    let snapshot_uuid = read_array::<16>(buf, offset, "BaselineBegin snapshot_uuid")?;
    let snapshot_seq = read_u64_le(buf, offset, "BaselineBegin snapshot_seq")?;
    let min_x = read_i32_le(buf, offset, "BaselineBegin min_pos.x")?;
    let min_y = read_i32_le(buf, offset, "BaselineBegin min_pos.y")?;
    let min_z = read_i32_le(buf, offset, "BaselineBegin min_pos.z")?;
    let size_x = read_i32_le(buf, offset, "BaselineBegin size.x")?;
    let size_y = read_i32_le(buf, offset, "BaselineBegin size.y")?;
    let size_z = read_i32_le(buf, offset, "BaselineBegin size.z")?;
    let state_hash = read_u64_le(buf, offset, "BaselineBegin state_hash")?;
    let palette_revision = read_u32_le(buf, offset, "BaselineBegin palette_revision")?;
    let total_chunks = read_u32_le(buf, offset, "BaselineBegin total_chunks")?;
    let total_encoded_bytes = read_u64_le(buf, offset, "BaselineBegin total_encoded_bytes")?;
    let total_decoded_bytes = read_u64_le(buf, offset, "BaselineBegin total_decoded_bytes")?;
    let compression_mode = read_u8(buf, offset, "BaselineBegin compression_mode")?;
    let palette_bits_per_block = read_u8(buf, offset, "BaselineBegin palette_bits_per_block")?;
    let biome_bits_per_quart = read_u8(buf, offset, "BaselineBegin biome_bits_per_quart")?;

    if total_decoded_bytes as usize > MAX_BASELINE_DECODED_BYTES {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Total Decoded Voxel Bytes",
            limit: MAX_BASELINE_DECODED_BYTES,
            actual: total_decoded_bytes as usize,
        });
    }

    let dimension = read_str(buf, offset, "BaselineBegin dimension")?.to_string();

    let block_palette_len = read_u32_le(buf, offset, "block_palette length")? as usize;
    if block_palette_len > MAX_PALETTE_ENTRIES {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Block Palette Entries",
            limit: MAX_PALETTE_ENTRIES,
            actual: block_palette_len,
        });
    }

    let mut block_palette = Vec::with_capacity(block_palette_len);
    for _ in 0..block_palette_len {
        block_palette.push(read_str(buf, offset, "block_palette entry")?.to_string());
    }

    let biome_palette_len = read_u16_le(buf, offset, "biome_palette length")? as usize;
    let mut biome_palette = Vec::with_capacity(biome_palette_len);
    for _ in 0..biome_palette_len {
        biome_palette.push(read_str(buf, offset, "biome_palette entry")?.to_string());
    }

    Ok(Packet::BaselineBegin {
        epoch,
        snapshot_uuid,
        snapshot_seq,
        min_pos: IVec3::new(min_x, min_y, min_z),
        size: IVec3::new(size_x, size_y, size_z),
        state_hash,
        palette_revision,
        total_chunks,
        total_encoded_bytes,
        total_decoded_bytes,
        compression_mode,
        palette_bits_per_block,
        biome_bits_per_quart,
        dimension,
        block_palette,
        biome_palette,
    })
}

pub(crate) fn decode_baseline_chunk(
    buf: &[u8],
    offset: &mut usize,
) -> Result<Packet, ProtocolError> {
    if *offset + 60 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineChunk header",
        });
    }
    let epoch = read_u64_le(buf, offset, "BaselineChunk epoch")?;
    let snapshot_uuid = read_array::<16>(buf, offset, "BaselineChunk snapshot_uuid")?;
    let chunk_index = read_u32_le(buf, offset, "BaselineChunk chunk_index")?;
    let total_chunks = read_u32_le(buf, offset, "BaselineChunk total_chunks")?;
    let first_voxel_index = read_u64_le(buf, offset, "BaselineChunk first_voxel_index")?;
    let voxel_count = read_u32_le(buf, offset, "BaselineChunk voxel_count")?;
    let payload_len = read_u32_le(buf, offset, "BaselineChunk payload_len")? as usize;
    let decoded_target_bytes = read_u32_le(buf, offset, "BaselineChunk decoded_target_bytes")?;
    let chunk_xxhash64 = read_u64_le(buf, offset, "BaselineChunk chunk_xxhash64")?;

    let payload = read_slice(buf, offset, payload_len, "BaselineChunk payload")?.to_vec();

    // Verify chunk hash
    let mut hasher = Xxh64::new(0);
    hasher.update(&payload);
    let computed = hasher.digest();
    if computed != chunk_xxhash64 {
        return Err(ProtocolError::ChecksumMismatch {
            field: "BaselineChunk payload",
            expected: chunk_xxhash64,
            actual: computed,
        });
    }

    Ok(Packet::BaselineChunk {
        epoch,
        snapshot_uuid,
        chunk_index,
        total_chunks,
        first_voxel_index,
        voxel_count,
        payload_len: payload_len as u32,
        decoded_target_bytes,
        chunk_xxhash64,
        payload,
    })
}

pub(crate) fn decode_baseline_end(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 45 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineEnd",
        });
    }
    let epoch = read_u64_le(buf, offset, "BaselineEnd epoch")?;
    let snapshot_uuid = read_array::<16>(buf, offset, "BaselineEnd snapshot_uuid")?;
    let snapshot_seq = read_u64_le(buf, offset, "BaselineEnd snapshot_seq")?;
    let total_chunks_sent = read_u32_le(buf, offset, "BaselineEnd total_chunks_sent")?;
    let final_state_hash = read_u64_le(buf, offset, "BaselineEnd final_state_hash")?;
    let status = read_u8(buf, offset, "BaselineEnd status")?;

    Ok(Packet::BaselineEnd {
        epoch,
        snapshot_uuid,
        snapshot_seq,
        total_chunks_sent,
        final_state_hash,
        status,
    })
}

pub(crate) fn decode_baseline_ack(buf: &[u8], offset: &mut usize) -> Result<Packet, ProtocolError> {
    if *offset + 33 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineAck",
        });
    }
    let epoch = read_u64_le(buf, offset, "BaselineAck epoch")?;
    let snapshot_uuid = read_array::<16>(buf, offset, "BaselineAck snapshot_uuid")?;
    let committed_seq = read_u64_le(buf, offset, "BaselineAck committed_seq")?;
    let res_byte = read_u8(buf, offset, "BaselineAck result_code")?;
    let result_code = BaselineAckResult::from_u8(res_byte).ok_or(ProtocolError::InvalidEnum {
        field: "BaselineAckResult",
        val: res_byte as u32,
    })?;

    Ok(Packet::BaselineAck {
        epoch,
        snapshot_uuid,
        committed_seq,
        result_code,
    })
}

pub(crate) fn decode_baseline_entities(
    buf: &[u8],
    offset: &mut usize,
) -> Result<Packet, ProtocolError> {
    if *offset + 28 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineEntities header",
        });
    }
    let epoch = read_u64_le(buf, offset, "BaselineEntities epoch")?;
    let snapshot_uuid = read_array::<16>(buf, offset, "BaselineEntities snapshot_uuid")?;
    let count = read_u32_le(buf, offset, "BaselineEntities count")? as usize;
    let max_possible = (buf.len().saturating_sub(*offset)) / 16;
    let mut entities = Vec::with_capacity(count.min(max_possible));
    for _ in 0..count {
        if *offset + 16 > buf.len() {
            return Err(ProtocolError::TruncatedPayload {
                field: "BaselineEntities entry coords & len",
            });
        }
        let rx = read_i32_le(buf, offset, "BaselineEntities rx")?;
        let ry = read_i32_le(buf, offset, "BaselineEntities ry")?;
        let rz = read_i32_le(buf, offset, "BaselineEntities rz")?;
        let payload_len = read_u32_le(buf, offset, "BaselineEntities payload_len")? as usize;
        let payload_slice = read_slice(buf, offset, payload_len, "BaselineEntities entry payload")?;

        if let Some(ed) = decode_entity_payload(payload_slice) {
            entities.push((IVec3::new(rx, ry, rz), ed));
        }
    }

    Ok(Packet::BaselineEntities {
        epoch,
        snapshot_uuid,
        entities,
    })
}
