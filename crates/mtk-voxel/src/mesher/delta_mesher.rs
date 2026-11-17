use glam::IVec3;
use mtk_core::MeshData;
use mtk_cull::FaceCuller;

use crate::mesher::shading::ModelSource;
use crate::mesher::SectionMesher;
use crate::storage::VoxelStorage;
use crate::types::MesherConfig;

/// Incrementally rebuilds meshes only for dirty sections in a `VoxelStorage`.
pub struct DeltaMesher;

impl DeltaMesher {
    /// Re-meshes all currently dirty sections in `world` and clears their dirty state using a unified ModelSource lookup.
    pub fn rebuild_dirty_sections_with_source<'a, F>(
        world: &mut VoxelStorage,
        culler: &FaceCuller,
        mut model_provider: F,
        config: &MesherConfig,
    ) -> Vec<(IVec3, MeshData)>
    where
        F: FnMut(&str) -> ModelSource<'a>,
    {
        let dirty_coords: Vec<IVec3> = world.dirty_sections.iter().copied().collect();
        if dirty_coords.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::with_capacity(dirty_coords.len());

        for coord in dirty_coords {
            let padded = world.get_section_padded_array(coord);
            if !padded.is_empty {
                let mesh =
                    SectionMesher::mesh_section(&padded, culler, &mut model_provider, config);
                results.push((coord, mesh));
            } else {
                results.push((coord, MeshData::new()));
            }
        }

        world.clear_dirty_sections();
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_mesher_rebuild() {
        let mut world = VoxelStorage::new();
        world.set_bounds(0, 0, 0, 16, 16, 16);
        world.set_block(0, 0, 0, "minecraft:stone", None);

        let culler = FaceCuller::default();
        let config = MesherConfig::default();

        let meshes = DeltaMesher::rebuild_dirty_sections_with_source(
            &mut world,
            &culler,
            |_| ModelSource::None,
            &config,
        );
        // The edit sits on the shared corner of the 8 sections around it, and the mesher
        // samples all of them through the 18^3 padded array, so all 8 are rebuilt; only the
        // section that actually owns the block has geometry.
        assert_eq!(meshes.len(), 8);
        assert_eq!(
            meshes.iter().filter(|(_, mesh)| !mesh.is_empty()).count(),
            1
        );
        assert!(world.dirty_sections.is_empty());
    }
}
