//! # Voxel Point Cloud Storage & Serialization
//!
//! Provides dense, unculled 3D point cloud representations of voxel worlds
//! for lossless persistent storage, scene roundtripping, and host viewport
//! attribute mapping without spatial occlusion culling.

mod convert;
mod mesh_data;
mod source;

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::types::VoxelError;

pub use source::PointCloudVoxelSource;

/// High-throughput flat-buffered representation of a voxel point cloud.
///
/// Designed for zero-copy memoryview sharing across FFI/Python boundaries
/// and direct attribute mapping to Blender/USD point domains without spatial occlusion culling.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VoxelPointCloud {
    /// 3D transformed vertex coordinates: `[x0, y0, z0, x1, y1, z1, ...]`.
    /// Computed from block centers `(x + 0.5, y + 0.5, z + 0.5)` under active coordinate system and centering.
    pub positions: Vec<f32>,
    /// Native discrete integer voxel coordinates.
    pub block_x: Vec<i32>,
    pub block_y: Vec<i32>,
    pub block_z: Vec<i32>,
    /// Unique canonical Minecraft blockstate palette.
    pub state_palette: Vec<String>,
    /// Per-point indices into `state_palette`.
    pub state_indices: Vec<i32>,
    /// Unique canonical Minecraft biome palette.
    pub biome_palette: Vec<String>,
    /// Per-point indices into `biome_palette`.
    pub biome_indices: Vec<i32>,
    /// Unique canonical visual NBT JSON strings for BlockEntityData.
    pub entity_palette: Vec<String>,
    /// Per-point indices into `entity_palette` (-1 indicates no block entity).
    /// If empty, indicates no points in this cloud possess a block entity.
    pub entity_indices: Vec<i32>,
    /// Optional composite light levels (sky in high nibble, block in low nibble).
    pub light_levels: Vec<u8>,
    /// Optional bounding box `[min_x, min_y, min_z, size_x, size_y, size_z]` to preserve origin alignment.
    pub bounds: Option<[i32; 6]>,
    /// Visual entities (such as Paintings, Item Frames, Armor Stands) in continuous 3D space.
    #[cfg_attr(feature = "serde", serde(default))]
    pub entities: Vec<mtk_model::EntityInstance>,
}

impl VoxelPointCloud {
    /// Creates an empty `VoxelPointCloud`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty `VoxelPointCloud` with pre-allocated capacity for N points.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            positions: Vec::with_capacity(capacity * 3),
            block_x: Vec::with_capacity(capacity),
            block_y: Vec::with_capacity(capacity),
            block_z: Vec::with_capacity(capacity),
            state_palette: Vec::with_capacity(16),
            state_indices: Vec::with_capacity(capacity),
            biome_palette: Vec::with_capacity(16),
            biome_indices: Vec::with_capacity(capacity),
            entity_palette: Vec::new(),
            entity_indices: Vec::new(),
            light_levels: Vec::with_capacity(capacity),
            bounds: None,
            entities: Vec::new(),
        }
    }

    /// Returns the total number of voxel points stored in this cloud.
    #[inline]
    pub fn len(&self) -> usize {
        self.block_x.len()
    }

    /// Checks if the point cloud contains no voxel points.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.block_x.is_empty()
    }

    /// Checks if this point cloud carries any block entities.
    #[inline]
    pub fn has_entities(&self) -> bool {
        !self.entity_palette.is_empty()
    }

    /// Checks if this point cloud carries any continuous visual entities.
    #[inline]
    pub fn has_visual_entities(&self) -> bool {
        !self.entities.is_empty()
    }

    /// Exports continuous visual entities as a list of compact JSON strings (discrete palette format).
    ///
    /// Note on naming: While named `visual_entities_palette` to conform with the DCC bridge
    /// property schema where string array attributes are termed 'palettes', continuous visual
    /// entities do not share coordinates on the voxel grid. Instead, each entry in this list is
    /// an independent canonical JSON serialization of a distinct `EntityInstance`.
    #[cfg(feature = "serde")]
    pub fn visual_entities_palette(&self) -> Result<Vec<String>, serde_json::Error> {
        self.entities.iter().map(serde_json::to_string).collect()
    }

    /// Deserializes and updates visual entities from a discrete palette list atomically.
    #[cfg(feature = "serde")]
    pub fn set_visual_entities_palette(
        &mut self,
        palette: &[String],
    ) -> Result<(), serde_json::Error> {
        let mut temp = Vec::with_capacity(palette.len());
        for item in palette {
            if !item.trim().is_empty() {
                let ent: mtk_model::EntityInstance = serde_json::from_str(item)?;
                ent.validate().map_err(|e| {
                    <serde_json::Error as serde::de::Error>::custom(format!(
                        "validation failed: {e}"
                    ))
                })?;
                temp.push(ent);
            }
        }
        self.entities = temp;
        Ok(())
    }

    /// Appends a single voxel point into the flat arrays without block entity.
    pub fn push(
        &mut self,
        pos: [f32; 3],
        block_pos: [i32; 3],
        state: String,
        biome: String,
        light: u8,
    ) {
        self.push_with_entity(pos, block_pos, state, biome, light, None);
    }

    /// Appends a single voxel point with optional visual NBT JSON entity payload.
    pub fn push_with_entity(
        &mut self,
        pos: [f32; 3],
        block_pos: [i32; 3],
        state: String,
        biome: String,
        light: u8,
        entity: Option<&str>,
    ) {
        self.positions.extend_from_slice(&pos);
        self.block_x.push(block_pos[0]);
        self.block_y.push(block_pos[1]);
        self.block_z.push(block_pos[2]);
        let state_idx = self.intern_state(&state);
        self.state_indices.push(state_idx);
        let biome_idx = self.intern_biome(&biome);
        self.biome_indices.push(biome_idx);
        self.light_levels.push(light);

        if let Some(ent) = entity {
            if self.entity_indices.len() < self.block_x.len() - 1 {
                self.entity_indices.resize(self.block_x.len() - 1, -1);
            }
            let ent_idx = self.intern_entity(ent);
            self.entity_indices.push(ent_idx);
        } else if !self.entity_indices.is_empty() {
            self.entity_indices.push(-1);
        }
    }

    /// Interns a blockstate into the palette and returns its index.
    pub fn intern_state(&mut self, state: &str) -> i32 {
        intern_palette(&mut self.state_palette, state)
    }

    /// Interns a biome into the palette and returns its index.
    pub fn intern_biome(&mut self, biome: &str) -> i32 {
        intern_palette(&mut self.biome_palette, biome)
    }

    /// Interns an entity visual JSON into the entity palette and returns its index.
    pub fn intern_entity(&mut self, entity_json: &str) -> i32 {
        intern_palette(&mut self.entity_palette, entity_json)
    }

    /// Resolves the blockstate string for point `index`.
    #[inline]
    pub fn state_at(&self, index: usize) -> Option<&str> {
        self.state_indices
            .get(index)
            .and_then(|&idx| self.state_palette.get(idx.max(0) as usize))
            .map(String::as_str)
    }

    /// Resolves the biome string for point `index`.
    #[inline]
    pub fn biome_at(&self, index: usize) -> Option<&str> {
        self.biome_indices
            .get(index)
            .and_then(|&idx| self.biome_palette.get(idx.max(0) as usize))
            .map(String::as_str)
    }

    /// Resolves the optional block entity visual JSON string for point `index`.
    #[inline]
    pub fn entity_at(&self, index: usize) -> Option<&str> {
        let &idx = self.entity_indices.get(index)?;
        if idx >= 0 {
            self.entity_palette.get(idx as usize).map(String::as_str)
        } else {
            None
        }
    }

    /// Scans the point cloud's block palette for unknown blockstates lacking both models and textures.
    pub fn scan_unknown_blocks(
        &self,
        model_db: Option<&mtk_model::BakedModelDatabase>,
        atlas: Option<&mtk_texture::AtlasAddressMap>,
        custom_aliases: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Vec<String> {
        let unique_states: std::collections::HashSet<&str> =
            self.state_palette.iter().map(|s| s.as_str()).collect();
        crate::storage::scan_unknown_blockstates(unique_states, model_db, atlas, custom_aliases)
    }

    /// Efficient batch constructor from palette indices, avoiding allocating millions of strings on the host.
    pub fn from_indexed_arrays(
        positions: Vec<f32>,
        block_x: Vec<i32>,
        block_y: Vec<i32>,
        block_z: Vec<i32>,
        state_indices: &[i32],
        state_palette: &[String],
        biome_indices: Option<&[i32]>,
        biome_palette: Option<&[String]>,
        light_levels: Option<Vec<u8>>,
        bounds: Option<[i32; 6]>,
        entity_indices: Option<&[i32]>,
        entity_palette: Option<&[String]>,
        visual_entities: Option<&[String]>,
    ) -> Result<Self, VoxelError> {
        let count = block_x.len();
        if positions.len() != count * 3
            || block_y.len() != count
            || block_z.len() != count
            || state_indices.len() != count
        {
            return Err(VoxelError::InvalidPointCloud(format!(
                "Array lengths mismatch: positions={}, block_x={}, block_y={}, block_z={}, state_indices={}",
                positions.len(),
                block_x.len(),
                block_y.len(),
                block_z.len(),
                state_indices.len()
            )));
        }
        validate_palette_indices("state", state_indices, state_palette)?;

        let (biome_palette, biome_indices) = match (biome_indices, biome_palette) {
            (Some(indices), Some(palette)) => {
                if indices.len() != count {
                    return Err(VoxelError::InvalidPointCloud(format!(
                        "biome_indices length ({}) must match point count ({})",
                        indices.len(),
                        count
                    )));
                }
                validate_palette_indices("biome", indices, palette)?;
                (palette.to_vec(), indices.to_vec())
            }
            (Some(_), None) => {
                return Err(VoxelError::InvalidPointCloud(
                    "biome_indices requires a biome_palette".to_string(),
                ));
            }
            _ => (vec!["minecraft:plains".to_string()], vec![0; count]),
        };

        let (entity_palette, entity_indices) = match (entity_indices, entity_palette) {
            (Some(indices), Some(palette)) if !palette.is_empty() => {
                if indices.len() != count {
                    return Err(VoxelError::InvalidPointCloud(format!(
                        "entity_indices length ({}) must match point count ({})",
                        indices.len(),
                        count
                    )));
                }
                validate_entity_indices(indices, palette)?;
                (palette.to_vec(), indices.to_vec())
            }
            _ => (Vec::new(), Vec::new()),
        };

        #[allow(unused_mut)]
        let mut parsed_entities = Vec::new();
        if let Some(vis) = visual_entities {
            #[cfg(feature = "serde")]
            {
                for item in vis {
                    if !item.trim().is_empty() {
                        let ent: mtk_model::EntityInstance =
                            serde_json::from_str(item).map_err(|e| {
                                VoxelError::InvalidPointCloud(format!(
                                    "failed to parse visual entity: {e}"
                                ))
                            })?;
                        ent.validate().map_err(|e| {
                            VoxelError::InvalidPointCloud(format!("invalid visual entity: {e}"))
                        })?;
                        parsed_entities.push(ent);
                    }
                }
            }
            #[cfg(not(feature = "serde"))]
            {
                if !vis.is_empty() {
                    return Err(VoxelError::InvalidPointCloud(
                        "visual_entities requires the `serde` feature in mtk-voxel".to_string(),
                    ));
                }
            }
        }

        Ok(Self {
            positions,
            block_x,
            block_y,
            block_z,
            state_palette: state_palette.to_vec(),
            state_indices: state_indices.to_vec(),
            biome_palette,
            biome_indices,
            entity_palette,
            entity_indices,
            light_levels: light_levels.unwrap_or_else(|| vec![0u8; count]),
            bounds,
            entities: parsed_entities,
        })
    }

    /// Creates an abstract `VoxelSource` wrapping this point cloud.
    pub fn as_source(&self) -> PointCloudVoxelSource {
        PointCloudVoxelSource::from_point_cloud(self)
    }
}

pub(crate) fn intern_palette(palette: &mut Vec<String>, value: &str) -> i32 {
    if let Some(idx) = palette.iter().position(|entry| entry == value) {
        return idx as i32;
    }
    let idx = palette.len() as i32;
    palette.push(value.to_string());
    idx
}

pub(crate) fn palette_index(
    palette: &mut Vec<String>,
    map: &mut HashMap<String, i32>,
    value: &str,
) -> i32 {
    if let Some(&idx) = map.get(value) {
        return idx;
    }
    let idx = palette.len() as i32;
    palette.push(value.to_string());
    map.insert(value.to_string(), idx);
    idx
}

pub(crate) fn validate_palette_indices(
    label: &str,
    indices: &[i32],
    palette: &[String],
) -> Result<(), VoxelError> {
    if indices.is_empty() {
        return Ok(());
    }
    if palette.is_empty() {
        return Err(VoxelError::InvalidPointCloud(format!(
            "{label} indices reference an empty {label} palette"
        )));
    }
    if let Some(&bad) = indices
        .iter()
        .find(|&&idx| idx < 0 || idx as usize >= palette.len())
    {
        return Err(VoxelError::InvalidPointCloud(format!(
            "{label} index {bad} is out of bounds for {label} palette length {}",
            palette.len()
        )));
    }
    Ok(())
}

pub(crate) fn validate_entity_indices(
    indices: &[i32],
    palette: &[String],
) -> Result<(), VoxelError> {
    if indices.is_empty() {
        return Ok(());
    }
    if let Some(&bad) = indices
        .iter()
        .find(|&&idx| idx < -1 || (idx >= 0 && idx as usize >= palette.len()))
    {
        return Err(VoxelError::InvalidPointCloud(format!(
            "entity index {bad} is out of bounds for entity palette length {}",
            palette.len()
        )));
    }
    Ok(())
}
