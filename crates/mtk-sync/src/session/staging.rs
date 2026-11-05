//! # Delta Staging Buffer (RFC v3.1 Section 5.2)
//!
//! Stages in-flight DELTA_OP and PALETTE_UPDATE packets arriving during Baseline transmission
//! and aligns with Snapshot Sequence barrier S for ordered replay.

use std::collections::VecDeque;

use crate::protocol::*;

/// Staging queue for in-flight delta packets.
#[derive(Debug, Default)]
pub(crate) struct DeltaStagingBuffer {
    queue: VecDeque<(u64, Packet)>,
    total_bytes: usize,
}

impl DeltaStagingBuffer {
    pub(crate) fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            total_bytes: 0,
        }
    }

    /// Clears the staging buffer completely.
    pub(crate) fn clear(&mut self) {
        self.queue.clear();
        self.total_bytes = 0;
    }

    /// Pushes an in-flight delta or palette update packet with its sequence ID.
    pub(crate) fn push(
        &mut self,
        seq_id: u64,
        packet: Packet,
        approximate_bytes: usize,
    ) -> Result<(), ResyncReason> {
        if self.queue.len() >= MAX_DELTA_STAGING_BATCHES || self.total_bytes >= 16 * 1024 * 1024 {
            return Err(ResyncReason::StateHashMismatch);
        }
        self.queue.push_back((seq_id, packet));
        self.total_bytes += approximate_bytes;
        Ok(())
    }

    /// Discards all packets with sequence <= snapshot_seq S, and verifies that
    /// the remaining packets start exactly at S + 1 and are strictly contiguous.
    pub(crate) fn drain_replaying_after_snapshot(
        &mut self,
        snapshot_seq: u64,
    ) -> Result<Vec<Packet>, ResyncReason> {
        // Discard any packet <= S
        while let Some((seq, _)) = self.queue.front() {
            if *seq <= snapshot_seq {
                self.queue.pop_front();
            } else {
                break;
            }
        }

        if self.queue.is_empty() {
            return Ok(Vec::new());
        }

        // Verify gap between snapshot_seq and first staged packet
        let first_seq = self.queue.front().unwrap().0;
        if first_seq != snapshot_seq + 1 {
            return Err(ResyncReason::SequenceGap);
        }

        let mut result = Vec::with_capacity(self.queue.len());
        for (expected, (seq, pkt)) in (first_seq..).zip(self.queue.drain(..)) {
            if seq != expected {
                return Err(ResyncReason::SequenceGap);
            }
            result.push(pkt);
        }

        self.total_bytes = 0;
        Ok(result)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staging_purge_and_gap_detection() {
        let mut staging = DeltaStagingBuffer::new();

        let dummy_pkt = |seq: u64| Packet::DeltaOp {
            epoch: 1,
            seq_id: seq,
            palette_revision: 1,
            count: 0,
            batch_xxhash64: 0,
            changes: Vec::new(),
        };

        staging.push(9, dummy_pkt(9), 10).unwrap();
        staging.push(10, dummy_pkt(10), 10).unwrap();
        staging.push(11, dummy_pkt(11), 10).unwrap();
        staging.push(12, dummy_pkt(12), 10).unwrap();

        // Baseline completed at snapshot sequence S = 10
        let replayed = staging.drain_replaying_after_snapshot(10).unwrap();
        assert_eq!(replayed.len(), 2);

        // Gap detection
        staging.push(15, dummy_pkt(15), 10).unwrap();
        let err = staging.drain_replaying_after_snapshot(10);
        assert_eq!(err, Err(ResyncReason::SequenceGap));
    }
}
