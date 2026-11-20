//! Documents and pins the mesher's progress-callback threading contract.
//!
//! `SectionMesher` reports progress from rayon workers. That is deliberate — hosts marshal the
//! report to their own main thread (the Blender add-on pushes it into a thread-safe queue) — and
//! it is only safe because every binding entry releases the GIL around the call. This test makes
//! the "callbacks may come from worker threads" half explicit, so nobody can quietly start
//! assuming caller-thread-only delivery (which is what made the parallel save loader deadlock).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::ThreadId;

use glam::IVec3;
use mtk_core::ProgressReport;
use mtk_voxel::{SectionStorage, VoxelStorage, VoxelWorld, VoxelWriter};

fn dense_world() -> VoxelStorage {
    // 4x2x4 sections = 32 sections of solid stone: enough work for rayon to split across workers.
    let mut storage = VoxelStorage::new();
    storage.set_bounds(0, 0, 0, 64, 32, 64);
    let stone = ["minecraft:stone"; 4096];
    for sx in 0..4 {
        for sy in 0..2 {
            for sz in 0..4 {
                let coord = IVec3::new(sx, sy, sz);
                VoxelWriter::set_section(
                    &mut storage,
                    sx,
                    sy,
                    sz,
                    SectionStorage::from_slice(coord, &stone),
                );
            }
        }
    }
    storage
}

#[test]
fn mesher_reports_progress_and_may_do_so_from_worker_threads() {
    let caller: ThreadId = std::thread::current().id();
    let events = AtomicUsize::new(0);
    let off_thread = AtomicUsize::new(0);
    let last_current = AtomicUsize::new(0);
    let regressed = AtomicUsize::new(0);

    let mut world = VoxelWorld::from_storage(dense_world(), None, None, None);
    world.num_threads = Some(4);

    let callback = |report: ProgressReport| {
        events.fetch_add(1, Ordering::Relaxed);
        if std::thread::current().id() != caller {
            off_thread.fetch_add(1, Ordering::Relaxed);
        }
        if report.stage == "meshing_sections" {
            let previous = last_current.load(Ordering::Relaxed);
            if report.current < previous {
                regressed.fetch_add(1, Ordering::Relaxed);
            }
            last_current.store(report.current, Ordering::Relaxed);
        }
    };
    let progress: mtk_core::ProgressCallback<'_> = &callback;

    world
        .rebuild_all_with_progress(Some(progress))
        .expect("meshing must succeed");

    assert!(
        events.load(Ordering::Relaxed) > 0,
        "the mesher must report progress"
    );
    assert_eq!(
        regressed.load(Ordering::Relaxed),
        0,
        "progress within a stage must never go backwards"
    );

    // The contract, not an accident: with a multi-thread pool, some reports come from workers.
    if mtk_core::threading::resolved_threads() > 1 {
        assert!(
            off_thread.load(Ordering::Relaxed) > 0,
            "mesher progress must be allowed to arrive from rayon workers; hosts have to marshal"
        );
    }
}

#[test]
fn mesher_progress_is_silent_when_no_callback_is_installed() {
    let mut world = VoxelWorld::from_storage(dense_world(), None, None, None);
    world.num_threads = Some(2);
    world
        .rebuild_all_with_progress(None)
        .expect("meshing must succeed without a progress callback");
}
