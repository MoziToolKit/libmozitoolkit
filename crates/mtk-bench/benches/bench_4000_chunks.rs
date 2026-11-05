#![forbid(unsafe_code)]
use mtk_core::IVec3;
use mtk_cull::FaceCuller;
use mtk_voxel::{MesherConfig, ModelSource, PaddedVoxelArray, SectionMesher, SectionStorage};
use std::time::Instant;

fn main() {
    println!("============================================================");
    println!(" 4,000+ Chunk Sections Multi-Core Meshing Benchmark");
    println!("============================================================");

    let num_sections = 4096;
    println!(
        "Generating {} test chunk sections (16.7 million voxels)...",
        num_sections
    );

    let sample_blocks = [
        "minecraft:stone",
        "minecraft:dirt",
        "minecraft:oak_planks",
        "minecraft:oak_stairs[facing=north,half=bottom,shape=straight]",
        "minecraft:oak_slab[type=bottom]",
        "minecraft:glass",
        "minecraft:oak_leaves",
        "minecraft:water",
        "minecraft:air",
    ];

    let gen_start = Instant::now();
    let mut sections: Vec<PaddedVoxelArray> = Vec::with_capacity(num_sections);

    for i in 0..num_sections {
        let cx = (i % 16) as i32;
        let cy = ((i / 16) % 16) as i32;
        let cz = (i / 256) as i32;
        let coord = IVec3::new(cx, cy, cz);

        // Generate synthetic terrain layer
        let mut core_blocks = Vec::with_capacity(4096);
        for lx in 0..16 {
            for ly in 0..16 {
                for lz in 0..16 {
                    let b = if ly < 4 {
                        sample_blocks[0] // stone
                    } else if ly < 8 {
                        sample_blocks[(lx + lz) % 3] // stone / dirt / planks
                    } else if ly < 12 {
                        sample_blocks[3 + (lx + lz) % 4] // stairs / slab / glass / leaves
                    } else {
                        sample_blocks[8] // air
                    };
                    core_blocks.push(b);
                }
            }
        }

        let sec = SectionStorage::from_slice(coord, &core_blocks);
        let padded = sec.build_padded_array(|_x, _y, _z| "minecraft:air");
        sections.push(padded);
    }
    println!(
        "Generated {} sections in {:.2} ms",
        num_sections,
        gen_start.elapsed().as_secs_f64() * 1000.0
    );

    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    // 1. Single-threaded meshing of 4,000 sections
    println!("\n[Test 1: Single-Threaded 4,096 Sections Meshing (1 core)]");
    let t_single_start = Instant::now();
    let mut total_tris_single = 0usize;
    let mut total_verts_single = 0usize;

    for sec in &sections {
        let mesh = SectionMesher::mesh_section(sec, &culler, |_st| ModelSource::None, &config);
        total_tris_single += mesh.triangle_count();
        total_verts_single += mesh.vertex_count();
    }
    let t_single = t_single_start.elapsed();
    let single_secs = t_single.as_secs_f64();
    println!(" Total Sections:       {}", num_sections);
    println!(" Total Triangles:      {:?}", total_tris_single);
    println!(" Total Vertices:       {:?}", total_verts_single);
    println!(
        " Total Time:           {:.3} s ({:.2} ms)",
        single_secs,
        single_secs * 1000.0
    );
    println!(
        " Average per Section:  {:.2} µs",
        (single_secs * 1_000_000.0) / num_sections as f64
    );
    println!(
        " Throughput:           {:.1} sections / sec ({:.2}M voxels/sec)",
        num_sections as f64 / single_secs,
        (num_sections * 4096) as f64 / (single_secs * 1e6)
    );

    // 2. Multi-threaded parallel meshing of 4,000 sections
    let available_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    println!(
        "\n[Test 2: Multi-Core Parallel Meshing (Rayon, {} threads)]",
        available_cores
    );

    for threads in [2, 4, available_cores] {
        let t_par_start = Instant::now();
        let results = SectionMesher::mesh_sections_parallel_with_source_and_progress(
            &sections,
            &culler,
            |_st| ModelSource::None,
            &config,
            Some(threads),
            None,
        )
        .unwrap();
        let t_par = t_par_start.elapsed();
        let par_secs = t_par.as_secs_f64();
        let speedup = single_secs / par_secs;

        let total_par_tris: usize = results.iter().map(|(_, m)| m.triangle_count()).sum();

        println!(
            " • [{:>2} threads] -> {:>7.2} ms ({:>8.1} sections/sec | {:>6.2}M voxels/sec) | Speedup vs 1-thread: {:.2}x",
            threads,
            par_secs * 1000.0,
            num_sections as f64 / par_secs,
            (num_sections * 4096) as f64 / (par_secs * 1e6),
            speedup
        );
        assert_eq!(total_par_tris, total_tris_single);
    }

    println!("============================================================");
}
