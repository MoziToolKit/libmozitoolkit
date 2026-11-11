//! # High-Throughput Sparse Voxel Storage for Scene Interchange
//!
//! Provides a compact column-oriented (Structure-of-Arrays) voxel representation
//! with orthogonal discrete palettes (BlockState, Biome, BlockEntity/HangingEntity).
//! Designed specifically for the `VOXL` chunk in `.mtkscene` packages and zero-copy
//! bidirectional conversion with DCC companion point clouds (`VoxelPointCloud`).

use mtk_model::BlockEntityData;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::storage::point_cloud::VoxelPointCloud;
use crate::storage::world::VoxelStorage;
use crate::types::MesherConfig;

/// High-throughput sparse voxel container optimized for scene interchange (`.mtkscene`).
///
/// Combines spatial bounds, global deduplicated palettes (State, Biome, BlockEntity),
/// and column-oriented (Structure-of-Arrays) voxel data.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SparseVoxelPayload {
    /// Spatial bounding box [min_x, min_y, min_z, size_x, size_y, size_z].
    pub bounds: [i32; 6],
    /// Global blockstate palette.
    pub state_palette: Vec<String>,
    /// Global biome palette.
    pub biome_palette: Vec<String>,
    /// Canonical visual NBT block entity / hanging entity JSON strings palette.
    #[cfg_attr(feature = "serde", serde(default))]
    pub entity_palette: Vec<String>,
    /// Columnar sparse voxels.
    pub columns: SparseVoxelColumns,
}

/// Columnar arrays of sparse voxel coordinates and palette indices.
///
/// The contiguous memory layout provides optimal compression ratio under Zstd.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SparseVoxelColumns {
    /// World integer coordinates X.
    pub x: Vec<i32>,
    /// World integer coordinates Y.
    pub y: Vec<i32>,
    /// World integer coordinates Z.
    pub z: Vec<i32>,
    /// Palette index into `state_palette`.
    pub state_indices: Vec<u16>,
    /// Palette index into `biome_palette`.
    pub biome_indices: Vec<u16>,
    /// Entity palette index (-1 if none; >=0 into `entity_palette`).
    pub entity_indices: Vec<i32>,
    /// Optional combined light levels (sky nibble << 4 | block nibble).
    #[cfg_attr(feature = "serde", serde(default))]
    pub light_levels: Option<Vec<u8>>,
}

impl SparseVoxelPayload {
    /// Creates an empty sparse voxel payload.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty sparse voxel payload with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bounds: [0; 6],
            state_palette: Vec::with_capacity(16),
            biome_palette: Vec::with_capacity(4),
            entity_palette: Vec::new(),
            columns: SparseVoxelColumns {
                x: Vec::with_capacity(capacity),
                y: Vec::with_capacity(capacity),
                z: Vec::with_capacity(capacity),
                state_indices: Vec::with_capacity(capacity),
                biome_indices: Vec::with_capacity(capacity),
                entity_indices: Vec::with_capacity(capacity),
                light_levels: None,
            },
        }
    }

    /// Returns the total number of sparse voxels.
    #[inline]
    pub fn len(&self) -> usize {
        self.columns.x.len()
    }

    /// Returns whether the payload contains no voxels.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.columns.x.is_empty()
    }

    /// Appends a voxel point into the columns with typed block entity data.
    pub fn push(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        state: &str,
        biome: &str,
        entity: Option<&BlockEntityData>,
        light: Option<u8>,
    ) {
        let e_json = entity.map(|e| e.to_visual_json());
        self.push_with_entity_json(x, y, z, state, biome, e_json.as_deref(), light);
    }

    /// Appends a voxel point into the columns with raw visual NBT JSON entity payload.
    pub fn push_with_entity_json(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        state: &str,
        biome: &str,
        entity_json: Option<&str>,
        light: Option<u8>,
    ) {
        let s_idx = intern_string(&mut self.state_palette, state);
        let b_idx = intern_string(&mut self.biome_palette, biome);
        let e_idx = match entity_json {
            Some(json) => intern_string(&mut self.entity_palette, json) as i32,
            None => -1,
        };

        self.columns.x.push(x);
        self.columns.y.push(y);
        self.columns.z.push(z);
        self.columns.state_indices.push(s_idx as u16);
        self.columns.biome_indices.push(b_idx as u16);
        self.columns.entity_indices.push(e_idx);

        if let Some(lvl) = light {
            let l_vec = self
                .columns
                .light_levels
                .get_or_insert_with(|| vec![0; self.columns.x.len() - 1]);
            l_vec.push(lvl);
        } else if let Some(ref mut l_vec) = self.columns.light_levels {
            l_vec.push(0);
        }
    }

    /// Resolves the typed `BlockEntityData` for the voxel at `index` (if any).
    pub fn entity_data_at(&self, index: usize) -> Option<BlockEntityData> {
        let &e_idx = self.columns.entity_indices.get(index)?;
        if e_idx >= 0 {
            let json = self.entity_palette.get(e_idx as usize)?;
            BlockEntityData::from_visual_json(json)
        } else {
            None
        }
    }

    /// Constructs a `SparseVoxelPayload` from a `VoxelPointCloud` with zero parsing overhead.
    pub fn from_point_cloud(cloud: &VoxelPointCloud) -> Self {
        let count = cloud.len();
        let state_indices: Vec<u16> = cloud
            .state_indices
            .iter()
            .map(|&i| i.max(0) as u16)
            .collect();

        let biome_indices: Vec<u16> = cloud
            .biome_indices
            .iter()
            .map(|&i| i.max(0) as u16)
            .collect();

        let entity_indices = if cloud.entity_indices.len() == count {
            cloud.entity_indices.clone()
        } else {
            vec![-1; count]
        };

        let light_levels = if cloud.light_levels.len() == count {
            Some(cloud.light_levels.clone())
        } else {
            None
        };

        let bounds = cloud
            .bounds
            .unwrap_or_else(|| compute_bounds(&cloud.block_x, &cloud.block_y, &cloud.block_z));

        Self {
            bounds,
            state_palette: cloud.state_palette.clone(),
            biome_palette: cloud.biome_palette.clone(),
            entity_palette: cloud.entity_palette.clone(),
            columns: SparseVoxelColumns {
                x: cloud.block_x.clone(),
                y: cloud.block_y.clone(),
                z: cloud.block_z.clone(),
                state_indices,
                biome_indices,
                entity_indices,
                light_levels,
            },
        }
    }

    /// Converts this payload back into an authoritative `VoxelPointCloud` for DCC viewport display.
    pub fn to_point_cloud(&self, config: Option<&MesherConfig>) -> VoxelPointCloud {
        let count = self.len();
        let state_indices: Vec<i32> = self
            .columns
            .state_indices
            .iter()
            .map(|&idx| idx as i32)
            .collect();

        let biome_indices: Vec<i32> = self
            .columns
            .biome_indices
            .iter()
            .map(|&idx| idx as i32)
            .collect();

        let entity_indices = if self.columns.entity_indices.len() == count {
            self.columns.entity_indices.clone()
        } else {
            vec![-1; count]
        };

        let light_levels = self
            .columns
            .light_levels
            .clone()
            .unwrap_or_else(|| vec![0; count]);

        // Compute 3D positions with center offsets and mesher config transformations
        let mut positions = Vec::with_capacity(count * 3);
        let mut eff_config = config.cloned().unwrap_or_default();
        let (bx, by, bz, sx, sy, sz) = (
            self.bounds[0],
            self.bounds[1],
            self.bounds[2],
            self.bounds[3],
            self.bounds[4],
            self.bounds[5],
        );
        let has_bounds = sx > 0 && sy > 0 && sz > 0;
        if eff_config.origin_centered && eff_config.selection_bounds.is_none() {
            if has_bounds {
                eff_config.selection_bounds = Some(([bx, by, bz], [sx, sy, sz]));
            } else if !self.columns.x.is_empty() {
                let computed = compute_bounds(&self.columns.x, &self.columns.y, &self.columns.z);
                if computed[3] > 0 && computed[4] > 0 && computed[5] > 0 {
                    eff_config.selection_bounds = Some((
                        [computed[0], computed[1], computed[2]],
                        [computed[3], computed[4], computed[5]],
                    ));
                }
            }
        }

        for i in 0..count {
            let bx = self.columns.x[i];
            let by = self.columns.y[i];
            let bz = self.columns.z[i];
            let center = glam::Vec3::new(bx as f32 + 0.5, by as f32 + 0.5, bz as f32 + 0.5);
            let transformed = eff_config.transform_position(center);
            positions.push(transformed.x);
            positions.push(transformed.y);
            positions.push(transformed.z);
        }

        let effective_bounds = if has_bounds {
            self.bounds
        } else if !self.columns.x.is_empty() {
            compute_bounds(&self.columns.x, &self.columns.y, &self.columns.z)
        } else {
            self.bounds
        };

        VoxelPointCloud {
            positions,
            block_x: self.columns.x.clone(),
            block_y: self.columns.y.clone(),
            block_z: self.columns.z.clone(),
            state_palette: self.state_palette.clone(),
            state_indices,
            biome_palette: self.biome_palette.clone(),
            biome_indices,
            entity_palette: self.entity_palette.clone(),
            entity_indices,
            light_levels,
            bounds: Some(effective_bounds),
            entities: Vec::new(),
        }
    }

    /// Extracts sparse voxels directly from a `VoxelStorage` world.
    pub fn from_storage(storage: &VoxelStorage, config: &MesherConfig) -> Self {
        let cloud = VoxelPointCloud::extract_from_storage(storage, config);
        Self::from_point_cloud(&cloud)
    }

    /// Reconstructs a full `VoxelStorage` instance from this sparse voxel payload.
    pub fn reconstruct_storage(&self) -> VoxelStorage {
        let cloud = self.to_point_cloud(None);
        cloud.reconstruct_storage()
    }

    /// Serializes this sparse voxel payload to a JSON string.
    #[cfg(feature = "serde")]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserializes a sparse voxel payload from a JSON string.
    #[cfg(feature = "serde")]
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[inline]
fn intern_string(palette: &mut Vec<String>, item: &str) -> usize {
    if let Some(pos) = palette.iter().position(|p| p == item) {
        pos
    } else {
        let idx = palette.len();
        palette.push(item.to_string());
        idx
    }
}

fn compute_bounds(x: &[i32], y: &[i32], z: &[i32]) -> [i32; 6] {
    if x.is_empty() {
        return [0; 6];
    }
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut min_z = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    let mut max_z = i32::MIN;
    for i in 0..x.len() {
        min_x = min_x.min(x[i]);
        min_y = min_y.min(y[i]);
        min_z = min_z.min(z[i]);
        max_x = max_x.max(x[i]);
        max_y = max_y.max(y[i]);
        max_z = max_z.max(z[i]);
    }
    [
        min_x,
        min_y,
        min_z,
        (max_x - min_x + 1).max(0),
        (max_y - min_y + 1).max(0),
        (max_z - min_z + 1).max(0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_voxel_payload_push_and_roundtrip() {
        let mut payload = SparseVoxelPayload::new();
        payload.push(
            10,
            64,
            -20,
            "minecraft:stone",
            "minecraft:plains",
            None,
            Some(15),
        );
        payload.push(
            10,
            65,
            -20,
            "minecraft:white_banner[rotation=2]",
            "minecraft:plains",
            Some(&BlockEntityData::new_banner(
                "white",
                vec![("stripe_top", "blue")],
            )),
            Some(14),
        );
        payload.push(
            11,
            65,
            -20,
            "minecraft:decorated_pot[facing=north]",
            "minecraft:plains",
            Some(&BlockEntityData::new_decorated_pot([
                Some("angler"),
                None,
                None,
                None,
            ])),
            None,
        );

        assert_eq!(payload.len(), 3);
        assert_eq!(payload.state_palette.len(), 3);
        assert_eq!(payload.biome_palette.len(), 1);
        assert_eq!(payload.entity_palette.len(), 2);

        // Verify typed lookup
        let be1 = payload.entity_data_at(1).expect("Must find banner");
        match be1 {
            BlockEntityData::Banner { base_color, .. } => {
                assert_eq!(base_color, "white");
            }
            _ => panic!("Expected Banner"),
        }

        // Convert to point cloud and verify
        let cloud = payload.to_point_cloud(None);
        assert_eq!(cloud.len(), 3);
        assert_eq!(cloud.entity_palette.len(), 2);
        assert_eq!(cloud.state_at(0), Some("minecraft:stone"));
        assert_eq!(
            cloud.state_at(1),
            Some("minecraft:white_banner[rotation=2]")
        );
        assert!(cloud.entity_at(1).unwrap().contains("\"minecraft:banner\""));

        // Roundtrip from point cloud back to sparse payload
        let payload2 = SparseVoxelPayload::from_point_cloud(&cloud);
        assert_eq!(payload.len(), payload2.len());
        assert_eq!(payload.state_palette, payload2.state_palette);
        assert_eq!(payload.entity_palette, payload2.entity_palette);
        assert_eq!(payload.columns.x, payload2.columns.x);
        assert_eq!(payload.columns.y, payload2.columns.y);
        assert_eq!(payload.columns.z, payload2.columns.z);
    }

    #[test]
    fn test_sparse_voxel_json_serde() {
        let mut payload = SparseVoxelPayload::new();
        payload.bounds = [0, 0, 0, 16, 16, 16];
        payload.push(
            5,
            10,
            15,
            "minecraft:oak_sign[rotation=4]",
            "minecraft:the_end",
            Some(&BlockEntityData::new_sign(
                vec!["hello".to_string()],
                None::<&str>,
                false,
            )),
            Some(15),
        );

        let json = payload.to_json().expect("Serialize sparse payload to json");
        assert!(json.contains("minecraft:sign"));

        let deserialized = SparseVoxelPayload::from_json(&json).expect("Deserialize json");
        assert_eq!(payload, deserialized);
    }

    #[test]
    fn test_to_point_cloud_origin_centered_auto_bounds() {
        let mut payload = SparseVoxelPayload::new();
        payload.bounds = [10, 20, 30, 10, 10, 10]; // center_x = 10 + 5 = 15, base_y = 20, center_z = 30 + 5 = 35
        payload.push(
            10,
            20,
            30,
            "minecraft:stone",
            "minecraft:plains",
            None,
            None,
        );

        let config = MesherConfig {
            origin_centered: true,
            selection_bounds: None,
            coordinate_system: crate::types::CoordinateSystem::Minecraft,
            ..Default::default()
        };

        let cloud = payload.to_point_cloud(Some(&config));
        // pos.x = 10.5 - 15.0 = -4.5
        // pos.y = 20.5 - 20.0 = 0.5
        // pos.z = 30.5 - 35.0 = -4.5
        assert!((cloud.positions[0] - (-4.5)).abs() < 1e-4);
        assert!((cloud.positions[1] - 0.5).abs() < 1e-4);
        assert!((cloud.positions[2] - (-4.5)).abs() < 1e-4);
    }
}
