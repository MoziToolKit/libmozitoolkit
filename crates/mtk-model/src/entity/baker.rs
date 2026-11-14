#![forbid(unsafe_code)]
//! Procedural entity baking engine for continuous-space visual entities.
//!
//! Converts high-level `EntityInstance` items (Paintings, Item Frames, Armor Stands)
//! into standalone local-space `MeshData` buffers, completely decoupled from
//! discrete voxel block grids and chunk meshing.

use mtk_core::MeshData;

use crate::baker::baked_model::ModelMeshOptions;
use crate::baker::BakedModel;
use crate::entity::instance::{EntityInstance, EntityKindData};
use crate::entity::{item_frame, painting};

/// Standalone procedural baker for continuous-space visual entities.
pub struct EntityBaker;

impl EntityBaker {
    /// Bakes an `EntityInstance` into an untextured or raw-named local `BakedModel`.
    pub fn bake_model(entity: &EntityInstance) -> Option<BakedModel> {
        match &entity.kind {
            EntityKindData::Painting(p) => Some(painting::create_painting_baked_model(
                &p.variant,
                p.width as u32,
                p.height as u32,
            )),
            EntityKindData::ItemFrame(f) => Some(item_frame::create_item_frame_baked_model(
                f.is_glowing,
                f.is_invisible,
            )),
            EntityKindData::ArmorStand(_) => {
                // Armor Stand procedural baked model (stub ready for expansion)
                None
            }
            EntityKindData::Generic { .. } => None,
        }
    }

    /// Bakes an `EntityInstance` into a local-space `MeshData` buffer with optional atlas UV remapping.
    ///
    /// The returned `MeshData` is strictly in local model coordinates (centered at (0,0,0)).
    /// Spatial translation, rotation, and scaling are applied by the DCC host via Object Transform.
    pub fn bake_entity_mesh<F>(
        entity: &EntityInstance,
        mut atlas_lookup: F,
    ) -> Option<(MeshData, Vec<String>)>
    where
        F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
    {
        if entity.validate().is_err() {
            return None;
        }

        let mut model = Self::bake_model(entity)?;
        model.remap_to_atlas_with(&mut atlas_lookup);

        let options = ModelMeshOptions {
            clip_hidden_volume: false,
            cull_duplicates: true,
            cull_coplanar_opposite: true,
            tolerance: 1e-4,
        };
        let (mut mesh, textures) = model.to_mesh_with_options(&options);

        // Ensure standard corner AO attribute is available (1.0 for unshaded entities)
        let loop_count = mesh.loop_count();
        if loop_count > 0 {
            use mtk_core::attribute_constants::ATTR_AO;
            use mtk_core::{AttributeData, AttributeDomain, MeshAttribute};
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_AO,
                AttributeDomain::Corner,
                AttributeData::Float(vec![1.0; loop_count]),
            ));
        }

        Some((mesh, textures))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DVec3, Vec3};
    use mtk_core::Direction;

    #[test]
    fn test_entity_baker_outputs_local_space_mesh() {
        let painting = EntityInstance::new_painting(
            None,
            DVec3::new(10.5, 64.0, -20.5),
            Direction::North,
            Vec3::ZERO,
            "kebab",
            1,
            1,
            Some([10, 64, -20]),
        );
        let (mesh, textures) =
            EntityBaker::bake_entity_mesh(&painting, |_tex| Some(([0.0, 0.0, 0.5, 0.5], 3, 42)))
                .expect("Bake succeeds");

        // 1x1 painting has exactly 6 faces (front, back, 4 edges) -> 24 vertices, 36 indices
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.indices.len(), 36);

        // Textures must include the painting variant and back texture
        assert_eq!(textures.len(), 2);
        assert!(textures.iter().any(|t| t == "minecraft:painting/kebab"));
        assert!(textures.iter().any(|t| t == "minecraft:painting/back"));

        // Verify local space bounding box: centered at x=0, y=0, z within [-0.03125, 0.03125]
        for pos in &mesh.positions {
            assert!(pos[0] >= -0.5 - 1e-4 && pos[0] <= 0.5 + 1e-4);
            assert!(pos[1] >= -0.5 - 1e-4 && pos[1] <= 0.5 + 1e-4);
            assert!(pos[2] >= -0.03125 - 1e-4 && pos[2] <= 0.03125 + 1e-4);
        }

        // Verify normals are unit length and non-zero
        for n in &mesh.normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-3,
                "Normal must be normalized, got len={len}"
            );
        }

        // Verify UVs are within the expected atlas remapped range [0.0, 0.5]
        for uv in &mesh.uvs {
            assert!(uv[0] >= -1e-4 && uv[0] <= 0.5 + 1e-4);
            assert!(uv[1] >= -1e-4 && uv[1] <= 0.5 + 1e-4);
        }

        // Verify standard AO attribute exists and contains all 1.0
        use mtk_core::attribute_constants::ATTR_AO;
        let ao_attr = mesh
            .get_custom_attribute(ATTR_AO)
            .expect("AO attribute exists");
        if let mtk_core::AttributeData::Float(ref vals) = ao_attr.data {
            assert_eq!(vals.len(), mesh.loop_count());
            for &val in vals {
                assert!((val - 1.0).abs() < 1e-5);
            }
        } else {
            panic!("Expected Float data for AO");
        }
    }
}
