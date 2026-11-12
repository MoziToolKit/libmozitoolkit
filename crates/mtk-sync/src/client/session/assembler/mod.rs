//! # Baseline Stream Assembler (RFC v3.1 Section 4.3 - 4.6 & Section 5.1)
//!
//! Reconstructs a full VoxelStorage volume from progressive chunk streams,
//! verifies chunk xxHash64, decompresses Zstd payloads, and validates the canonical state_hash SSOT.

pub(crate) mod finalize;

use std::collections::HashMap;
use std::io::Read;

use glam::IVec3;

use crate::protocol::*;

/// Baseline assembly state machine.
pub struct BaselineAssembler {
    pub epoch: u64,
    pub snapshot_uuid: [u8; 16],
    pub snapshot_seq: u64,
    pub min_pos: IVec3,
    pub size: IVec3,
    pub total_voxels: usize,
    pub total_chunks: u32,
    pub compression_mode: u8,
    pub palette_bits_per_block: u8,
    pub biome_bits_per_quart: u8,
    pub dimension: String,
    pub block_palette: Vec<String>,
    pub biome_palette: Vec<String>,

    pub chunks_received: u32,
    pub expected_next_voxel_index: u64,
    pub raw_voxel_indices: Vec<u16>,
    pub raw_quart_biome_indices: Vec<u16>,
    pub block_entities: HashMap<IVec3, mtk_model::BlockEntityData>,
    pub visual_entities: Vec<mtk_model::EntityInstance>,
    pub visual_entities_hash: Option<u64>,
}

impl BaselineAssembler {
    /// Creates a new assembler from BASELINE_BEGIN packet.
    pub fn new(begin_pkt: &Packet) -> Result<Self, CancelReason> {
        let (
            epoch,
            snapshot_uuid,
            snapshot_seq,
            min_pos,
            size,
            _state_hash,
            _palette_revision,
            total_chunks,
            _total_encoded_bytes,
            _total_decoded_bytes,
            compression_mode,
            palette_bits_per_block,
            biome_bits_per_quart,
            dimension,
            block_palette,
            biome_palette,
        ) = match begin_pkt {
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
            } => (
                *epoch,
                *snapshot_uuid,
                *snapshot_seq,
                *min_pos,
                *size,
                *state_hash,
                *palette_revision,
                *total_chunks,
                *total_encoded_bytes,
                *total_decoded_bytes,
                *compression_mode,
                *palette_bits_per_block,
                *biome_bits_per_quart,
                dimension.clone(),
                block_palette.clone(),
                biome_palette.clone(),
            ),
            _ => return Err(CancelReason::ReasonBadChunk),
        };

        if size.x <= 0 || size.y <= 0 || size.z <= 0 {
            return Err(CancelReason::ReasonBadChunk);
        }

        let total_voxels = (size.x as u64)
            .checked_mul(size.y as u64)
            .and_then(|v| v.checked_mul(size.z as u64))
            .ok_or(CancelReason::ReasonBadChunk)?;

        if total_voxels > MAX_SELECTION_VOLUME {
            return Err(CancelReason::ReasonBadChunk);
        }

        Ok(Self {
            epoch,
            snapshot_uuid,
            snapshot_seq,
            min_pos,
            size,
            total_voxels: total_voxels as usize,
            total_chunks,
            compression_mode,
            palette_bits_per_block,
            biome_bits_per_quart,
            dimension,
            block_palette,
            biome_palette,
            chunks_received: 0,
            expected_next_voxel_index: 0,
            raw_voxel_indices: Vec::with_capacity(total_voxels as usize),
            raw_quart_biome_indices: Vec::new(),
            block_entities: HashMap::new(),
            visual_entities: Vec::new(),
            visual_entities_hash: None,
        })
    }

    /// Stores continuous visual entities and their authoritative hash.
    pub(crate) fn set_visual_entities(
        &mut self,
        entities: Vec<mtk_model::EntityInstance>,
        entities_hash: u64,
    ) {
        self.visual_entities = entities;
        self.visual_entities_hash = Some(entities_hash);
    }

    /// Appends and processes a single BASELINE_CHUNK.
    pub fn process_chunk(&mut self, chunk: &Packet) -> Result<(), ResyncReason> {
        let (
            epoch,
            snapshot_uuid,
            chunk_index,
            total_chunks,
            first_voxel_index,
            voxel_count,
            _payload_len,
            decoded_target_bytes,
            _chunk_xxhash64,
            payload,
        ) = match chunk {
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
            } => (
                *epoch,
                *snapshot_uuid,
                *chunk_index,
                *total_chunks,
                *first_voxel_index,
                *voxel_count,
                *payload_len,
                *decoded_target_bytes,
                *chunk_xxhash64,
                payload,
            ),
            _ => return Err(ResyncReason::ChunkCorrupted),
        };

        if epoch != self.epoch || snapshot_uuid != self.snapshot_uuid {
            return Err(ResyncReason::ChunkCorrupted);
        }

        if total_chunks != self.total_chunks {
            return Err(ResyncReason::ChunkCorrupted);
        }

        if chunk_index != self.chunks_received {
            return Err(ResyncReason::SequenceGap);
        }

        // Every chunk must advance the voxel cursor monotonically. Pure biome chunks
        // (voxel_count == 0) may start at or beyond the cursor: the server splits the
        // payload on fixed byte offsets, so the trailing biome stream can occupy one or
        // more chunks whose `first_voxel_index` is `k * voxelsPerChunk` rather than the
        // exact voxel count. Rejecting those used to put large selections into an endless
        // resync loop (e.g. 512x128x512 with more than one biome).
        let is_biome_continuation =
            voxel_count == 0 && first_voxel_index >= self.expected_next_voxel_index;
        if first_voxel_index != self.expected_next_voxel_index && !is_biome_continuation {
            return Err(ResyncReason::SequenceGap);
        }

        // Decompress payload
        let decompressed: Vec<u8> = if self.compression_mode == 1 {
            let mut decoder = zstd::stream::Decoder::new(&payload[..])
                .map_err(|_| ResyncReason::ChunkCorrupted)?;
            let mut buf = Vec::with_capacity(decoded_target_bytes as usize);
            decoder
                .read_to_end(&mut buf)
                .map_err(|_| ResyncReason::ChunkCorrupted)?;
            buf
        } else {
            payload.clone()
        };

        // Unpack voxel indices
        let bits = if self.palette_bits_per_block > 0 {
            self.palette_bits_per_block
        } else if self.block_palette.len() > 256 {
            16
        } else {
            8
        };

        let unpacked = unpack_lsb_indices(&decompressed, voxel_count as usize, bits);
        self.raw_voxel_indices.extend(unpacked);

        // Check if there are remaining biome bytes at the end of the chunk
        let voxel_bytes_len = (voxel_count as usize * bits as usize).div_ceil(8);
        if decompressed.len() > voxel_bytes_len {
            let biome_slice = &decompressed[voxel_bytes_len..];
            let biome_bits = if self.biome_bits_per_quart > 0 {
                self.biome_bits_per_quart
            } else {
                8
            };
            let remaining_biomes = biome_slice.len() * 8 / biome_bits as usize;
            let unpacked_biomes = unpack_lsb_indices(biome_slice, remaining_biomes, biome_bits);
            self.raw_quart_biome_indices.extend(unpacked_biomes);
        }

        self.expected_next_voxel_index += voxel_count as u64;
        self.chunks_received += 1;

        Ok(())
    }
}
