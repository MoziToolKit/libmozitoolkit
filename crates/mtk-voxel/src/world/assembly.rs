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
        // 32-bit on purpose: this runs once per vertex of the whole world and 64-bit `idiv` is
        // ~2-3x slower. Positions are world block coordinates, so the quantized key always fits.
        let stride = (SECTION_STRIDE * inv).round() as i32;
        // `transform_position` is `R(pos) + offset`; because `R` only permutes axes, Section
        // boundary planes (world coordinate multiples of 16) land on `offset` modulo 16.
        let offset = self.config.transform_position(glam::Vec3::ZERO);
        let phases = [
            (offset.x * inv).round() as i32,
            (offset.y * inv).round() as i32,
            (offset.z * inv).round() as i32,
        ];
        move |key: [i32; 3]| {
            if stride < 4 {
                return true;
            }
            key.iter().zip(phases.iter()).any(|(&k, &phase)| {
                let delta = (k.wrapping_sub(phase)).rem_euclid(stride);
                delta <= 1 || stride - delta <= 1
            })
        }
    }

    /// Re-assembles and welds the unified world mesh from the current section cache without cloning.
    ///
    /// Only Section seam vertices are hashed: `SectionMesher` already welds every Section
    /// internally (grid pool for unit cubes, `weld_vertices` for model geometry), so the only
    /// vertices that can coincide across Sections are the ones lying on a Section boundary plane.
    /// A full spatial weld produced the same output for strictly more work, and is what made a
    /// cold `rebuild_all` spend ~20% of its time in assembly.
    /// Stage timings are recorded when a profiler is supplied.
    pub(crate) fn assemble_world_mesh(
        &mut self,
        mut profiler: Option<&mut mtk_core::ExecutionProfiler>,
    ) {
        let collect_sorted_refs = || -> Vec<&MeshData> {
            let mut entries: Vec<(&glam::IVec3, &MeshData)> =
                self.section_mesh_cache.iter().collect();
            entries.sort_unstable_by_key(|(c, _)| (c.x, c.y, c.z));
            entries.into_iter().map(|(_, m)| m).collect()
        };
        let meshes: Vec<&MeshData> = match profiler.as_deref_mut() {
            Some(p) => p.measure_with_count(
                "collect_refs",
                "mesh",
                self.section_mesh_cache.len(),
                |_| collect_sorted_refs(),
            ),
            None => collect_sorted_refs(),
        };

        let weld = self.config.weld_vertices;
        let merge = || {
            if !weld {
                MeshData::merge_all_refs(&meshes)
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

        // `assemble_world_mesh` only records the raw atlas chunk IDs used by the merged mesh;
        // `merged.face_materials` intentionally stays as raw atlas chunk IDs (not compacted to
        // `0..N-1` slots), because `used_materials()`, `has_dynamic_entity_chunks()`, and host
        // `mtk_atlas_chunk_id` remapping all read raw chunk IDs from it.
        let finalize_world_mesh = |this: &mut Self, merged: MeshData| {
            this.used_chunk_ids = merged
                .used_materials()
                .into_iter()
                .map(|id| id as u32)
                .collect();
            this.world_mesh = Some(merged);
        };
        match profiler {
            Some(p) => p.measure("record_used_chunks", "mesh", |_| {
                finalize_world_mesh(self, merged)
            }),
            None => finalize_world_mesh(self, merged),
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

    /// The incremental path only welds Section seam vertices. `rebuild_all` used to do a full
    /// spatial weld of the whole world; this proves the cheap path produces the same mesh, which
    /// is what lets both paths share one implementation.
    #[test]
    fn seam_only_weld_matches_a_full_weld() {
        use crate::mesher::SectionMesher;
        use crate::storage::VoxelStorage;
        use crate::types::MesherConfig;
        use mtk_cull::FaceCuller;

        let mut storage = VoxelStorage::new();
        storage.set_bounds(0, 0, 0, 32, 16, 16);
        for x in 0..32 {
            for y in 0..16 {
                for z in 0..16 {
                    // Stone meets glass at x = 16: neither side culls the shared plane, so both
                    // Sections emit coincident vertices there and the weld must merge them.
                    let state = if x < 16 {
                        "minecraft:stone"
                    } else {
                        "minecraft:glass"
                    };
                    storage.set_block(x, y, z, state, None);
                }
            }
        }

        let config = MesherConfig::default();
        let culler = FaceCuller::default();
        let sections: Vec<mtk_core::MeshData> = storage
            .get_all_non_empty_sections()
            .into_iter()
            .map(|coord| {
                let padded = storage.get_section_padded_array(coord);
                SectionMesher::mesh_section(
                    &padded,
                    &culler,
                    |_| crate::mesher::ModelSource::None,
                    &config,
                )
            })
            .collect();
        assert!(sections.len() >= 2, "need two meshed sections");
        let refs: Vec<&mtk_core::MeshData> = sections.iter().collect();

        let world = VoxelWorld::from_storage(storage, Some(config), None, None);
        let seam = world.section_seam_predicate(1e-4);
        let full = mtk_core::MeshData::merge_welded_sections(&refs, 1e-4);
        let filtered = mtk_core::MeshData::merge_welded_sections_filtered(&refs, 1e-4, seam);

        assert!(
            full.positions.len() < refs.iter().map(|m| m.positions.len()).sum(),
            "the seam must actually weld something for this test to mean anything"
        );
        assert_eq!(full.positions, filtered.positions);
        assert_eq!(full.normals, filtered.normals);
        assert_eq!(full.indices, filtered.indices);
        assert_eq!(full.face_materials, filtered.face_materials);
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
