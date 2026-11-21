//! Regression test for Live Sync incremental remeshing.
//!
//! The baseline path rebuilds the whole world (full spatial weld), but live deltas go
//! through `check_debounced_remesh` → `VoxelWorld::rebuild_dirty`. That incremental path
//! used to skip cross-Section welding entirely, silently splitting every previously
//! welded chunk seam after the first block edit.
//!
//! The invariant is "the incremental mesh equals a full rebuild of the same world", not "no two
//! vertices share a position": welding keys on `(position, normal)`, so a corner shared by three
//! differently oriented faces legitimately has three vertices.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use crossbeam_channel::unbounded;
use mtk_sync::unstable::testing::{check_debounced_remesh, SessionState};
use mtk_sync::{SyncEvent, SyncLifecycle};
use mtk_voxel::{MesherConfig, VoxelWorld};

/// Sorted `(position, normal)` pairs: policy independent, so it catches a split seam (which shows
/// up as extra vertices) without hardcoding how welding keys vertices.
fn vertex_key_set(mesh: &mtk_core::MeshData) -> Vec<([u32; 3], [u32; 3])> {
    let mut keys: Vec<([u32; 3], [u32; 3])> = mesh
        .positions
        .iter()
        .zip(mesh.normals.iter())
        .map(|(p, n)| {
            (
                [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()],
                [n[0].to_bits(), n[1].to_bits(), n[2].to_bits()],
            )
        })
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn test_live_delta_remesh_keeps_cross_section_seams_welded() {
    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        ..Default::default()
    };

    let mut world = VoxelWorld::new(Some(config), None, None);
    world.set_bounds(0, 0, 0, 32, 16, 16);
    // Two cubes facing each other across the x = 16 Section seam.
    world.set_block(15, 0, 0, "minecraft:stone", None);
    world.set_block(16, 0, 0, "minecraft:stone", None);

    let baseline = world.rebuild_all().unwrap().clone();
    // 10 visible faces, welded per (corner, face direction).
    assert_eq!(baseline.face_count(), 10);
    assert_eq!(baseline.vertex_count(), 32);

    // A live DELTA_OP touches one block in Section 0 and schedules a dirty remesh.
    world.apply_delta_update(0, 0, 0, &[(0, 5, 5, "minecraft:dirt")]);

    let world = Arc::new(RwLock::new(world));
    let mut state = SessionState::new();
    state.lifecycle = SyncLifecycle::Live;
    state.pending_dirty_rebuild = true;
    state.last_mesh_time = None;

    let (event_sender, event_receiver) = unbounded();
    let generation = Arc::new(AtomicU64::new(7));
    check_debounced_remesh(&mut state, &world, &event_sender, &generation);

    let mesh = {
        let guard = world.read().unwrap();
        guard.get_world_mesh().unwrap().clone()
    };
    // The delta path must reproduce the topology a full rebuild of the same world produces.
    let mut reference = VoxelWorld::new(
        Some(MesherConfig {
            weld_vertices: true,
            origin_centered: true,
            ..Default::default()
        }),
        None,
        None,
    );
    reference.set_bounds(0, 0, 0, 32, 16, 16);
    reference.set_block(15, 0, 0, "minecraft:stone", None);
    reference.set_block(16, 0, 0, "minecraft:stone", None);
    reference.set_block(0, 5, 5, "minecraft:dirt", None);
    let expected = reference.rebuild_all().unwrap().clone();

    assert_eq!(mesh.face_count(), 16);
    assert_eq!(mesh.face_count(), expected.face_count());
    assert_eq!(
        mesh.vertex_count(),
        expected.vertex_count(),
        "seams were split"
    );
    assert_eq!(
        vertex_key_set(&mesh),
        vertex_key_set(&expected),
        "seams were split"
    );

    let mut emitted = None;
    while let Ok(event) = event_receiver.try_recv() {
        if let SyncEvent::WorldMeshReady {
            mesh, generation, ..
        } = event
        {
            emitted = Some((mesh, generation));
        }
    }
    let (emitted_mesh, emitted_generation) =
        emitted.expect("delta remesh must emit WorldMeshReady");
    assert_eq!(emitted_generation, 8);
    assert_eq!(emitted_mesh.vertex_count(), expected.vertex_count());
    assert_eq!(vertex_key_set(&emitted_mesh), vertex_key_set(&expected));
}

#[test]
fn test_debounced_remesh_quiet_window_atomic_with_block_entity() {
    use std::time::{Duration, Instant};

    let mut world = VoxelWorld::new(None, None, None);
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(0, 0, 0, "minecraft:stone", None);
    let _ = world.rebuild_all().unwrap();

    let world = Arc::new(RwLock::new(world));
    let mut state = SessionState::new();
    state.lifecycle = SyncLifecycle::Live;
    state.pending_dirty_rebuild = true;
    state.last_delta_at = Some(Instant::now()); // Delta just arrived!
    state.dirty_since = Some(Instant::now());

    let (event_sender, event_receiver) = unbounded();
    let generation = Arc::new(AtomicU64::new(1));

    // 1. Immediately after delta arrived (quiet window not passed), remesh must NOT fire.
    check_debounced_remesh(&mut state, &world, &event_sender, &generation);
    assert!(
        state.pending_dirty_rebuild,
        "Debounce must hold dirty state when quiet window has not elapsed"
    );
    assert!(
        event_receiver.try_recv().is_err(),
        "No WorldMeshReady should be emitted during quiet debounce window"
    );

    // 2. Attach the companion BlockEntity to section
    {
        let mut w = world.write().unwrap();
        if let Some(sec) = w.storage.sections.get_mut(&glam::IVec3::ZERO) {
            sec.set_block_entity(
                0,
                Some(mtk_model::BlockEntityData::DecoratedPot {
                    sherds: [
                        Some("minecraft:miner_pottery_sherd".to_string()),
                        None,
                        None,
                        None,
                    ],
                }),
            );
            w.storage.dirty_sections.insert(glam::IVec3::ZERO);
        }
    }

    // 3. Simulate quiet window expiring (> 30ms)
    state.last_delta_at = Some(Instant::now() - Duration::from_millis(35));
    check_debounced_remesh(&mut state, &world, &event_sender, &generation);

    // 4. Now remesh should have fired and cleared pending_dirty_rebuild
    assert!(
        !state.pending_dirty_rebuild,
        "Dirty flag must be cleared after successful remesh"
    );
    let event = event_receiver
        .try_recv()
        .expect("WorldMeshReady must be emitted once quiet window expires");
    if let SyncEvent::WorldMeshReady { generation, .. } = event {
        assert_eq!(generation, 2);
    } else {
        panic!("Expected WorldMeshReady event");
    }
}
