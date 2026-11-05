use mtk_voxel::types::MesherConfig;
use mtk_voxel::VoxelWorld;

#[test]
fn test_voxel_world_lifecycle_and_used_chunks() {
    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        ..Default::default()
    };

    let mut world = VoxelWorld::new(Some(config), None, None);
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(0, 0, 0, "minecraft:stone", None);
    world.set_block(1, 0, 0, "minecraft:dirt", None);

    let mesh = world.rebuild_all().unwrap();
    assert!(!mesh.is_empty());
    assert!(mesh.vertex_count() > 0);

    // Initial used chunks (unit cubes default to chunk 0 without atlas)
    let used_chunks = world.used_chunk_ids();
    assert_eq!(used_chunks, &[0]);

    // Test incremental delta modification
    world.set_block(0, 0, 0, "minecraft:air", None);
    let rebuilt = world.rebuild_dirty().unwrap();
    assert_eq!(rebuilt.len(), 1);

    let updated_mesh = world.get_world_mesh().unwrap();
    assert!(!updated_mesh.is_empty());

    // Test compact materials
    let mapping = world.compact_world_mesh_materials();
    assert_eq!(mapping, vec![0]);
}

#[test]
fn test_voxel_world_rebuild_with_progress() {
    use mtk_core::ProgressReport;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let mut world = VoxelWorld::new(None, None, None);
    world.set_bounds(0, 0, 0, 32, 16, 16);
    world.set_block(0, 0, 0, "minecraft:stone", None);
    world.set_block(16, 0, 0, "minecraft:stone", None);

    let progress_events = Arc::new(AtomicUsize::new(0));
    let pe_clone = Arc::clone(&progress_events);

    let cb = move |r: ProgressReport| {
        pe_clone.fetch_add(1, Ordering::SeqCst);
        assert!(r.current <= r.total);
    };

    let mesh = world.rebuild_all_with_progress(Some(&cb)).unwrap();
    assert!(!mesh.is_empty());
    // Should have reported progress for meshing and assembly
    assert!(progress_events.load(Ordering::SeqCst) >= 2);
}

#[test]
fn test_voxel_world_clear_cache_and_hot_reload() {
    let mut world = VoxelWorld::new(None, None, None);
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(0, 0, 0, "minecraft:stone", None);

    let mesh = world.rebuild_all().unwrap();
    assert!(!mesh.is_empty());
    assert!(world.get_world_mesh().is_some());
    assert!(!world.get_section_cache().is_empty());
    assert!(world.storage.dirty_sections.is_empty());

    // Clear cache
    world.clear_cache();
    assert!(world.get_world_mesh().is_none());
    assert!(world.get_section_cache().is_empty());
    assert!(world.used_chunk_ids().is_empty());
    assert!(!world.storage.dirty_sections.is_empty());

    // Incremental or full remesh rebuilds everything
    let rebuilt = world.rebuild_dirty().unwrap();
    assert_eq!(rebuilt.len(), 1);
    assert!(world.get_world_mesh().is_some());
    assert!(!world.get_section_cache().is_empty());

    // Hot reload assets also purges mesh cache
    world.hot_reload_assets(None, None, None);
    assert!(world.get_world_mesh().is_none());
    assert!(world.get_section_cache().is_empty());
    assert!(!world.storage.dirty_sections.is_empty());
}

#[test]
fn test_voxel_world_incremental_empty_section_removes_cache() {
    let mut world = VoxelWorld::new(None, None, None);
    world.set_bounds(0, 0, 0, 32, 16, 16);
    // Section 0 has 1 block, Section 1 has 1 block
    world.set_block(0, 0, 0, "minecraft:stone", None);
    world.set_block(16, 0, 0, "minecraft:stone", None);

    let mesh = world.rebuild_all().unwrap();
    assert!(!mesh.is_empty());
    assert_eq!(world.get_section_cache().len(), 2);

    // Completely empty Section 0 by setting to air
    world.set_block(0, 0, 0, "minecraft:air", None);
    assert!(world
        .storage
        .dirty_sections
        .contains(&glam::IVec3::new(0, 0, 0)));

    let rebuilt = world.rebuild_dirty().unwrap();
    // DeltaMesher must return Section 0 as empty MeshData
    assert!(rebuilt
        .iter()
        .any(|(coord, m)| *coord == glam::IVec3::new(0, 0, 0) && m.is_empty()));

    // Crucial check: section_mesh_cache must now ONLY contain Section 1 (Section 0 removed!)
    assert_eq!(world.get_section_cache().len(), 1);
    assert!(!world
        .get_section_cache()
        .contains_key(&glam::IVec3::new(0, 0, 0)));
    assert!(world
        .get_section_cache()
        .contains_key(&glam::IVec3::new(1, 0, 0)));

    // World mesh must only have geometry from Section 1 (welded cube = 8 vertices)
    let unified = world.get_world_mesh().unwrap();
    assert_eq!(unified.vertex_count(), 8);
}

fn canonical_vertex_positions(mesh: &mtk_core::MeshData) -> Vec<[u32; 3]> {
    let mut positions: Vec<[u32; 3]> = mesh
        .positions
        .iter()
        .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
        .collect();
    positions.sort_unstable();
    positions
}

fn assert_no_duplicate_vertex_positions(mesh: &mtk_core::MeshData) {
    let mut seen = std::collections::HashSet::new();
    for p in &mesh.positions {
        let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
        assert!(
            seen.insert(key),
            "duplicate vertex position survived assembly: {p:?}"
        );
    }
}

#[test]
fn test_incremental_rebuild_preserves_cross_section_seam_welding() {
    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        ..Default::default()
    };

    let mut world = VoxelWorld::new(Some(config), None, None);
    world.set_bounds(0, 0, 0, 32, 16, 16);
    // Two cubes facing each other across the x = 16 Section seam. Their shared face is
    // culled, and the four rim vertices on the seam must be welded into one manifold.
    world.set_block(15, 0, 0, "minecraft:stone", None);
    world.set_block(16, 0, 0, "minecraft:stone", None);

    let full = world.rebuild_all().unwrap();
    assert_eq!(full.vertex_count(), 12);
    assert_eq!(full.face_count(), 10);
    assert_no_duplicate_vertex_positions(full);

    // An unrelated edit inside Section 0 forces the incremental (dirty) assembly path.
    world.set_block(0, 5, 5, "minecraft:dirt", None);
    let rebuilt = world.rebuild_dirty().unwrap();
    assert_eq!(rebuilt.len(), 1);

    let incremental = world.get_world_mesh().unwrap().clone();
    // Two welded cubes (12) + one welded dirt cube (8).
    assert_eq!(incremental.vertex_count(), 20);
    assert_eq!(incremental.face_count(), 16);
    assert_no_duplicate_vertex_positions(&incremental);

    // Incremental assembly must agree with a from-scratch rebuild.
    let full_again = world.rebuild_all().unwrap().clone();
    assert_eq!(incremental.vertex_count(), full_again.vertex_count());
    assert_eq!(incremental.face_count(), full_again.face_count());
    assert_eq!(
        canonical_vertex_positions(&incremental),
        canonical_vertex_positions(&full_again)
    );
    assert_no_duplicate_vertex_positions(&full_again);
}

#[test]
fn test_incremental_seam_welding_with_unaligned_selection_bounds() {
    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        ..Default::default()
    };

    let mut world = VoxelWorld::new(Some(config), None, None);
    // Selection origin is not Section-aligned: the origin-centering offset is fractional
    // relative to the 16-block Section grid, so the seam predicate must still find x = 16.
    world.set_bounds(5, 64, 5, 32, 16, 16);
    world.set_block(15, 64, 5, "minecraft:stone", None);
    world.set_block(16, 64, 5, "minecraft:stone", None);

    let full = world.rebuild_all().unwrap();
    assert_eq!(full.vertex_count(), 12);
    assert_no_duplicate_vertex_positions(full);

    world.set_block(5, 70, 10, "minecraft:dirt", None);
    world.rebuild_dirty().unwrap();
    let incremental = world.get_world_mesh().unwrap();
    assert_eq!(incremental.vertex_count(), 20);
    assert_no_duplicate_vertex_positions(incremental);
}

#[test]
fn test_voxel_world_manifest_export_import() {
    let mut world = VoxelWorld::new(None, None, None);
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(0, 0, 0, "minecraft:stone", None);
    world
        .storage
        .calculate_and_store_section_crc(glam::IVec3::new(0, 0, 0));
    world.rebuild_all().unwrap();

    let meta = world.export_manifest_metadata();
    assert_eq!(meta["size_x"], 16);

    let mut restored = VoxelWorld::new(None, None, None);
    assert!(restored.import_manifest_metadata(&meta));
    let (min_x, _, _, sz_x, _, _) = restored.storage.get_bounds();
    assert_eq!(min_x, 0);
    assert_eq!(sz_x, 16);
    assert_eq!(restored.storage.section_crc_map.len(), 1);
}
