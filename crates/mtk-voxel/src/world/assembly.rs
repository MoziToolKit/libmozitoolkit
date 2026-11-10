//! # World Mesh Assembly and Seam Welding
//!
//! Provides manifold mesh assembly from cached chunk sections, incremental
//! seam welding along 16-block interfaces, and static animation attribute pruning.

use mtk_core::MeshData;

use super::VoxelWorld;

impl VoxelWorld {
    /// Builds the predicate used by incremental assembly to select the only vertices that can
    /// be shared by two Section meshes: positions lying on a 16-block Section boundary plane.
    ///
    /// `SectionMesher` already welds every Section internally, so incremental rebuilds only need
    /// to stitch the interfaces. Hashing just those vertices keeps delta remeshing fast while
    /// restoring the manifold seams that the full rebuild already produces.
    pub(crate) fn section_seam_predicate(&self, tolerance: f32) -> impl Fn([i32; 3]) -> bool + '_ {
        const SECTION_STRIDE: f32 = mtk_core::constants::voxel::SECTION_SIZE as f32;
        let inv = 1.0 / tolerance;
        let stride = (SECTION_STRIDE * inv).round() as i64;
        // `transform_position` is `R(pos) + offset`; because `R` only permutes axes, Section
        // boundary planes (world coordinate multiples of 16) land on `offset` modulo 16.
        let offset = self.config.transform_position(glam::Vec3::ZERO);
        let phases = [
            (offset.x * inv).round() as i64,
            (offset.y * inv).round() as i64,
            (offset.z * inv).round() as i64,
        ];
        move |key: [i32; 3]| {
            if stride < 4 {
                return true;
            }
            key.iter().zip(phases.iter()).any(|(&k, &phase)| {
                let delta = (k as i64 - phase).rem_euclid(stride);
                delta <= 1 || stride - delta <= 1
            })
        }
    }

    /// Re-assembles and welds the unified world mesh from the current section cache without cloning.
    /// Incremental mode (`full_weld = false`) keeps the fast path by welding only Section seam
    /// vertices, since each Section mesh is already internally welded by `SectionMesher`. Global
    /// rebuilds (`rebuild_all`) perform a full spatial weld. Stage timings are recorded when a
    /// profiler is supplied.
    pub(crate) fn assemble_world_mesh(
        &mut self,
        full_weld: bool,
        mut profiler: Option<&mut mtk_core::ExecutionProfiler>,
    ) {
        let meshes: Vec<&MeshData> = match profiler.as_deref_mut() {
            Some(p) => p.measure_with_count(
                "collect_refs",
                "mesh",
                self.section_mesh_cache.len(),
                |_| self.section_mesh_cache.values().collect(),
            ),
            None => self.section_mesh_cache.values().collect(),
        };

        let weld = self.config.weld_vertices;
        let merge = || {
            if !weld {
                MeshData::merge_all_refs(&meshes)
            } else if full_weld {
                MeshData::merge_welded_sections(&meshes, 1e-4)
            } else {
                let seam = self.section_seam_predicate(1e-4);
                MeshData::merge_welded_sections_filtered(&meshes, 1e-4, seam)
            }
        };
        let mut merged = match profiler.as_deref_mut() {
            Some(p) => p.measure("merge_welded_seams", "mesh", |_| merge()),
            None => merge(),
        };
        prune_static_animation_attributes(&mut merged);

        let compact = |this: &mut Self, merged: MeshData| {
            this.used_chunk_ids = merged
                .used_materials()
                .into_iter()
                .map(|id| id as u32)
                .collect();
            this.world_mesh = Some(merged);
        };
        match profiler {
            Some(p) => p.measure("compact_material_slots", "mesh", |_| compact(self, merged)),
            None => compact(self, merged),
        }
    }
}

/// Removes face-level animation attributes when the merged world contains no
/// animated faces. Section meshes intentionally carry a homogeneous attribute
/// set so merging never drops per-face data; this is the only safe level for
/// optional attribute pruning.
pub(crate) fn prune_static_animation_attributes(mesh: &mut MeshData) {
    let static_only = mesh
        .get_custom_attribute(mtk_core::attribute_constants::ATTR_ANIM_TIMING)
        .and_then(|attr| match &attr.data {
            mtk_core::AttributeData::Float3(values) => {
                Some(values.iter().all(|timing| timing[0] <= 1.0))
            }
            _ => None,
        })
        .unwrap_or(false);
    if static_only {
        mesh.remove_custom_attribute(mtk_core::attribute_constants::ATTR_ANIM_TIMING);
        mesh.remove_custom_attribute(mtk_core::attribute_constants::ATTR_ANIM_FRAME_SIZE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesher::collector::FaceAttributesCollector;
    use mtk_core::attribute_constants::{ATTR_ANIM_FRAME_SIZE, ATTR_ANIM_TIMING};

    fn mesh_with_timing(timing: [f32; 3]) -> MeshData {
        let mut collector = FaceAttributesCollector::with_capacity(1);
        collector.push_face(
            "minecraft:block/stone",
            0,
            [1.0, 1.0, 0.0, 0.0],
            0.0,
            [1.0, 0.0, 0.0, 0.0],
            [[1.0, 1.0, 1.0, 1.0]; 4],
            glam::IVec3::ZERO,
            timing,
            [16.0, 16.0, 0.0],
        );
        let mut mesh = MeshData::new();
        collector.attach_to_mesh(&mut mesh);
        mesh
    }

    #[test]
    fn mixed_static_and_animated_sections_merge_and_prune() {
        let static_mesh = mesh_with_timing([1.0, 1.0, 0.0]);
        let animated_mesh = mesh_with_timing([4.0, 2.0, 1.0]);
        let mut merged = MeshData::merge_all_refs(&[&static_mesh, &animated_mesh]);
        assert_eq!(
            merged
                .get_custom_attribute(ATTR_ANIM_TIMING)
                .expect("merged animation attribute")
                .len(),
            2
        );
        prune_static_animation_attributes(&mut merged);
        assert!(merged.get_custom_attribute(ATTR_ANIM_TIMING).is_some());
        assert!(merged.get_custom_attribute(ATTR_ANIM_FRAME_SIZE).is_some());
    }

    #[test]
    fn static_world_prunes_animation_attributes() {
        let a = mesh_with_timing([1.0, 1.0, 0.0]);
        let b = mesh_with_timing([1.0, 1.0, 0.0]);
        let mut merged = MeshData::merge_all_refs(&[&a, &b]);
        prune_static_animation_attributes(&mut merged);
        assert!(merged.get_custom_attribute(ATTR_ANIM_TIMING).is_none());
        assert!(merged.get_custom_attribute(ATTR_ANIM_FRAME_SIZE).is_none());
    }
}
