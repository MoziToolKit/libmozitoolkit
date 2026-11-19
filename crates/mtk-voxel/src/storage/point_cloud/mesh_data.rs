//! Conversion between `VoxelPointCloud` and `mtk_core::MeshData`.

use mtk_core::attribute_constants::{
    ATTR_BIOME_IDX, ATTR_BIOME_PALETTE, ATTR_BLOCK_X, ATTR_BLOCK_Y, ATTR_BLOCK_Z, ATTR_BOUNDS,
    ATTR_ENTITY_IDX, ATTR_ENTITY_PALETTE, ATTR_LIGHT, ATTR_PALETTE, ATTR_STATE_IDX,
};
use mtk_core::MeshData;
use mtk_core::{AttributeData, AttributeDomain, MeshAttribute};

use crate::types::VoxelError;

use super::{validate_entity_indices, validate_palette_indices, VoxelPointCloud};

impl VoxelPointCloud {
    /// Converts this `VoxelPointCloud` into a pure-point `MeshData` with standard
    /// attributes attached on the `Point` domain (without edges or polygons).
    pub fn to_mesh_data(&self) -> MeshData {
        self.clone().into_mesh_data()
    }

    /// Converts this owned `VoxelPointCloud` into a pure-point `MeshData`, moving
    /// internal attribute buffers directly without cloning.
    pub fn into_mesh_data(self) -> MeshData {
        let count = self.len();
        let (chunks, _) = self.positions.as_chunks::<3>();
        let positions = chunks.to_vec();
        let mut mesh = MeshData {
            positions,
            normals: Vec::new(),
            indices: Vec::new(),
            uvs: Vec::new(),
            uv_layout: mtk_core::UvLayout::None,
            secondary_uvs: None,
            colors: None,
            face_materials: Vec::new(),
            face_tint_indices: Vec::new(),
            quad_indices: None,
            custom_attributes: std::collections::HashMap::new(),
        };

        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BLOCK_X,
            AttributeDomain::Point,
            AttributeData::Int32(self.block_x),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BLOCK_Y,
            AttributeDomain::Point,
            AttributeData::Int32(self.block_y),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BLOCK_Z,
            AttributeDomain::Point,
            AttributeData::Int32(self.block_z),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_STATE_IDX,
            AttributeDomain::Point,
            AttributeData::Int32(self.state_indices),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_PALETTE,
            AttributeDomain::Mesh,
            AttributeData::String(self.state_palette),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BIOME_IDX,
            AttributeDomain::Point,
            AttributeData::Int32(self.biome_indices),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BIOME_PALETTE,
            AttributeDomain::Mesh,
            AttributeData::String(self.biome_palette),
        ));

        if !self.entity_palette.is_empty() && self.entity_indices.len() == count {
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_ENTITY_IDX,
                AttributeDomain::Point,
                AttributeData::Int32(self.entity_indices),
            ));
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_ENTITY_PALETTE,
                AttributeDomain::Mesh,
                AttributeData::String(self.entity_palette),
            ));
        }

        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_LIGHT,
            AttributeDomain::Point,
            AttributeData::UInt8(self.light_levels),
        ));

        if let Some(b) = self.bounds {
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_BOUNDS,
                AttributeDomain::Mesh,
                AttributeData::Int32(b.to_vec()),
            ));
        }

        mesh
    }

    /// Reconstructs a `VoxelPointCloud` from a `MeshData` containing Point-domain attributes.
    pub fn from_mesh_data(mesh: &MeshData) -> Result<Self, VoxelError> {
        let count = mesh.positions.len();
        if count == 0 {
            return Ok(Self::new());
        }

        let mut positions = Vec::with_capacity(count * 3);
        for p in &mesh.positions {
            positions.extend_from_slice(p);
        }

        let block_x = mesh
            .get_custom_attribute(ATTR_BLOCK_X)
            .and_then(|a| match &a.data {
                AttributeData::Int32(v) if v.len() == count => Some(v.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                VoxelError::InvalidPointCloud(format!(
                    "Missing or malformed {ATTR_BLOCK_X} attribute"
                ))
            })?;

        let block_y = mesh
            .get_custom_attribute(ATTR_BLOCK_Y)
            .and_then(|a| match &a.data {
                AttributeData::Int32(v) if v.len() == count => Some(v.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                VoxelError::InvalidPointCloud(format!(
                    "Missing or malformed {ATTR_BLOCK_Y} attribute"
                ))
            })?;

        let block_z = mesh
            .get_custom_attribute(ATTR_BLOCK_Z)
            .and_then(|a| match &a.data {
                AttributeData::Int32(v) if v.len() == count => Some(v.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                VoxelError::InvalidPointCloud(format!(
                    "Missing or malformed {ATTR_BLOCK_Z} attribute"
                ))
            })?;

        let state_indices = mesh
            .get_custom_attribute(ATTR_STATE_IDX)
            .and_then(|a| match &a.data {
                AttributeData::Int32(v) if v.len() == count => Some(v.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                VoxelError::InvalidPointCloud(format!(
                    "Missing or malformed {ATTR_STATE_IDX} attribute"
                ))
            })?;

        let state_palette = mesh
            .get_custom_attribute(ATTR_PALETTE)
            .and_then(|a| match &a.data {
                AttributeData::String(v) if !v.is_empty() => Some(v.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                VoxelError::InvalidPointCloud(format!(
                    "Missing or malformed {ATTR_PALETTE} attribute"
                ))
            })?;
        validate_palette_indices("state", &state_indices, &state_palette)?;

        let biome_indices = mesh
            .get_custom_attribute(ATTR_BIOME_IDX)
            .and_then(|a| match &a.data {
                AttributeData::Int32(v) if v.len() == count => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec![0; count]);
        let biome_palette = mesh
            .get_custom_attribute(ATTR_BIOME_PALETTE)
            .and_then(|a| match &a.data {
                AttributeData::String(v) if !v.is_empty() => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec!["minecraft:plains".to_string()]);
        validate_palette_indices("biome", &biome_indices, &biome_palette)?;

        let (entity_palette, entity_indices) =
            if let Some(p_attr) = mesh.get_custom_attribute(ATTR_ENTITY_PALETTE) {
                if let AttributeData::String(p) = &p_attr.data {
                    let indices = mesh
                        .get_custom_attribute(ATTR_ENTITY_IDX)
                        .and_then(|a| match &a.data {
                            AttributeData::Int32(v) if v.len() == count => Some(v.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| vec![-1; count]);
                    validate_entity_indices(&indices, p)?;
                    (p.clone(), indices)
                } else {
                    (Vec::new(), Vec::new())
                }
            } else {
                (Vec::new(), Vec::new())
            };

        let light_levels = mesh
            .get_custom_attribute(ATTR_LIGHT)
            .and_then(|a| match &a.data {
                AttributeData::UInt8(v) if v.len() == count => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec![0u8; count]);

        let bounds = mesh
            .get_custom_attribute(ATTR_BOUNDS)
            .and_then(|a| match &a.data {
                AttributeData::Int32(v) if v.len() == 6 => {
                    Some([v[0], v[1], v[2], v[3], v[4], v[5]])
                }
                _ => None,
            });

        Ok(Self {
            positions,
            block_x,
            block_y,
            block_z,
            state_palette,
            state_indices,
            biome_palette,
            biome_indices,
            entity_palette,
            entity_indices,
            light_levels,
            bounds,
        })
    }
}
