//! # Authoritative Delta Batch Encoder (RFC v3.1 Section 4.7)

use crate::protocol::{
    compute_delta_batch_hash, DeltaChangeEntry, Packet, MAX_DELTA_BATCH_CHANGES,
};

/// Encodes a list of `DeltaChangeEntry` into a `Packet::DeltaOp`, automatically computing `batch_xxhash64`.
/// If the number of changes exceeds `MAX_DELTA_BATCH_CHANGES`, it splits them into multiple packets.
pub fn encode_delta_op(
    epoch: u64,
    seq_id: u64,
    palette_revision: u32,
    changes: Vec<DeltaChangeEntry>,
) -> Vec<Packet> {
    if changes.is_empty() {
        return Vec::new();
    }

    let mut packets = Vec::new();
    let chunks = changes.chunks(MAX_DELTA_BATCH_CHANGES);

    for (current_seq, chunk) in (seq_id..).zip(chunks) {
        let chunk_vec = chunk.to_vec();
        let batch_xxhash64 = compute_delta_batch_hash(&chunk_vec);
        let count = chunk_vec.len() as u16;

        packets.push(Packet::DeltaOp {
            epoch,
            seq_id: current_seq,
            palette_revision,
            count,
            batch_xxhash64,
            changes: chunk_vec,
        });
    }

    packets
}
