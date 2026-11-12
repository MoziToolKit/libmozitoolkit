//! # Protocol Binary Encoder (RFC v3.1)
//!
//! Encodes typed `Packet` instances into WebSocket outgoing binary payloads.

use crate::protocol::constants::*;
use crate::protocol::packet::*;

/// Helper to write a little-endian u16-length prefixed string.
fn write_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    buf.extend_from_slice(bytes);
}

#[inline]
fn write_uuid(buf: &mut Vec<u8>, uuid_str: &str) {
    let (most_sig, least_sig) = crate::protocol::parse_uuid_bits(uuid_str);
    buf.extend_from_slice(&most_sig.to_le_bytes());
    buf.extend_from_slice(&least_sig.to_le_bytes());
}

#[inline]
fn write_visual_entity(buf: &mut Vec<u8>, entity: &mtk_model::EntityInstance) {
    let uuid_str = entity.uuid.as_deref().unwrap_or("");
    write_uuid(buf, uuid_str);
    buf.extend_from_slice(&entity.position.x.to_le_bytes());
    buf.extend_from_slice(&entity.position.y.to_le_bytes());
    buf.extend_from_slice(&entity.position.z.to_le_bytes());
    buf.extend_from_slice(&entity.rotation.x.to_le_bytes());
    buf.extend_from_slice(&entity.rotation.y.to_le_bytes());
    buf.extend_from_slice(&entity.rotation.z.to_le_bytes());
    buf.extend_from_slice(&entity.scale.x.to_le_bytes());
    buf.extend_from_slice(&entity.scale.y.to_le_bytes());
    buf.extend_from_slice(&entity.scale.z.to_le_bytes());
    let facing = crate::protocol::direction_to_facing_byte(entity.facing);
    buf.push(facing);
    write_str(buf, &entity.id);
    let payload = match &entity.kind {
        mtk_model::EntityKindData::Generic { payload, .. } if !payload.is_empty() => {
            payload.clone()
        }
        _ => serde_json::to_vec(&entity.kind).unwrap_or_default(),
    };
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    if !payload.is_empty() {
        buf.extend_from_slice(&payload);
    }
}

/// Encodes any `Packet` into a complete WebSocket binary payload with standard 4-byte header.
pub fn encode_packet(packet: &Packet) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&PROTOCOL_MAGIC);
    buf.push(PROTOCOL_VERSION);

    match packet {
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
            subscribed_visual_entities,
        } => {
            buf.push(Opcode::Hello as u8);
            buf.extend_from_slice(&min_version.to_le_bytes());
            buf.extend_from_slice(&max_version.to_le_bytes());
            buf.extend_from_slice(&capabilities.to_le_bytes());
            buf.extend_from_slice(&flags.to_le_bytes());
            buf.extend_from_slice(cached_world_uuid);
            buf.extend_from_slice(&cached_epoch.to_le_bytes());
            buf.extend_from_slice(&cached_seq.to_le_bytes());
            buf.extend_from_slice(&cached_min.x.to_le_bytes());
            buf.extend_from_slice(&cached_min.y.to_le_bytes());
            buf.extend_from_slice(&cached_min.z.to_le_bytes());
            buf.extend_from_slice(&cached_size.x.to_le_bytes());
            buf.extend_from_slice(&cached_size.y.to_le_bytes());
            buf.extend_from_slice(&cached_size.z.to_le_bytes());
            buf.extend_from_slice(&cached_state_hash.to_le_bytes());
            write_str(&mut buf, auth_token);
            write_str(&mut buf, cached_dimension);
            if (*capabilities & CAP_SUPPORTS_VISUAL_ENTITIES) != 0 {
                buf.extend_from_slice(&(subscribed_visual_entities.len() as u16).to_le_bytes());
                for entity_id in subscribed_visual_entities {
                    write_str(&mut buf, entity_id);
                }
            }
        }

        Packet::Welcome {
            negotiated_version,
            server_uuid,
            epoch,
            server_seq,
            current_min,
            current_size,
            current_state_hash,
            decision,
            message_or_dimension,
        } => {
            buf.push(Opcode::Welcome as u8);
            buf.extend_from_slice(&negotiated_version.to_le_bytes());
            buf.extend_from_slice(server_uuid);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&server_seq.to_le_bytes());
            buf.extend_from_slice(&current_min.x.to_le_bytes());
            buf.extend_from_slice(&current_min.y.to_le_bytes());
            buf.extend_from_slice(&current_min.z.to_le_bytes());
            buf.extend_from_slice(&current_size.x.to_le_bytes());
            buf.extend_from_slice(&current_size.y.to_le_bytes());
            buf.extend_from_slice(&current_size.z.to_le_bytes());
            buf.extend_from_slice(&current_state_hash.to_le_bytes());
            buf.push(*decision as u8);
            write_str(&mut buf, message_or_dimension);
        }

        Packet::BaselineBegin {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            min_pos,
            size,
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
        } => {
            buf.push(Opcode::BaselineBegin as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(snapshot_uuid);
            buf.extend_from_slice(&snapshot_seq.to_le_bytes());
            buf.extend_from_slice(&min_pos.x.to_le_bytes());
            buf.extend_from_slice(&min_pos.y.to_le_bytes());
            buf.extend_from_slice(&min_pos.z.to_le_bytes());
            buf.extend_from_slice(&size.x.to_le_bytes());
            buf.extend_from_slice(&size.y.to_le_bytes());
            buf.extend_from_slice(&size.z.to_le_bytes());
            buf.extend_from_slice(&state_hash.to_le_bytes());
            buf.extend_from_slice(&palette_revision.to_le_bytes());
            buf.extend_from_slice(&total_chunks.to_le_bytes());
            buf.extend_from_slice(&total_encoded_bytes.to_le_bytes());
            buf.extend_from_slice(&total_decoded_bytes.to_le_bytes());
            buf.push(*compression_mode);
            buf.push(*palette_bits_per_block);
            buf.push(*biome_bits_per_quart);
            write_str(&mut buf, dimension);

            buf.extend_from_slice(&(block_palette.len() as u32).to_le_bytes());
            for entry in block_palette {
                write_str(&mut buf, entry);
            }

            buf.extend_from_slice(&(biome_palette.len() as u16).to_le_bytes());
            for entry in biome_palette {
                write_str(&mut buf, entry);
            }
        }

        Packet::BaselineChunk {
            epoch,
            snapshot_uuid,
            chunk_index,
            total_chunks,
            first_voxel_index,
            voxel_count,
            payload_len,
            decoded_target_bytes,
            chunk_xxhash64,
            payload,
        } => {
            buf.push(Opcode::BaselineChunk as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(snapshot_uuid);
            buf.extend_from_slice(&chunk_index.to_le_bytes());
            buf.extend_from_slice(&total_chunks.to_le_bytes());
            buf.extend_from_slice(&first_voxel_index.to_le_bytes());
            buf.extend_from_slice(&voxel_count.to_le_bytes());
            buf.extend_from_slice(&payload_len.to_le_bytes());
            buf.extend_from_slice(&decoded_target_bytes.to_le_bytes());
            buf.extend_from_slice(&chunk_xxhash64.to_le_bytes());
            buf.extend_from_slice(payload);
        }

        Packet::BaselineEnd {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            total_chunks_sent,
            final_state_hash,
            status,
        } => {
            buf.push(Opcode::BaselineEnd as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(snapshot_uuid);
            buf.extend_from_slice(&snapshot_seq.to_le_bytes());
            buf.extend_from_slice(&total_chunks_sent.to_le_bytes());
            buf.extend_from_slice(&final_state_hash.to_le_bytes());
            buf.push(*status);
        }

        Packet::BaselineAck {
            epoch,
            snapshot_uuid,
            committed_seq,
            result_code,
        } => {
            buf.push(Opcode::BaselineAck as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(snapshot_uuid);
            buf.extend_from_slice(&committed_seq.to_le_bytes());
            buf.push(*result_code as u8);
        }

        Packet::DeltaOp {
            epoch,
            seq_id,
            palette_revision,
            count,
            batch_xxhash64,
            changes,
        } => {
            buf.push(Opcode::DeltaOp as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&seq_id.to_le_bytes());
            buf.extend_from_slice(&palette_revision.to_le_bytes());
            buf.extend_from_slice(&count.to_le_bytes());
            buf.extend_from_slice(&batch_xxhash64.to_le_bytes());
            for entry in changes {
                buf.extend_from_slice(&entry.rel_pos.x.to_le_bytes());
                buf.extend_from_slice(&entry.rel_pos.y.to_le_bytes());
                buf.extend_from_slice(&entry.rel_pos.z.to_le_bytes());
                buf.extend_from_slice(&entry.state_id.to_le_bytes());
            }
        }

        Packet::PaletteUpdate {
            epoch,
            seq_id,
            base_revision,
            new_revision,
            entries,
        } => {
            buf.push(Opcode::PaletteUpdate as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&seq_id.to_le_bytes());
            buf.extend_from_slice(&base_revision.to_le_bytes());
            buf.extend_from_slice(&new_revision.to_le_bytes());
            buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            for (id, s) in entries {
                buf.extend_from_slice(&id.to_le_bytes());
                write_str(&mut buf, s);
            }
        }

        Packet::Cancel {
            epoch,
            snapshot_uuid,
            reason,
        } => {
            buf.push(Opcode::Cancel as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(snapshot_uuid);
            buf.push(*reason as u8);
        }

        Packet::ResyncRequest {
            epoch,
            missing_from_seq,
            missing_to_seq,
            reason,
        } => {
            buf.push(Opcode::ResyncRequest as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&missing_from_seq.to_le_bytes());
            buf.extend_from_slice(&missing_to_seq.to_le_bytes());
            buf.push(*reason as u8);
        }

        Packet::Ping { timestamp } => {
            buf.push(Opcode::Ping as u8);
            buf.extend_from_slice(&timestamp.to_le_bytes());
        }

        Packet::Pong { timestamp } => {
            buf.push(Opcode::Pong as u8);
            buf.extend_from_slice(&timestamp.to_le_bytes());
        }

        Packet::Error {
            epoch,
            error_code,
            message,
        } => {
            buf.push(Opcode::Error as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&(*error_code as u16).to_le_bytes());
            write_str(&mut buf, message);
        }

        Packet::BaselineEntities {
            epoch,
            snapshot_uuid,
            entities,
        } => {
            buf.push(Opcode::BaselineEntities as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(snapshot_uuid);
            buf.extend_from_slice(&(entities.len() as u32).to_le_bytes());
            for (pos, ed) in entities {
                buf.extend_from_slice(&(pos.x as u32).to_le_bytes());
                buf.extend_from_slice(&(pos.y as u32).to_le_bytes());
                buf.extend_from_slice(&(pos.z as u32).to_le_bytes());
                let ed_bytes = serde_json::to_vec(ed).unwrap_or_default();
                buf.extend_from_slice(&(ed_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&ed_bytes);
            }
        }

        Packet::DeltaEntity {
            epoch,
            seq_id,
            rel_pos,
            entity,
        } => {
            buf.push(Opcode::DeltaEntity as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&seq_id.to_le_bytes());
            buf.extend_from_slice(&(rel_pos.x as u32).to_le_bytes());
            buf.extend_from_slice(&(rel_pos.y as u32).to_le_bytes());
            buf.extend_from_slice(&(rel_pos.z as u32).to_le_bytes());
            if let Some(ed) = entity {
                let ed_bytes = serde_json::to_vec(ed).unwrap_or_default();
                buf.extend_from_slice(&(ed_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&ed_bytes);
            } else {
                buf.extend_from_slice(&0u32.to_le_bytes());
            }
        }

        Packet::BaselineVisualEntities {
            epoch,
            snapshot_seq,
            entities_hash,
            entities,
        } => {
            buf.push(Opcode::BaselineVisualEntities as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&snapshot_seq.to_le_bytes());
            buf.extend_from_slice(&entities_hash.to_le_bytes());
            buf.extend_from_slice(&(entities.len() as u32).to_le_bytes());
            for e in entities {
                write_visual_entity(&mut buf, e);
            }
        }

        Packet::DeltaVisualEntity {
            epoch,
            seq_id,
            delta,
        } => {
            buf.push(Opcode::DeltaVisualEntity as u8);
            buf.extend_from_slice(&epoch.to_le_bytes());
            buf.extend_from_slice(&seq_id.to_le_bytes());
            buf.push(delta.action_byte());
            match delta {
                VisualEntityDelta::Upsert(entity) => {
                    write_visual_entity(&mut buf, entity);
                }
                VisualEntityDelta::Remove { uuid } => {
                    write_uuid(&mut buf, uuid);
                }
            }
        }
    }

    buf
}
