//! # Baked Model Representation and Database
//!
//! Provides data structures, smart BlockState variant lookup, mesh conversion,
//! and backward-compatible serialization for baked block and OBJ models.

pub mod binary;
pub mod database;
pub mod model;
pub mod to_mesh;
pub mod variant_group;

pub use database::*;
pub use model::*;
pub use variant_group::*;

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec2, Vec3};
    use mtk_core::Direction;
    use std::collections::HashMap;

    #[test]
    fn test_baked_model_to_mesh() {
        let mut faces = HashMap::new();
        faces.insert(
            Direction::Up,
            BakedFace {
                direction: Direction::Up,
                texture: "minecraft:block/stone".to_string(),
                vertices: [
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ],
                uvs: [
                    Vec2::new(0.0, 0.0),
                    Vec2::new(0.0, 1.0),
                    Vec2::new(1.0, 1.0),
                    Vec2::new(1.0, 0.0),
                ],
                normal: Vec3::Y,
                ..Default::default()
            },
        );

        let model = BakedModel {
            block_state: "minecraft:stone".to_string(),
            elements: vec![BakedElement {
                from_pos: [0.0, 0.0, 0.0],
                to_pos: [16.0, 16.0, 16.0],
                faces,
            }],
            obj_faces: Vec::new(),
            faces: [
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
            ],
            is_cube: true,
            is_opaque: true,
            is_emissive: false,
            cull_meta: None,
            culled_faces: Default::default(),
            unculled_faces: Default::default(),
        };

        let mesh = model.to_mesh(false);
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.face_count(), 1);
        assert_eq!(mesh.face_materials[0], 0);

        // Verify standard attributes
        use mtk_core::attribute_constants::*;
        assert!(mesh.has_custom_attribute(ATTR_SOURCE_TEXTURE_IDX));
        assert!(mesh.has_custom_attribute(ATTR_SOURCE_TEXTURE_PALETTE));
        assert!(mesh.has_custom_attribute(ATTR_UV_TRANSFORM));

        let pal_attr = mesh
            .get_custom_attribute(ATTR_SOURCE_TEXTURE_PALETTE)
            .unwrap();
        if let mtk_core::AttributeData::String(ref vals) = pal_attr.data {
            assert_eq!(vals[0], "minecraft:block/stone");
        } else {
            panic!("Expected String attribute data");
        }

        let idx_attr = mesh.get_custom_attribute(ATTR_SOURCE_TEXTURE_IDX).unwrap();
        if let mtk_core::AttributeData::Int32(ref vals) = idx_attr.data {
            assert_eq!(vals[0], 0);
        } else {
            panic!("Expected Int32 attribute data");
        }
    }

    #[test]
    fn test_smart_blockstate_lookup() {
        let mut db = BakedModelDatabase::new();

        let dummy_model = |state: &str| BakedModel {
            block_state: state.to_string(),
            elements: Vec::new(),
            obj_faces: Vec::new(),
            faces: [
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
                BakedFace::default(),
            ],
            is_cube: false,
            is_opaque: false,
            is_emissive: false,
            cull_meta: None,
            culled_faces: Default::default(),
            unculled_faces: Default::default(),
        };

        db.insert(
            "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]".to_string(),
            dummy_model("minecraft:oak_stairs[facing=east,half=bottom,shape=straight]"),
        );
        db.insert(
            "minecraft:smooth_stone_slab[type=bottom]".to_string(),
            dummy_model("minecraft:smooth_stone_slab[type=bottom]"),
        );
        db.insert(
            "minecraft:chest".to_string(),
            dummy_model("minecraft:chest"),
        );
        db.insert(
            "minecraft:red_bed[facing=north,part=foot]".to_string(),
            dummy_model("minecraft:red_bed[facing=north,part=foot]"),
        );

        // Tier 1: Exact match
        assert!(db.get("minecraft:chest").is_some());
        assert!(db.get("minecraft:smooth_stone_slab[type=bottom]").is_some());

        // Tier 2: Stripping non-geometric properties (waterlogged, occupied)
        let stairs_query =
            "minecraft:oak_stairs[facing=east,half=bottom,shape=straight,waterlogged=false]";
        let slab_query = "minecraft:smooth_stone_slab[type=bottom,waterlogged=false]";
        let bed_query = "minecraft:red_bed[facing=north,occupied=false,part=foot]";

        let stairs_found = db.get(stairs_query);
        assert!(
            stairs_found.is_some(),
            "Stairs with waterlogged=false must match"
        );
        assert_eq!(
            stairs_found.unwrap().block_state,
            "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]"
        );

        let slab_found = db.get(slab_query);
        assert!(
            slab_found.is_some(),
            "Slab with waterlogged=false must match"
        );
        assert_eq!(
            slab_found.unwrap().block_state,
            "minecraft:smooth_stone_slab[type=bottom]"
        );

        let bed_found = db.get(bed_query);
        assert!(bed_found.is_some(), "Bed with occupied=false must match");
        assert_eq!(
            bed_found.unwrap().block_state,
            "minecraft:red_bed[facing=north,part=foot]"
        );

        // Tier 3/4: Fallback for chest with state properties to base chest model
        let chest_query = "minecraft:chest[facing=south,type=single,waterlogged=false]";
        let chest_found = db.get(chest_query);
        assert!(
            chest_found.is_some(),
            "Chest with properties must fallback to base chest model"
        );
        assert_eq!(chest_found.unwrap().block_state, "minecraft:chest");

        // Relaxed fallback: flower_amount=1 falling back to closest variant
        db.insert(
            "minecraft:wildflowers[flower_amount=2]".to_string(),
            dummy_model("minecraft:wildflowers[flower_amount=2]"),
        );
        let wf_found = db.get("minecraft:wildflowers[flower_amount=1]");
        assert!(
            wf_found.is_some(),
            "Wildflowers flower_amount=1 must fallback to closest variant"
        );
        assert_eq!(
            wf_found.unwrap().block_state,
            "minecraft:wildflowers[flower_amount=2]"
        );
    }
}
