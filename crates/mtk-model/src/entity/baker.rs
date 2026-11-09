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

        let face_count = mesh.face_count();
        if face_count > 0 {
            use mtk_core::attribute_constants::{
                ATTR_AO, ATTR_BLOCK_X, ATTR_BLOCK_Y, ATTR_BLOCK_Z,
            };
            use mtk_core::{AttributeData, AttributeDomain, MeshAttribute};

            let bx = entity.position.x.floor() as i32;
            let by = entity.position.y.floor() as i32;
            let bz = entity.position.z.floor() as i32;
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_BLOCK_X,
                AttributeDomain::Face,
                AttributeData::Int32(vec![bx; face_count]),
            ));
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_BLOCK_Y,
                AttributeDomain::Face,
                AttributeData::Int32(vec![by; face_count]),
            ));
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_BLOCK_Z,
                AttributeDomain::Face,
                AttributeData::Int32(vec![bz; face_count]),
            ));

            let loop_count = mesh.loop_count();
            if loop_count > 0 {
                mesh.add_custom_attribute(MeshAttribute::new(
                    ATTR_AO,
                    AttributeDomain::Corner,
                    AttributeData::Float(vec![1.0; loop_count]),
                ));
            }
        }

        Some((mesh, textures))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DVec3, Vec2, Vec3};
    use mtk_core::attribute_constants::{
        ATTR_ANIM_FRAME_SIZE, ATTR_ANIM_TIMING, ATTR_AO, ATTR_ATLAS_CHUNK_ID, ATTR_BLOCK_X,
        ATTR_BLOCK_Y, ATTR_BLOCK_Z,
    };
    use mtk_core::{AttributeData, AttributeDomain, FaceAttributes, MeshAttribute, Quad};

    #[test]
    fn test_entity_baker_merges_cleanly_into_voxel_world_mesh() {
        let mut world_mesh = MeshData::new();
        let quad = Quad::new(
            [
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            [
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
        );
        let mut attr = FaceAttributes::new("minecraft:block/stone", 0);
        attr.atlas_chunk_id = Some(0);
        attr.block_pos = [1, 2, 3];
        attr.anim_timing = [4.0, 2.0, 1.0];
        attr.anim_frame_size = [16.0, 16.0, 0.0];
        world_mesh.append_quad(&quad, &attr);
        world_mesh.populate_standard_face_attributes(&[attr], true);
        world_mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_AO,
            AttributeDomain::Corner,
            AttributeData::Float(vec![0.8; 4]),
        ));

        let painting = EntityInstance::new_painting(
            DVec3::new(-8.0, -59.0, 10.0),
            Direction::East,
            "kebab",
            1,
            1,
        );
        let (ent_mesh, _) =
            EntityBaker::bake_entity_mesh(&painting, |_tex| Some(([0.0, 0.0, 0.5, 0.5], 3, 42)))
                .expect("Should bake painting entity mesh");

        assert_eq!(ent_mesh.face_count(), 6);
        assert_eq!(ent_mesh.used_materials(), vec![3]);
        assert_eq!(ent_mesh.quad_indices.as_ref().map(|q| q.len()), Some(24));

        world_mesh.append_mesh(&ent_mesh);

        assert_eq!(world_mesh.face_count(), 7);
        assert_eq!(world_mesh.quad_indices.as_ref().map(|q| q.len()), Some(28));
        assert_eq!(world_mesh.used_materials(), vec![0, 3]);
        assert_eq!(
            world_mesh
                .get_custom_attribute(ATTR_ATLAS_CHUNK_ID)
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            world_mesh.get_custom_attribute(ATTR_BLOCK_X).unwrap().len(),
            7
        );
        assert_eq!(
            world_mesh.get_custom_attribute(ATTR_BLOCK_Y).unwrap().len(),
            7
        );
        assert_eq!(
            world_mesh.get_custom_attribute(ATTR_BLOCK_Z).unwrap().len(),
            7
        );
        assert_eq!(
            world_mesh
                .get_custom_attribute(ATTR_ANIM_TIMING)
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            world_mesh
                .get_custom_attribute(ATTR_ANIM_FRAME_SIZE)
                .unwrap()
                .len(),
            7
        );
        assert_eq!(world_mesh.get_custom_attribute(ATTR_AO).unwrap().len(), 28);
    }
}
