//! # Standard Face Attributes Population
//!
//! Converts slices of `FaceAttributes` into strongly-typed `MeshAttribute` layers on `MeshData`.

use super::MeshData;
use crate::attributes::constants::*;
use crate::attributes::{
    AttributeData, AttributeDomain, FaceAttributes, MeshAttribute, MeshAttributeSpec,
    MESH_ATTRIBUTE_SPECS,
};

impl MeshData {
    /// Populates standard Face attributes from an ordered slice of `FaceAttributes`.
    ///
    /// `include_block_coords` is opt-in: standalone baked-model meshes have no
    /// meaningful world position, while voxel/world meshes do.
    pub fn populate_standard_face_attributes(
        &mut self,
        face_attrs: &[FaceAttributes],
        include_block_coords: bool,
    ) {
        let face_count = face_attrs.len();
        if face_count == 0 {
            return;
        }

        let mut palette = Vec::new();
        let mut tex_indices = Vec::with_capacity(face_count);
        let mut uv_transforms = Vec::with_capacity(face_count);
        let mut uv_rotations = Vec::with_capacity(face_count);
        let mut anim_timings = Vec::with_capacity(face_count);
        let mut anim_sizes = Vec::with_capacity(face_count);
        let mut tint_colors = Vec::with_capacity(face_count);
        let mut tint_datas = Vec::with_capacity(face_count);
        let mut block_x = Vec::with_capacity(face_count);
        let mut block_y = Vec::with_capacity(face_count);
        let mut block_z = Vec::with_capacity(face_count);
        let mut chunk_ids = Vec::with_capacity(face_count);
        let mut has_chunk_id = false;
        let mut has_animation = false;

        for attr in face_attrs {
            let key = &attr.texture_key;
            let idx = match palette.iter().position(|k| k == key) {
                Some(i) => i as i32,
                None => {
                    let i = palette.len() as i32;
                    palette.push(key.clone());
                    i
                }
            };
            tex_indices.push(idx);
            uv_transforms.push(attr.uv_transform);
            uv_rotations.push(attr.uv_rotation);
            anim_timings.push(attr.anim_timing);
            anim_sizes.push(attr.anim_frame_size);
            has_animation |= attr.anim_timing[0] > 1.0;
            tint_colors.push(attr.biome_tint_color);
            tint_datas.push(attr.biome_tint_data);
            if include_block_coords {
                block_x.push(attr.block_pos[0]);
                block_y.push(attr.block_pos[1]);
                block_z.push(attr.block_pos[2]);
            }

            if let Some(cid) = attr.atlas_chunk_id {
                has_chunk_id = true;
                chunk_ids.push(cid as i32);
            } else {
                chunk_ids.push(0);
            }
        }

        self.add_custom_attribute(MeshAttribute::new(
            ATTR_SOURCE_TEXTURE_IDX,
            AttributeDomain::Face,
            AttributeData::Int32(tex_indices),
        ));
        self.add_custom_attribute(MeshAttribute::new(
            ATTR_SOURCE_TEXTURE_PALETTE,
            AttributeDomain::Mesh,
            AttributeData::String(palette),
        ));
        self.add_custom_attribute(MeshAttribute::new(
            ATTR_UV_TRANSFORM,
            AttributeDomain::Face,
            AttributeData::Float4(uv_transforms),
        ));
        self.add_custom_attribute(MeshAttribute::new(
            ATTR_UV_ROTATION,
            AttributeDomain::Face,
            AttributeData::Float(uv_rotations),
        ));
        if has_animation {
            self.add_custom_attribute(MeshAttribute::new(
                ATTR_ANIM_TIMING,
                AttributeDomain::Face,
                AttributeData::Float3(anim_timings),
            ));
            self.add_custom_attribute(MeshAttribute::new(
                ATTR_ANIM_FRAME_SIZE,
                AttributeDomain::Face,
                AttributeData::Float3(anim_sizes),
            ));
        }
        self.add_custom_attribute(MeshAttribute::new(
            ATTR_BIOME_TINT_COLOR,
            AttributeDomain::Face,
            AttributeData::Float4(tint_colors),
        ));
        self.add_custom_attribute(MeshAttribute::new(
            ATTR_BIOME_TINT_DATA,
            AttributeDomain::Face,
            AttributeData::Float4(tint_datas),
        ));
        if include_block_coords {
            self.add_custom_attribute(MeshAttribute::new(
                ATTR_BLOCK_X,
                AttributeDomain::Face,
                AttributeData::Int32(block_x),
            ));
            self.add_custom_attribute(MeshAttribute::new(
                ATTR_BLOCK_Y,
                AttributeDomain::Face,
                AttributeData::Int32(block_y),
            ));
            self.add_custom_attribute(MeshAttribute::new(
                ATTR_BLOCK_Z,
                AttributeDomain::Face,
                AttributeData::Int32(block_z),
            ));
        }

        if has_chunk_id {
            self.add_custom_attribute(MeshAttribute::new(
                ATTR_ATLAS_CHUNK_ID,
                AttributeDomain::Face,
                AttributeData::Int32(chunk_ids),
            ));
        }

        debug_assert!(
            self.custom_attributes
                .keys()
                .all(|name| attribute_spec(name).is_some()),
            "populate_standard_face_attributes emitted an attribute missing from MESH_ATTRIBUTE_SPECS"
        );
    }
}

/// Looks up a canonical attribute declaration by name.
///
/// Returns `None` for retired or unknown attributes; hosts must not write those.
pub fn attribute_spec(name: &str) -> Option<&'static MeshAttributeSpec> {
    MESH_ATTRIBUTE_SPECS.iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attributes::RETIRED_MESH_ATTRIBUTES;

    #[test]
    fn test_populate_standard_face_attributes_stays_within_schema() {
        let attrs = vec![FaceAttributes::new("minecraft:block/stone", 0)];
        let mut mesh = MeshData::new();
        mesh.populate_standard_face_attributes(&attrs, false);

        for name in mesh.custom_attributes.keys() {
            assert!(
                attribute_spec(name).is_some(),
                "attribute {name} is not declared in MESH_ATTRIBUTE_SPECS"
            );
        }
        for name in RETIRED_MESH_ATTRIBUTES {
            assert!(
                mesh.get_custom_attribute(name).is_none(),
                "retired attribute {name} must not be emitted"
            );
        }
    }

    #[test]
    fn test_standalone_faces_skip_world_and_animation_attributes() {
        let attrs = vec![FaceAttributes::new("minecraft:block/stone", 0)];
        let mut mesh = MeshData::new();
        mesh.populate_standard_face_attributes(&attrs, false);

        assert!(mesh.get_custom_attribute(ATTR_BLOCK_X).is_none());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_Y).is_none());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_Z).is_none());
        assert!(mesh.get_custom_attribute(ATTR_ANIM_TIMING).is_none());
        assert!(mesh.get_custom_attribute(ATTR_ANIM_FRAME_SIZE).is_none());
    }

    #[test]
    fn test_world_faces_keep_block_coordinates() {
        let mut attr = FaceAttributes::new("minecraft:block/stone", 0);
        attr.block_pos = [1, 2, 3];
        let mut mesh = MeshData::new();
        mesh.populate_standard_face_attributes(&[attr], true);

        assert!(mesh.get_custom_attribute(ATTR_BLOCK_X).is_some());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_Y).is_some());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_Z).is_some());
    }
}
