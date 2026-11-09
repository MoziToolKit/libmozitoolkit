use mtk_sync::{DeltaChangeEntry, Packet, SyncDecision};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Encodes a WELCOME packet (0x02) to bytes.
#[pyfunction]
#[pyo3(signature = (server_uuid, epoch, sequence, decision, current_min, current_size, current_state_hash, dimension="minecraft:overworld", negotiated_version=3))]
pub fn encode_welcome<'py>(
    py: Python<'py>,
    server_uuid: &[u8],
    epoch: u64,
    sequence: u64,
    decision: u8,
    current_min: (i32, i32, i32),
    current_size: (i32, i32, i32),
    current_state_hash: u64,
    dimension: &str,
    negotiated_version: u16,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut uuid_arr = [0u8; 16];
    if server_uuid.len() >= 16 {
        uuid_arr.copy_from_slice(&server_uuid[..16]);
    }
    let dec = match decision {
        0 => SyncDecision::VerifiedSkipRebuild,
        2 => SyncDecision::FullBaselineRequired,
        _ => SyncDecision::Rejected,
    };
    let pkt = Packet::Welcome {
        negotiated_version,
        server_uuid: uuid_arr,
        epoch,
        server_seq: sequence,
        current_min: glam::IVec3::new(current_min.0, current_min.1, current_min.2),
        current_size: glam::IVec3::new(current_size.0, current_size.1, current_size.2),
        current_state_hash,
        decision: dec,
        message_or_dimension: dimension.to_string(),
    };
    let bytes = mtk_sync::encode_packet(&pkt);
    Ok(PyBytes::new(py, &bytes))
}

/// Encodes a BASELINE_BEGIN packet (0x03) to bytes.
#[pyfunction]
#[pyo3(signature = (
    epoch, snapshot_uuid, snapshot_seq, min_pos, size, state_hash,
    palette_revision, total_chunks, total_encoded_bytes, total_decoded_bytes,
    compression_mode, palette_bits_per_block, biome_bits_per_quart,
    dimension, block_palette, biome_palette
))]
pub fn encode_baseline_begin<'py>(
    py: Python<'py>,
    epoch: u64,
    snapshot_uuid: &[u8],
    snapshot_seq: u64,
    min_pos: (i32, i32, i32),
    size: (i32, i32, i32),
    state_hash: u64,
    palette_revision: u32,
    total_chunks: u32,
    total_encoded_bytes: u64,
    total_decoded_bytes: u64,
    compression_mode: u8,
    palette_bits_per_block: u8,
    biome_bits_per_quart: u8,
    dimension: &str,
    block_palette: Vec<String>,
    biome_palette: Vec<String>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut uuid_arr = [0u8; 16];
    if snapshot_uuid.len() >= 16 {
        uuid_arr.copy_from_slice(&snapshot_uuid[..16]);
    }
    let pkt = Packet::BaselineBegin {
        epoch,
        snapshot_uuid: uuid_arr,
        snapshot_seq,
        min_pos: glam::IVec3::new(min_pos.0, min_pos.1, min_pos.2),
        size: glam::IVec3::new(size.0, size.1, size.2),
        state_hash,
        palette_revision,
        total_chunks,
        total_encoded_bytes,
        total_decoded_bytes,
        compression_mode,
        palette_bits_per_block,
        biome_bits_per_quart,
        dimension: dimension.to_string(),
        block_palette,
        biome_palette,
    };
    let bytes = mtk_sync::encode_packet(&pkt);
    Ok(PyBytes::new(py, &bytes))
}

/// Encodes a BASELINE_CHUNK packet (0x04) to bytes.
#[pyfunction]
#[pyo3(signature = (
    epoch, snapshot_uuid, chunk_index, total_chunks, first_voxel_index,
    voxel_count, decoded_target_bytes, payload, chunk_xxhash64=0
))]
pub fn encode_baseline_chunk<'py>(
    py: Python<'py>,
    epoch: u64,
    snapshot_uuid: &[u8],
    chunk_index: u32,
    total_chunks: u32,
    first_voxel_index: u64,
    voxel_count: u32,
    decoded_target_bytes: u32,
    payload: &[u8],
    chunk_xxhash64: u64,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut uuid_arr = [0u8; 16];
    if snapshot_uuid.len() >= 16 {
        uuid_arr.copy_from_slice(&snapshot_uuid[..16]);
    }
    let hash = if chunk_xxhash64 != 0 {
        chunk_xxhash64
    } else {
        xxhash_rust::xxh64::xxh64(payload, 0)
    };
    let pkt = Packet::BaselineChunk {
        epoch,
        snapshot_uuid: uuid_arr,
        chunk_index,
        total_chunks,
        first_voxel_index,
        voxel_count,
        payload_len: payload.len() as u32,
        decoded_target_bytes,
        chunk_xxhash64: hash,
        payload: payload.to_vec(),
    };
    let bytes = mtk_sync::encode_packet(&pkt);
    Ok(PyBytes::new(py, &bytes))
}

/// Encodes a BASELINE_END packet (0x05) to bytes.
#[pyfunction]
#[pyo3(signature = (epoch, snapshot_uuid, snapshot_seq, total_chunks_sent, final_state_hash, status=0))]
pub fn encode_baseline_end<'py>(
    py: Python<'py>,
    epoch: u64,
    snapshot_uuid: &[u8],
    snapshot_seq: u64,
    total_chunks_sent: u32,
    final_state_hash: u64,
    status: u8,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut uuid_arr = [0u8; 16];
    if snapshot_uuid.len() >= 16 {
        uuid_arr.copy_from_slice(&snapshot_uuid[..16]);
    }
    let pkt = Packet::BaselineEnd {
        epoch,
        snapshot_uuid: uuid_arr,
        snapshot_seq,
        total_chunks_sent,
        final_state_hash,
        status,
    };
    let bytes = mtk_sync::encode_packet(&pkt);
    Ok(PyBytes::new(py, &bytes))
}

/// Encodes a DELTA_OP packet (0x07) to bytes.
#[pyfunction]
#[pyo3(signature = (epoch, seq_id, palette_revision, entries))]
pub fn encode_delta_op<'py>(
    py: Python<'py>,
    epoch: u64,
    seq_id: u64,
    palette_revision: u32,
    entries: Vec<(i32, i32, i32, u32)>,
) -> PyResult<Bound<'py, PyBytes>> {
    let delta_entries: Vec<DeltaChangeEntry> = entries
        .into_iter()
        .map(|(x, y, z, state_id)| DeltaChangeEntry {
            rel_pos: glam::IVec3::new(x, y, z),
            state_id: state_id as u16,
        })
        .collect();
    let count = delta_entries.len() as u16;
    let mut hasher = xxhash_rust::xxh64::Xxh64::new(0);
    for entry in &delta_entries {
        hasher.update(&entry.rel_pos.x.to_le_bytes());
        hasher.update(&entry.rel_pos.y.to_le_bytes());
        hasher.update(&entry.rel_pos.z.to_le_bytes());
        hasher.update(&entry.state_id.to_le_bytes());
    }
    let batch_xxhash64 = hasher.digest();

    let pkt = Packet::DeltaOp {
        epoch,
        seq_id,
        palette_revision,
        count,
        batch_xxhash64,
        changes: delta_entries,
    };
    let bytes = mtk_sync::encode_packet(&pkt);
    Ok(PyBytes::new(py, &bytes))
}

/// Encodes a PALETTE_UPDATE packet (0x08) to bytes.
#[pyfunction]
#[pyo3(signature = (epoch, seq_id, base_revision, new_revision, entries))]
pub fn encode_palette_update<'py>(
    py: Python<'py>,
    epoch: u64,
    seq_id: u64,
    base_revision: u32,
    new_revision: u32,
    entries: Vec<(u16, String)>,
) -> PyResult<Bound<'py, PyBytes>> {
    let pkt = Packet::PaletteUpdate {
        epoch,
        seq_id,
        base_revision,
        new_revision,
        entries,
    };
    let bytes = mtk_sync::encode_packet(&pkt);
    Ok(PyBytes::new(py, &bytes))
}
