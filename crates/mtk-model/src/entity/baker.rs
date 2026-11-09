#![forbid(unsafe_code)]
//! Procedural entity baking engine for continuous-space visual entities.
//!
//! Converts high-level `EntityInstance` items (Paintings, Item Frames, Armor Stands)
//! into standalone world-space `MeshData` buffers, completely decoupled from
//! discrete voxel block grids and `SectionMesher`.

use mtk_core::{Direction, MeshData};

use crate::baker::baked_model::ModelMeshOptions;
use crate::baker::BakedModel;
use crate::entity::instance::{EntityInstance, EntityKindData};
use crate::entity::painting;

/// Standalone procedural baker for continuous-space visual entities.
pub struct EntityBaker;

impl EntityBaker {
    /// Bakes an `EntityInstance` into an untextured or raw-named `BakedModel`.
    pub fn bake_model(entity: &EntityInstance) -> Option<BakedModel> {
        match &entity.kind {
            EntityKindData::Painting(p) => {
                let facing = entity.facing.unwrap_or(Direction::North);
                Some(painting::create_painting_baked_model_with_size(
                    &p.variant,
                    facing,
                    p.width as u32,
                    p.height as u32,
                ))
            }
            EntityKindData::ItemFrame(_) => {
                // Item Frame procedural baked model (stub ready for expansion)
                None
            }
            EntityKindData::ArmorStand(_) => {
                // Armor Stand procedural baked model (stub ready for expansion)
                None
            }
            EntityKindData::Generic { .. } => None,
        }
    }

    /// Bakes an `EntityInstance` into a world-space `MeshData` buffer with optional atlas UV remapping.
    pub fn bake_entity_mesh<F>(
        entity: &EntityInstance,
        mut atlas_lookup: F,
    ) -> Option<(MeshData, Vec<String>)>
    where
        F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
    {
        let mut model = Self::bake_model(entity)?;
        model.remap_to_atlas_with(&mut atlas_lookup);

        let options = ModelMeshOptions {
            clip_hidden_volume: false,
            cull_duplicates: true,
            cull_coplanar_opposite: true,
            tolerance: 1e-4,
        };
        let (mut mesh, textures) = model.to_mesh_with_options(&options);

        // Apply world translation based on entity continuous coordinate
        let off_x = entity.position.x as f32;
        let off_y = entity.position.y as f32;
        let off_z = entity.position.z as f32;
        for pos in &mut mesh.positions {
            pos[0] += off_x;
            pos[1] += off_y;
            pos[2] += off_z;
        }

        Some((mesh, textures))
    }
}
