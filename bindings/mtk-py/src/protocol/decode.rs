use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use mtk_sync::*;

/// Decodes a raw binary packet into a Python dict.
#[pyfunction]
pub fn decode_packet<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let packet = mtk_sync::decode_packet(data)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let dict = PyDict::new(py);

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
        } => {
            dict.set_item("type", "HELLO")?;
            dict.set_item("min_version", min_version)?;
            dict.set_item("max_version", max_version)?;
            dict.set_item("capabilities", capabilities)?;
            dict.set_item("flags", flags)?;
            dict.set_item("cached_world_uuid", PyBytes::new(py, &cached_world_uuid))?;
            dict.set_item("cached_epoch", cached_epoch)?;
            dict.set_item("cached_seq", cached_seq)?;
            dict.set_item("cached_min_x", cached_min.x)?;
            dict.set_item("cached_min_y", cached_min.y)?;
            dict.set_item("cached_min_z", cached_min.z)?;
            dict.set_item("cached_size_x", cached_size.x)?;
            dict.set_item("cached_size_y", cached_size.y)?;
            dict.set_item("cached_size_z", cached_size.z)?;
            dict.set_item("cached_state_hash", format!("0x{:016x}", cached_state_hash))?;
            dict.set_item("auth_token", auth_token)?;
            dict.set_item("cached_dimension", cached_dimension)?;
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
            dict.set_item("type", "WELCOME")?;
            dict.set_item("negotiated_version", negotiated_version)?;
            dict.set_item("server_uuid", PyBytes::new(py, &server_uuid))?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("server_seq", server_seq)?;
            dict.set_item("min_x", current_min.x)?;
            dict.set_item("min_y", current_min.y)?;
            dict.set_item("min_z", current_min.z)?;
            dict.set_item("size_x", current_size.x)?;
            dict.set_item("size_y", current_size.y)?;
            dict.set_item("size_z", current_size.z)?;
            dict.set_item(
                "current_state_hash",
                format!("0x{:016x}", current_state_hash),
            )?;
            let dec_str = match decision {
                SyncDecision::VerifiedSkipRebuild => "VERIFIED_SKIP_REBUILD",
                SyncDecision::FullBaselineRequired => "FULL_BASELINE_REQUIRED",
                SyncDecision::Rejected => "REJECTED",
            };
            dict.set_item("decision", dec_str)?;
            dict.set_item("message_or_dimension", message_or_dimension)?;
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
            dict.set_item("type", "BASELINE_BEGIN")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("snapshot_uuid", PyBytes::new(py, &snapshot_uuid))?;
            dict.set_item("snapshot_seq", snapshot_seq)?;
            dict.set_item("min_x", min_pos.x)?;
            dict.set_item("min_y", min_pos.y)?;
            dict.set_item("min_z", min_pos.z)?;
            dict.set_item("size_x", size.x)?;
            dict.set_item("size_y", size.y)?;
            dict.set_item("size_z", size.z)?;
            dict.set_item("state_hash", format!("0x{:016x}", state_hash))?;
            dict.set_item("palette_revision", palette_revision)?;
            dict.set_item("total_chunks", total_chunks)?;
            dict.set_item("total_encoded_bytes", total_encoded_bytes)?;
            dict.set_item("total_decoded_bytes", total_decoded_bytes)?;
            dict.set_item("compression_mode", compression_mode)?;
            dict.set_item("palette_bits_per_block", palette_bits_per_block)?;
            dict.set_item("biome_bits_per_quart", biome_bits_per_quart)?;
            dict.set_item("dimension", dimension)?;
            dict.set_item("block_palette", block_palette)?;
            dict.set_item("biome_palette", biome_palette)?;
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
            dict.set_item("type", "BASELINE_CHUNK")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("snapshot_uuid", PyBytes::new(py, &snapshot_uuid))?;
            dict.set_item("chunk_index", chunk_index)?;
            dict.set_item("total_chunks", total_chunks)?;
            dict.set_item("first_voxel_index", first_voxel_index)?;
            dict.set_item("voxel_count", voxel_count)?;
            dict.set_item("payload_len", payload_len)?;
            dict.set_item("decoded_target_bytes", decoded_target_bytes)?;
            dict.set_item("chunk_xxhash64", chunk_xxhash64)?;
            dict.set_item("payload", PyBytes::new(py, &payload))?;
        }
        Packet::BaselineEnd {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            total_chunks_sent,
            final_state_hash,
            status,
        } => {
            dict.set_item("type", "BASELINE_END")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("snapshot_uuid", PyBytes::new(py, &snapshot_uuid))?;
            dict.set_item("snapshot_seq", snapshot_seq)?;
            dict.set_item("total_chunks_sent", total_chunks_sent)?;
            dict.set_item("final_state_hash", format!("0x{:016x}", final_state_hash))?;
            dict.set_item("status", status)?;
        }
        Packet::BaselineAck {
            epoch,
            snapshot_uuid,
            committed_seq,
            result_code,
        } => {
            dict.set_item("type", "BASELINE_ACK")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("snapshot_uuid", PyBytes::new(py, &snapshot_uuid))?;
            dict.set_item("committed_seq", committed_seq)?;
            dict.set_item("result_code", result_code as u8)?;
        }
        Packet::DeltaOp {
            epoch,
            seq_id,
            palette_revision,
            count,
            batch_xxhash64,
            changes,
        } => {
            dict.set_item("type", "DELTA_OP")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("seq_id", seq_id)?;
            dict.set_item("palette_revision", palette_revision)?;
            dict.set_item("count", count)?;
            dict.set_item("batch_xxhash64", batch_xxhash64)?;

            let change_list = PyList::empty(py);
            for c in changes {
                let item = PyDict::new(py);
                item.set_item("rel_x", c.rel_pos.x)?;
                item.set_item("rel_y", c.rel_pos.y)?;
                item.set_item("rel_z", c.rel_pos.z)?;
                item.set_item("state_id", c.state_id)?;
                change_list.append(item)?;
            }
            dict.set_item("changes", change_list)?;
        }
        Packet::PaletteUpdate {
            epoch,
            seq_id,
            base_revision,
            new_revision,
            entries,
        } => {
            dict.set_item("type", "PALETTE_UPDATE")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("seq_id", seq_id)?;
            dict.set_item("base_revision", base_revision)?;
            dict.set_item("new_revision", new_revision)?;
            let entry_list = PyList::empty(py);
            for (id, str_val) in entries {
                entry_list.append((id, str_val))?;
            }
            dict.set_item("entries", entry_list)?;
        }
        Packet::ResyncRequest {
            epoch,
            missing_from_seq,
            missing_to_seq,
            reason,
        } => {
            dict.set_item("type", "RESYNC_REQUEST")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("missing_from_seq", missing_from_seq)?;
            dict.set_item("missing_to_seq", missing_to_seq)?;
            dict.set_item("reason", reason as u8)?;
        }
        Packet::Cancel {
            epoch,
            snapshot_uuid,
            reason,
        } => {
            dict.set_item("type", "CANCEL")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("snapshot_uuid", PyBytes::new(py, &snapshot_uuid))?;
            dict.set_item("reason", reason as u8)?;
        }
        Packet::Ping { timestamp } => {
            dict.set_item("type", "PING")?;
            dict.set_item("timestamp", timestamp)?;
        }
        Packet::Pong { timestamp } => {
            dict.set_item("type", "PONG")?;
            dict.set_item("timestamp", timestamp)?;
        }
        Packet::Error {
            epoch,
            error_code,
            message,
        } => {
            dict.set_item("type", "ERROR")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("error_code", error_code as u16)?;
            dict.set_item("message", message)?;
        }
        Packet::BaselineEntities {
            epoch,
            snapshot_uuid,
            entities,
        } => {
            dict.set_item("type", "BASELINE_ENTITIES")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("snapshot_uuid", PyBytes::new(py, &snapshot_uuid))?;
            let ent_list = pyo3::types::PyList::empty(py);
            for (pos, ed) in entities {
                let item = PyDict::new(py);
                item.set_item("rel_x", pos.x)?;
                item.set_item("rel_y", pos.y)?;
                item.set_item("rel_z", pos.z)?;
                let json_str = serde_json::to_string(&ed).unwrap_or_default();
                item.set_item("entity", json_str)?;
                ent_list.append(item)?;
            }
            dict.set_item("entities", ent_list)?;
        }
        Packet::DeltaEntity {
            epoch,
            seq_id,
            rel_pos,
            entity,
        } => {
            dict.set_item("type", "DELTA_ENTITY")?;
            dict.set_item("epoch", epoch)?;
            dict.set_item("seq_id", seq_id)?;
            dict.set_item("rel_x", rel_pos.x)?;
            dict.set_item("rel_y", rel_pos.y)?;
            dict.set_item("rel_z", rel_pos.z)?;
            if let Some(ed) = entity {
                let json_str = serde_json::to_string(&ed).unwrap_or_default();
                dict.set_item("entity", json_str)?;
            } else {
                dict.set_item("entity", py.None())?;
            }
        }
    }

    Ok(dict)
}
