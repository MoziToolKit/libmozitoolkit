//! # Face Attributes Collector
//!
//! High-throughput collector for canonical per-face attributes needed for Atlas
//! materials, UV tiling transforms, and dynamic Biome Tinting.

use glam::IVec3;
use mtk_core::attribute_constants::*;
use mtk_core::MeshData;
use mtk_core::{AttributeData, AttributeDomain, MeshAttribute};

#[derive(Debug, Clone, Default)]
pub struct FaceAttributesCollector {
    pub source_texture_palette: Vec<String>,
    pub source_texture_indices: Vec<u16>,
    pub atlas_chunk_ids: Vec<i32>,
    pub uv_transforms: Vec<[f32; 4]>,
    pub uv_rotations: Vec<f32>,
    pub biome_tint_data: Vec<[f32; 4]>,
    pub biome_tint_colors: Vec<[f32; 4]>,
    pub block_xs: Vec<i32>,
    pub block_ys: Vec<i32>,
    pub block_zs: Vec<i32>,
    pub anim_timings: Vec<[f32; 3]>,
    pub anim_frame_sizes: Vec<[f32; 3]>,
    pub ao_values: Vec<f32>,
    pub last_texture_idx: Option<u16>,
}

impl FaceAttributesCollector {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            source_texture_palette: Vec::with_capacity(16),
            source_texture_indices: Vec::with_capacity(capacity),
            atlas_chunk_ids: Vec::with_capacity(capacity),
            uv_transforms: Vec::with_capacity(capacity),
            uv_rotations: Vec::with_capacity(capacity),
            biome_tint_data: Vec::with_capacity(capacity),
            biome_tint_colors: Vec::with_capacity(capacity),
            block_xs: Vec::with_capacity(capacity),
            block_ys: Vec::with_capacity(capacity),
            block_zs: Vec::with_capacity(capacity),
            anim_timings: Vec::with_capacity(capacity),
            anim_frame_sizes: Vec::with_capacity(capacity),
            ao_values: Vec::with_capacity(capacity * 4),
            last_texture_idx: None,
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn push_face(
        &mut self,
        source_texture_key: &str,
        atlas_chunk_id: i32,
        uv_transform: [f32; 4],
        uv_rotation: f32,
        tint_data: [f32; 4],
        tint_color: [f32; 4],
        block_pos: IVec3,
        anim_timing: [f32; 3],
        anim_frame_size: [f32; 3],
    ) {
        let pal_idx = if let Some(last_idx) = self.last_texture_idx {
            if self
                .source_texture_palette
                .get(last_idx as usize)
                .map(|s| s.as_str())
                == Some(source_texture_key)
            {
                last_idx
            } else if let Some(idx) = self
                .source_texture_palette
                .iter()
                .position(|k| k == source_texture_key)
            {
                let u = idx as u16;
                self.last_texture_idx = Some(u);
                u
            } else {
                let idx = self.source_texture_palette.len() as u16;
                self.source_texture_palette
                    .push(source_texture_key.to_string());
                self.last_texture_idx = Some(idx);
                idx
            }
        } else if let Some(idx) = self
            .source_texture_palette
            .iter()
            .position(|k| k == source_texture_key)
        {
            let u = idx as u16;
            self.last_texture_idx = Some(u);
            u
        } else {
            let idx = self.source_texture_palette.len() as u16;
            self.source_texture_palette
                .push(source_texture_key.to_string());
            self.last_texture_idx = Some(idx);
            idx
        };
        self.source_texture_indices.push(pal_idx);
        self.atlas_chunk_ids.push(atlas_chunk_id);
        self.uv_transforms.push(uv_transform);
        self.uv_rotations.push(uv_rotation);
        self.biome_tint_data.push(tint_data);
        self.biome_tint_colors.push(tint_color);
        self.block_xs.push(block_pos.x);
        self.block_ys.push(block_pos.y);
        self.block_zs.push(block_pos.z);
        self.anim_timings.push(anim_timing);
        self.anim_frame_sizes.push(anim_frame_size);
    }

    pub fn attach_to_mesh(self, mesh: &mut MeshData) {
        if self.source_texture_indices.is_empty() {
            return;
        }

        // Provenance is emitted strictly as `palette + indices` (Golden Rule 8).
        // The legacy per-face `ATTR_SOURCE_TEXTURE_KEY` string layer is listed in
        // `mtk_core::RETIRED_MESH_ATTRIBUTES` and must never be written
        // again: it cost one heap-allocated String per face and every host had to
        // purge it right after ingestion.
        let indices_i32: Vec<i32> = self
            .source_texture_indices
            .iter()
            .map(|&i| i as i32)
            .collect();
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_SOURCE_TEXTURE_IDX,
            AttributeDomain::Face,
            AttributeData::Int32(indices_i32),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_SOURCE_TEXTURE_PALETTE,
            AttributeDomain::Mesh,
            AttributeData::String(self.source_texture_palette.clone()),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_ATLAS_CHUNK_ID,
            AttributeDomain::Face,
            AttributeData::Int32(self.atlas_chunk_ids),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_UV_TRANSFORM,
            AttributeDomain::Face,
            AttributeData::Float4(self.uv_transforms),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_UV_ROTATION,
            AttributeDomain::Face,
            AttributeData::Float(self.uv_rotations),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BIOME_TINT_DATA,
            AttributeDomain::Face,
            AttributeData::Float4(self.biome_tint_data),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BIOME_TINT_COLOR,
            AttributeDomain::Face,
            AttributeData::Float4(self.biome_tint_colors),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BLOCK_X,
            AttributeDomain::Face,
            AttributeData::Int32(self.block_xs),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BLOCK_Y,
            AttributeDomain::Face,
            AttributeData::Int32(self.block_ys),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_BLOCK_Z,
            AttributeDomain::Face,
            AttributeData::Int32(self.block_zs),
        ));
        // Keep the attribute set homogeneous across every Section. Static
        // sections carry default timing values; the merged world mesh prunes
        // these attributes once if nothing in the world is animated.
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_ANIM_TIMING,
            AttributeDomain::Face,
            AttributeData::Float3(self.anim_timings),
        ));
        mesh.add_custom_attribute(MeshAttribute::new(
            ATTR_ANIM_FRAME_SIZE,
            AttributeDomain::Face,
            AttributeData::Float3(self.anim_frame_sizes),
        ));
        if !self.ao_values.is_empty() {
            mesh.add_custom_attribute(MeshAttribute::new(
                ATTR_AO,
                AttributeDomain::Corner,
                AttributeData::Float(self.ao_values),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_core::attribute_constants::{
        ATTR_ANIM_FRAME_SIZE, ATTR_ANIM_TIMING, ATTR_BLOCK_X, ATTR_BLOCK_Y, ATTR_BLOCK_Z,
    };

    fn collector_with(anim_timing: [f32; 3]) -> FaceAttributesCollector {
        let mut collector = FaceAttributesCollector::with_capacity(1);
        collector.push_face(
            "minecraft:block/stone",
            0,
            [1.0, 1.0, 0.0, 0.0],
            0.0,
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            glam::IVec3::new(1, 2, 3),
            anim_timing,
            [16.0, 16.0, 0.0],
        );
        collector
    }

    #[test]
    fn static_faces_keep_animation_attributes_for_merge() {
        let mut mesh = MeshData::new();
        collector_with([1.0, 1.0, 0.0]).attach_to_mesh(&mut mesh);

        assert_eq!(
            mesh.get_custom_attribute(ATTR_ANIM_TIMING)
                .expect("static section must carry default timing")
                .len(),
            1
        );
        assert!(mesh.get_custom_attribute(ATTR_ANIM_FRAME_SIZE).is_some());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_X).is_some());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_Y).is_some());
        assert!(mesh.get_custom_attribute(ATTR_BLOCK_Z).is_some());
    }

    #[test]
    fn static_faces_keep_animation_attributes() {
        let mut mesh = MeshData::new();
        collector_with([4.0, 2.0, 1.0]).attach_to_mesh(&mut mesh);

        assert!(mesh.get_custom_attribute(ATTR_ANIM_TIMING).is_some());
        assert!(mesh.get_custom_attribute(ATTR_ANIM_FRAME_SIZE).is_some());
    }
}
