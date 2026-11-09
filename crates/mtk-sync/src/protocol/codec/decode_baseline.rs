//! # Baseline Packet Decoders (RFC v3.1)
//!
//! Binary decoders for full snapshot streaming: BaselineBegin, BaselineChunk,
//! BaselineEnd, BaselineAck, and BaselineEntities.

use glam::IVec3;
use xxhash_rust::xxh64::Xxh64;

use super::decode::{decode_entity_payload, read_str};
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
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let mut snapshot_uuid = [0u8; 16];
    snapshot_uuid.copy_from_slice(&buf[*offset + 8..*offset + 24]);
    let snapshot_seq = u64::from_le_bytes(buf[*offset + 24..*offset + 32].try_into().unwrap());
    let min_x = i32::from_le_bytes(buf[*offset + 32..*offset + 36].try_into().unwrap());
    let min_y = i32::from_le_bytes(buf[*offset + 36..*offset + 40].try_into().unwrap());
    let min_z = i32::from_le_bytes(buf[*offset + 40..*offset + 44].try_into().unwrap());
    let size_x = i32::from_le_bytes(buf[*offset + 44..*offset + 48].try_into().unwrap());
    let size_y = i32::from_le_bytes(buf[*offset + 48..*offset + 52].try_into().unwrap());
    let size_z = i32::from_le_bytes(buf[*offset + 52..*offset + 56].try_into().unwrap());
    let state_hash = u64::from_le_bytes(buf[*offset + 56..*offset + 64].try_into().unwrap());
    let palette_revision = u32::from_le_bytes(buf[*offset + 64..*offset + 68].try_into().unwrap());
    let total_chunks = u32::from_le_bytes(buf[*offset + 68..*offset + 72].try_into().unwrap());
    let total_encoded_bytes =
        u64::from_le_bytes(buf[*offset + 72..*offset + 80].try_into().unwrap());
    let total_decoded_bytes =
        u64::from_le_bytes(buf[*offset + 80..*offset + 88].try_into().unwrap());
    let compression_mode = buf[*offset + 88];
    let palette_bits_per_block = buf[*offset + 89];
    let biome_bits_per_quart = buf[*offset + 90];
    *offset += 91;

    if total_decoded_bytes as usize > MAX_BASELINE_DECODED_BYTES {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Total Decoded Voxel Bytes",
            limit: MAX_BASELINE_DECODED_BYTES,
            actual: total_decoded_bytes as usize,
        });
    }

    let dimension = read_str(buf, offset, "BaselineBegin dimension")?.to_string();

    if *offset + 4 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "block_palette length",
        });
    }
    let block_palette_len =
        u32::from_le_bytes(buf[*offset..*offset + 4].try_into().unwrap()) as usize;
    *offset += 4;
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

    if *offset + 2 > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "biome_palette length",
        });
    }
    let biome_palette_len =
        u16::from_le_bytes(buf[*offset..*offset + 2].try_into().unwrap()) as usize;
    *offset += 2;
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
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let mut snapshot_uuid = [0u8; 16];
    snapshot_uuid.copy_from_slice(&buf[*offset + 8..*offset + 24]);
    let chunk_index = u32::from_le_bytes(buf[*offset + 24..*offset + 28].try_into().unwrap());
    let total_chunks = u32::from_le_bytes(buf[*offset + 28..*offset + 32].try_into().unwrap());
    let first_voxel_index = u64::from_le_bytes(buf[*offset + 32..*offset + 40].try_into().unwrap());
    let voxel_count = u32::from_le_bytes(buf[*offset + 40..*offset + 44].try_into().unwrap());
    let payload_len =
        u32::from_le_bytes(buf[*offset + 44..*offset + 48].try_into().unwrap()) as usize;
    let decoded_target_bytes =
        u32::from_le_bytes(buf[*offset + 48..*offset + 52].try_into().unwrap());
    let chunk_xxhash64 = u64::from_le_bytes(buf[*offset + 52..*offset + 60].try_into().unwrap());
    *offset += 60;

    if *offset + payload_len > buf.len() {
        return Err(ProtocolError::TruncatedPayload {
            field: "BaselineChunk payload",
        });
    }
    let payload = buf[*offset..*offset + payload_len].to_vec();

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
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let mut snapshot_uuid = [0u8; 16];
    snapshot_uuid.copy_from_slice(&buf[*offset + 8..*offset + 24]);
    let snapshot_seq = u64::from_le_bytes(buf[*offset + 24..*offset + 32].try_into().unwrap());
    let total_chunks_sent = u32::from_le_bytes(buf[*offset + 32..*offset + 36].try_into().unwrap());
    let final_state_hash = u64::from_le_bytes(buf[*offset + 36..*offset + 44].try_into().unwrap());
    let status = buf[*offset + 44];
    *offset += 45;

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
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let mut snapshot_uuid = [0u8; 16];
    snapshot_uuid.copy_from_slice(&buf[*offset + 8..*offset + 24]);
    let committed_seq = u64::from_le_bytes(buf[*offset + 24..*offset + 32].try_into().unwrap());
    let res_byte = buf[*offset + 32];
    let result_code = BaselineAckResult::from_u8(res_byte).ok_or(ProtocolError::InvalidEnum {
        field: "BaselineAckResult",
        val: res_byte as u32,
    })?;
    *offset += 33;

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
    let epoch = u64::from_le_bytes(buf[*offset..*offset + 8].try_into().unwrap());
    let snapshot_uuid: [u8; 16] = buf[*offset + 8..*offset + 24].try_into().unwrap();
    let count = u32::from_le_bytes(buf[*offset + 24..*offset + 28].try_into().unwrap()) as usize;
    *offset += 28;

    let mut entities = Vec::with_capacity(count);
    for _ in 0..count {
        if *offset + 16 > buf.len() {
            return Err(ProtocolError::TruncatedPayload {
                field: "BaselineEntities entry coords & len",
            });
        }
        let rx = u32::from_le_bytes(buf[*offset..*offset + 4].try_into().unwrap()) as i32;
        let ry = u32::from_le_bytes(buf[*offset + 4..*offset + 8].try_into().unwrap()) as i32;
        let rz = u32::from_le_bytes(buf[*offset + 8..*offset + 12].try_into().unwrap()) as i32;
        let payload_len =
            u32::from_le_bytes(buf[*offset + 12..*offset + 16].try_into().unwrap()) as usize;
        *offset += 16;
        if *offset + payload_len > buf.len() {
            return Err(ProtocolError::TruncatedPayload {
                field: "BaselineEntities entry payload",
            });
        }
        let payload_slice = &buf[*offset..*offset + payload_len];
        *offset += payload_len;

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
