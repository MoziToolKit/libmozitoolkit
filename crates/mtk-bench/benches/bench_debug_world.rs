#![forbid(unsafe_code)]
//! Debug-world meshing throughput benchmark.
//!
//! Mirrors the pipeline timed by `MoziToolKit.dev.api.run_mesher_benchmark`:
//! padded-array extraction → parallel section meshing → unified merge/weld.
//! Reports cold-culler (fresh `FaceCuller` per run, like the Python bridge) and
//! warm-culler (reused cache) medians so palette/cull-meta caching wins are visible.

use std::time::Instant;

use mtk_core::MeshData;
use mtk_cull::FaceCuller;
use mtk_voxel::{MesherConfig, ModelSource, SectionMesher, VoxelStorage};

struct RunStats {
    quads: usize,
    verts: usize,
    elapsed_ms: f64,
}

/// FNV-1a checksum over every mesh buffer plus custom attributes, so refactors can be
/// validated as byte-for-byte output preserving.
pub fn mesh_checksum(m: &MeshData) -> u64 {
    fn feed(h: &mut u64, bytes: &[u8]) {
        for &b in bytes {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    fn feed_f32(h: &mut u64, values: &[f32]) {
        for v in values {
            feed(h, &v.to_bits().to_le_bytes());
        }
    }

    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    feed_f32(
        &mut h,
        &m.positions.iter().flatten().copied().collect::<Vec<_>>(),
    );
    feed_f32(
        &mut h,
        &m.normals.iter().flatten().copied().collect::<Vec<_>>(),
    );
    feed_f32(&mut h, &m.uvs.iter().flatten().copied().collect::<Vec<_>>());
    for i in &m.indices {
        feed(&mut h, &i.to_le_bytes());
    }
    for slot in &m.face_materials {
        feed(&mut h, &slot.to_le_bytes());
    }
    for idx in &m.face_tint_indices {
        feed(&mut h, &idx.to_le_bytes());
    }
    let mut names: Vec<_> = m.custom_attributes.keys().cloned().collect();
    names.sort();
    for name in names {
        feed(&mut h, name.as_bytes());
        feed(
            &mut h,
            format!("{:?}", m.custom_attributes[&name]).as_bytes(),
        );
    }
    h
}

fn build_merged(storage: &VoxelStorage, culler: &FaceCuller, config: &MesherConfig) -> MeshData {
    let mut non_empty = storage.get_all_non_empty_sections();
    // Section enumeration comes from a std HashMap, whose iteration order varies per
    // process. Sort so the benchmark output (vertex order) is reproducible.
    non_empty.sort_by_key(|c| (c.x, c.y, c.z));
    let padded: Vec<_> = non_empty
        .iter()
        .map(|&coord| storage.get_section_padded_array(coord))
        .collect();

    let results = SectionMesher::mesh_sections_parallel_with_source_and_progress(
        &padded,
        culler,
        |_st| ModelSource::None,
        config,
        None,
        None,
    )
    .expect("section meshing failed");

    let refs: Vec<&MeshData> = results.iter().map(|(_, m)| m).collect();
    if config.weld_vertices {
        MeshData::merge_welded_sections(&refs, 1e-4)
    } else {
        MeshData::merge_all_refs(&refs)
    }
}

fn run_once(storage: &VoxelStorage, culler: &FaceCuller, config: &MesherConfig) -> RunStats {
    let t = Instant::now();
    let merged = build_merged(storage, culler, config);
    RunStats {
        quads: merged.quad_count(),
        verts: merged.vertex_count(),
        elapsed_ms: t.elapsed().as_secs_f64() * 1000.0,
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[values.len() / 2]
}

fn report(label: &str, runs: &[RunStats]) {
    let times: Vec<f64> = runs.iter().map(|r| r.elapsed_ms).collect();
    let med = median(times.clone());
    let min = times.iter().cloned().fold(f64::INFINITY, f64::min);
    let quads = runs[0].quads;
    println!(
        "{label:<18} median={med:7.3} ms  min={min:7.3} ms  quads={quads:<6} verts={:<6} -> {:.2} M quads/s",
        runs[0].verts,
        quads as f64 / med / 1000.0
    );
}

fn main() {
    println!("============================================================");
    println!(" Embedded Debug World Meshing Benchmark");
    println!("============================================================");

    let storage = VoxelStorage::create_debug_world().expect("debug world unavailable");
    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        enable_ao: true,
        ..Default::default()
    };

    let sections = storage.get_all_non_empty_sections().len();
    println!(
        "Sections: {sections} | bounds: {}x{}x{}",
        storage.size_x, storage.size_y, storage.size_z
    );

    let iters = 15;
    // Warmup (also primes global registries).
    for _ in 0..3 {
        run_once(&storage, &FaceCuller::default(), &config);
    }

    let probe = build_merged(&storage, &FaceCuller::default(), &config);
    println!(
        "Output checksum: 0x{:016x} (quads={}, verts={})",
        mesh_checksum(&probe),
        probe.quad_count(),
        probe.vertex_count()
    );

    // Cold: a fresh culler per run, matching `bridge/world.py` behaviour.
    let mut cold = Vec::with_capacity(iters);
    for _ in 0..iters {
        cold.push(run_once(&storage, &FaceCuller::default(), &config));
    }
    report("cold culler", &cold);

    // Warm: the same culler reused across runs.
    let culler = FaceCuller::default();
    run_once(&storage, &culler, &config);
    let mut warm = Vec::with_capacity(iters);
    for _ in 0..iters {
        warm.push(run_once(&storage, &culler, &config));
    }
    report("warm culler", &warm);
    println!("cull cache entries: {}", culler.cache_len());
    println!("============================================================");
}
