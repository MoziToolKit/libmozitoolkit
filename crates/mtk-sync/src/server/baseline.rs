//! # Authoritative Baseline Stream Encoder (RFC v3.1 / v3.2 Section 4.3 - 4.6)
//!
//! Encodes 3D voxel selections, biomes, and entities into canonical RFC v3.1 progressive
//! packet streams with Bit-packing, xxHash64 verification, and Zstd compression.

use glam::IVec3;
use std::collections::HashMap;
use xxhash_rust::xxh64::Xxh64;

use mtk_model::{BlockEntityData, EntityInstance};
use mtk_voxel::VoxelStorage;

use crate::protocol::{
    choose_bits_per_item, compute_sparse_state_hash, compute_sparse_state_hash_for_storage,
    pack_lsb_indices, Packet, ProtocolError, QuartGrid, SparseBlockEntry, MAX_SELECTION_VOLUME,
};

/// Configuration for baseline encoding and chunking.
#[derive(Debug, Clone)]
pub struct BaselineEncoderConfig {
    /// Maximum decoded payload bytes per chunk before compression (default: 256 KiB = 262,144).
    pub max_chunk_size: usize,
    /// 0 = Uncompressed, 1 = Zstd compression (default: 1).
    pub compression_mode: u8,
    /// Zstd compression level (1-22, default: 3).
    pub compression_level: i32,
    /// Epoch identifier for this server session.
    pub epoch: u64,
    /// Snapshot UUID.
    pub snapshot_uuid: [u8; 16],
    /// Snapshot sequence ID.
    pub snapshot_seq: u64,
    /// Palette revision ID.
    pub palette_revision: u32,
}

impl Default for BaselineEncoderConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 256 * 1024, // 256 KiB
            compression_mode: 1,
            compression_level: 3,
            epoch: 1,
            snapshot_uuid: [0u8; 16],
            snapshot_seq: 1,
            palette_revision: 1,
        }
    }
}

/// A fully packaged stream of RFC baseline packets ready to be sent over network.
#[derive(Debug, Clone)]
pub struct BaselineStream {
    pub packets: Vec<Packet>,
    pub state_hash: u64,
    pub total_voxels: u64,
    pub total_chunks: u32,
    pub block_palette: Vec<String>,
    pub biome_palette: Vec<String>,
}

/// Validates selection dimensions and calculates total voxels without overflow.
#[inline]
pub fn validate_baseline_bounds(size: IVec3) -> Result<u64, ProtocolError> {
    if size.x <= 0 || size.y <= 0 || size.z <= 0 {
        return Err(ProtocolError::InvalidDimensions(size.x, size.y, size.z));
    }

    let total_voxels = (size.x as u64)
        .checked_mul(size.y as u64)
        .and_then(|v| v.checked_mul(size.z as u64))
        .ok_or(ProtocolError::ResourceLimitExceeded {
            resource: "Volume Voxels",
            limit: MAX_SELECTION_VOLUME as usize,
            actual: usize::MAX,
        })?;

    if total_voxels > MAX_SELECTION_VOLUME {
        return Err(ProtocolError::ResourceLimitExceeded {
            resource: "Volume Voxels",
            limit: MAX_SELECTION_VOLUME as usize,
            actual: total_voxels as usize,
        });
    }

    Ok(total_voxels)
}

/// Helper to build unique palettes and mapped index tables.
#[derive(Debug, Default)]
struct PaletteBuilder {
    palette: Vec<String>,
    map: HashMap<String, u16>,
}

impl PaletteBuilder {
    fn new_block_palette() -> Self {
        let mut map = HashMap::new();
        map.insert("minecraft:air".to_string(), 0);
        Self {
            palette: vec!["minecraft:air".to_string()],
            map,
        }
    }

    fn new_biome_palette() -> Self {
        Self {
            palette: Vec::new(),
            map: HashMap::new(),
        }
    }

    #[inline]
    fn get_or_insert(&mut self, name: &str) -> u16 {
        if let Some(&idx) = self.map.get(name) {
            idx
        } else {
            let idx = self.palette.len() as u16;
            self.palette.push(name.to_string());
            self.map.insert(name.to_string(), idx);
            idx
        }
    }

    fn finish(mut self, fallback: Option<&str>) -> Vec<String> {
        if self.palette.is_empty() {
            if let Some(fb) = fallback {
                self.palette.push(fb.to_string());
            }
        }
        self.palette
    }
}

/// Encodes an existing `VoxelStorage` into a canonical RFC v3.1 baseline stream.
pub fn encode_baseline_from_storage(
    storage: &VoxelStorage,
    min_pos: IVec3,
    size: IVec3,
    dimension: &str,
    visual_entities: &[EntityInstance],
    config: &BaselineEncoderConfig,
) -> Result<BaselineStream, ProtocolError> {
    let total_voxels = validate_baseline_bounds(size)?;

    // 1. Build palettes and indices in (Y, Z, X) traversal order
    let mut block_builder = PaletteBuilder::new_block_palette();
    let mut voxel_indices: Vec<u16> = Vec::with_capacity(total_voxels as usize);
    let mut block_entities: Vec<(IVec3, BlockEntityData)> = Vec::new();

    for ly in 0..size.y {
        let wy = min_pos.y + ly;
        for lz in 0..size.z {
            let wz = min_pos.z + lz;
            for lx in 0..size.x {
                let wx = min_pos.x + lx;
                let raw_block = storage.get_block(wx, wy, wz);
                let state_str = if raw_block.is_empty() || raw_block == "minecraft:air" {
                    "minecraft:air"
                } else {
                    raw_block
                };

                let p_idx = block_builder.get_or_insert(state_str);
                voxel_indices.push(p_idx);

                if let Some(be) = storage.get_block_entity(wx, wy, wz) {
                    block_entities.push((IVec3::new(lx, ly, lz), be.clone()));
                }
            }
        }
    }
    let block_palette = block_builder.finish(None);

    // 2. Biome scanning on 4x4x4 quart grid
    let quart_grid = QuartGrid::from_bounds(min_pos, size);
    let mut biome_builder = PaletteBuilder::new_biome_palette();
    let mut quart_biome_indices: Vec<u16> = Vec::with_capacity(quart_grid.total_quarts);

    for qy in 0..quart_grid.size_q.y as usize {
        let world_qy = quart_grid.min_q.y + qy as i32;
        for qz in 0..quart_grid.size_q.z as usize {
            let world_qz = quart_grid.min_q.z + qz as i32;
            for qx in 0..quart_grid.size_q.x as usize {
                let world_qx = quart_grid.min_q.x + qx as i32;
                let b_str = storage.get_biome(world_qx * 4, world_qy * 4, world_qz * 4);
                let normalized = if b_str.trim().is_empty() {
                    "minecraft:plains"
                } else {
                    b_str.trim()
                };
                let b_idx = biome_builder.get_or_insert(normalized);
                quart_biome_indices.push(b_idx);
            }
        }
    }
    let biome_palette = biome_builder.finish(Some("minecraft:plains"));

    // 3. Compute authoritative state_hash
    let state_hash = compute_sparse_state_hash_for_storage(storage, min_pos, size, dimension);

    // 4. Delegate to chunking assembly
    assemble_baseline_stream(
        min_pos,
        size,
        total_voxels,
        dimension,
        block_palette,
        voxel_indices,
        biome_palette,
        quart_biome_indices,
        block_entities,
        visual_entities.to_vec(),
        state_hash,
        config,
    )
}

/// Encodes raw memory arrays into a canonical RFC v3.1 baseline stream (ideal for JNI / low-overhead FFI).
pub fn encode_baseline_from_raw(
    min_pos: IVec3,
    size: IVec3,
    dimension: &str,
    raw_states: &[u32],
    state_to_name: &[String],
    raw_biomes: &[u32],
    biome_to_name: &[String],
    block_entities: &[(IVec3, String)], // Relative coords + JSON string
    visual_entities: &[EntityInstance],
    config: &BaselineEncoderConfig,
) -> Result<BaselineStream, ProtocolError> {
    let total_voxels = validate_baseline_bounds(size)?;

    if raw_states.len() < total_voxels as usize {
        return Err(ProtocolError::TruncatedPayload {
            field: "raw_states length less than total voxels",
        });
    }

    // 1. Build compact block palette & mapped indices
    let mut block_builder = PaletteBuilder::new_block_palette();
    let mut id_remap: HashMap<u32, u16> = HashMap::new();
    let mut voxel_indices: Vec<u16> = Vec::with_capacity(total_voxels as usize);

    for &raw_id in raw_states.iter().take(total_voxels as usize) {
        if let Some(&p_idx) = id_remap.get(&raw_id) {
            voxel_indices.push(p_idx);
        } else {
            let state_str = state_to_name
                .get(raw_id as usize)
                .map(|s| s.as_str())
                .unwrap_or("minecraft:air");
            let normalized = if state_str.is_empty() || state_str == "minecraft:air" {
                "minecraft:air"
            } else {
                state_str
            };

            let p_idx = block_builder.get_or_insert(normalized);
            id_remap.insert(raw_id, p_idx);
            voxel_indices.push(p_idx);
        }
    }
    let block_palette = block_builder.finish(None);

    // 2. Build biome palette & mapped indices
    let quart_grid = QuartGrid::from_bounds(min_pos, size);
    let mut biome_builder = PaletteBuilder::new_biome_palette();
    let mut biome_id_remap: HashMap<u32, u16> = HashMap::new();
    let mut quart_biome_indices: Vec<u16> = Vec::with_capacity(raw_biomes.len());

    for &raw_b_id in raw_biomes {
        if let Some(&b_idx) = biome_id_remap.get(&raw_b_id) {
            quart_biome_indices.push(b_idx);
        } else {
            let b_str = biome_to_name
                .get(raw_b_id as usize)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("minecraft:plains");
            let b_idx = biome_builder.get_or_insert(b_str);
            biome_id_remap.insert(raw_b_id, b_idx);
            quart_biome_indices.push(b_idx);
        }
    }
    let biome_palette = biome_builder.finish(Some("minecraft:plains"));

    // 3. Compute state hash
    let sx = size.x as usize;
    let slice_area = sx * (size.z as usize);

    let mut entity_map: HashMap<IVec3, &str> = HashMap::new();
    let mut parsed_block_entities = Vec::new();
    for (rel_pos, json) in block_entities {
        entity_map.insert(*rel_pos, json.as_str());
        if let Some(data) = BlockEntityData::from_visual_json(json) {
            parsed_block_entities.push((*rel_pos, data));
        }
    }

    let non_air_iter = voxel_indices
        .iter()
        .enumerate()
        .filter_map(|(idx, &p_idx)| {
            if p_idx == 0 {
                return None;
            }
            let ly = (idx / slice_area) as u32;
            let rem = idx % slice_area;
            let lz = (rem / sx) as u32;
            let lx = (rem % sx) as u32;

            let q_idx = quart_grid.relative_voxel_to_quart_index(lx, ly, lz);
            let b_idx = quart_biome_indices.get(q_idx).copied().unwrap_or(0);
            let biome = biome_palette
                .get(b_idx as usize)
                .map(|s| s.as_str())
                .unwrap_or("minecraft:plains");

            let entity_json = entity_map
                .get(&IVec3::new(lx as i32, ly as i32, lz as i32))
                .copied();

            Some(SparseBlockEntry {
                rel_x: lx,
                rel_y: ly,
                rel_z: lz,
                state: &block_palette[p_idx as usize],
                biome,
                entity_json,
            })
        });

    let state_hash = compute_sparse_state_hash(min_pos, size, dimension, non_air_iter);

    // 4. Delegate to chunking assembly
    assemble_baseline_stream(
        min_pos,
        size,
        total_voxels,
        dimension,
        block_palette,
        voxel_indices,
        biome_palette,
        quart_biome_indices,
        parsed_block_entities,
        visual_entities.to_vec(),
        state_hash,
        config,
    )
}

fn assemble_baseline_stream(
    min_pos: IVec3,
    size: IVec3,
    total_voxels: u64,
    dimension: &str,
    block_palette: Vec<String>,
    voxel_indices: Vec<u16>,
    biome_palette: Vec<String>,
    quart_biome_indices: Vec<u16>,
    block_entities: Vec<(IVec3, BlockEntityData)>,
    visual_entities: Vec<EntityInstance>,
    state_hash: u64,
    config: &BaselineEncoderConfig,
) -> Result<BaselineStream, ProtocolError> {
    let palette_bits_per_block = choose_bits_per_item(block_palette.len());
    let biome_bits_per_quart = if biome_palette.len() > 1 {
        choose_bits_per_item(biome_palette.len())
    } else {
        0
    };

    // Pack Biomes stream if multiple biomes exist
    let packed_biomes = if biome_palette.len() > 1 && !quart_biome_indices.is_empty() {
        pack_lsb_indices(&quart_biome_indices, biome_bits_per_quart)
    } else {
        Vec::new()
    };

    // Determine voxels per chunk based on max_chunk_size
    let bits = palette_bits_per_block as usize;
    let bytes_per_voxel_num = bits;
    let bytes_per_voxel_denom = 8;
    let max_chunk_voxels =
        ((config.max_chunk_size * bytes_per_voxel_denom) / bytes_per_voxel_num).max(1);

    let mut chunk_packets: Vec<Packet> = Vec::new();
    let mut total_encoded_bytes = 0u64;
    let mut total_decoded_bytes = 0u64;

    let mut voxel_offset = 0usize;
    let total_v = total_voxels as usize;

    let mut pending_biome_bytes = &packed_biomes[..];

    while voxel_offset < total_v || !pending_biome_bytes.is_empty() {
        let chunk_index = chunk_packets.len() as u32;
        let remaining_voxels = total_v.saturating_sub(voxel_offset);
        let cur_voxel_count = remaining_voxels.min(max_chunk_voxels);

        // Pack voxel slice
        let mut decompressed = if cur_voxel_count > 0 {
            let slice = &voxel_indices[voxel_offset..voxel_offset + cur_voxel_count];
            pack_lsb_indices(slice, palette_bits_per_block)
        } else {
            Vec::new()
        };

        // If this is the last voxel chunk or pure biome continuation, attach biomes
        let is_last_voxel_chunk = voxel_offset + cur_voxel_count >= total_v;
        if is_last_voxel_chunk && !pending_biome_bytes.is_empty() {
            let space_left = config.max_chunk_size.saturating_sub(decompressed.len());
            let take_biomes = pending_biome_bytes
                .len()
                .min(space_left.max(pending_biome_bytes.len()));
            decompressed.extend_from_slice(&pending_biome_bytes[..take_biomes]);
            pending_biome_bytes = &pending_biome_bytes[take_biomes..];
        }

        let decoded_target_bytes = decompressed.len() as u32;

        // Compress payload
        let payload = if config.compression_mode == 1 {
            zstd::encode_all(&decompressed[..], config.compression_level).map_err(|_| {
                ProtocolError::TruncatedPayload {
                    field: "zstd compression failed",
                }
            })?
        } else {
            decompressed
        };

        let mut hasher = Xxh64::new(0);
        hasher.update(&payload);
        let chunk_xxhash64 = hasher.digest();

        let payload_len = payload.len() as u32;
        total_encoded_bytes += payload_len as u64;
        total_decoded_bytes += decoded_target_bytes as u64;

        chunk_packets.push(Packet::BaselineChunk {
            epoch: config.epoch,
            snapshot_uuid: config.snapshot_uuid,
            chunk_index,
            total_chunks: 0, // Placeholder, updated below
            first_voxel_index: voxel_offset as u64,
            voxel_count: cur_voxel_count as u32,
            payload_len,
            decoded_target_bytes,
            chunk_xxhash64,
            payload,
        });

        voxel_offset += cur_voxel_count;
    }

    let total_chunks = chunk_packets.len() as u32;

    // Backfill total_chunks on all chunks
    for pkt in &mut chunk_packets {
        if let Packet::BaselineChunk {
            total_chunks: ref mut tc,
            ..
        } = pkt
        {
            *tc = total_chunks;
        }
    }

    // 1. BASELINE_BEGIN
    let begin_packet = Packet::BaselineBegin {
        epoch: config.epoch,
        snapshot_uuid: config.snapshot_uuid,
        snapshot_seq: config.snapshot_seq,
        min_pos,
        size,
        state_hash,
        palette_revision: config.palette_revision,
        total_chunks,
        total_encoded_bytes,
        total_decoded_bytes,
        compression_mode: config.compression_mode,
        palette_bits_per_block,
        biome_bits_per_quart,
        dimension: dimension.to_string(),
        block_palette: block_palette.clone(),
        biome_palette: biome_palette.clone(),
    };

    // 2. BASELINE_END
    let end_packet = Packet::BaselineEnd {
        epoch: config.epoch,
        snapshot_uuid: config.snapshot_uuid,
        snapshot_seq: config.snapshot_seq,
        total_chunks_sent: total_chunks,
        final_state_hash: state_hash,
        status: 0,
    };

    let mut packets = Vec::with_capacity(chunk_packets.len() + 4);
    packets.push(begin_packet);
    packets.extend(chunk_packets);
    packets.push(end_packet);

    // 3. Optional BASELINE_ENTITIES (RFC v3.2 Sparse Block Entities)
    if !block_entities.is_empty() {
        packets.push(Packet::BaselineEntities {
            epoch: config.epoch,
            snapshot_uuid: config.snapshot_uuid,
            entities: block_entities,
        });
    }

    // 4. Optional BASELINE_VISUAL_ENTITIES (RFC v3.2 Continuous Visual Entities)
    if !visual_entities.is_empty() {
        let entities_hash = crate::protocol::compute_visual_entities_hash(&visual_entities);
        packets.push(Packet::BaselineVisualEntities {
            epoch: config.epoch,
            snapshot_seq: config.snapshot_seq,
            entities_hash,
            entities: visual_entities,
        });
    }

    Ok(BaselineStream {
        packets,
        state_hash,
        total_voxels,
        total_chunks,
        block_palette,
        biome_palette,
    })
}
