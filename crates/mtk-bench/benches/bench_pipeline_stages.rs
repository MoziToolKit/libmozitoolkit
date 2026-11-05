#![forbid(unsafe_code)]
use std::time::Instant;

use mtk_core::MeshData;
use mtk_cull::FaceCuller;
use mtk_voxel::{
    MesherConfig, ModelSource, PaddedVoxelArray, SectionMesher, VoxelStorage, VoxelWorld,
};

fn main() {
    println!("============================================================");
    println!(" MoziToolKit End-to-End Meshing Pipeline Stage Profiler");
    println!("============================================================");

    let num_sections_x = 8;
    let num_sections_z = 8;
    let num_sections_y = 4;
    let total_sections = num_sections_x * num_sections_z * num_sections_y; // 256 sections (~1.05M voxels)

    println!(
        "Generating {} test chunk sections (1.05 million voxels)...",
        total_sections
    );

    let mut storage = VoxelStorage::new();
    let sample_blocks = [
        "minecraft:stone",
        "minecraft:dirt",
        "minecraft:grass_block",
        "minecraft:oak_planks",
        "minecraft:air",
    ];

    for cx in 0..num_sections_x {
        for cz in 0..num_sections_z {
            for cy in 0..num_sections_y {
                let mut blocks = Vec::with_capacity(4096);
                for ly in 0..16 {
                    let gy = cy * 16 + ly;
                    for _lx in 0..16 {
                        for _lz in 0..16 {
                            let b = if gy < 16 {
                                sample_blocks[0] // stone
                            } else if gy < 32 {
                                sample_blocks[1] // dirt
                            } else if gy < 48 {
                                sample_blocks[2] // grass
                            } else if gy < 56 {
                                sample_blocks[3] // planks
                            } else {
                                sample_blocks[4] // air
                            };
                            blocks.push(b.to_string());
                        }
                    }
                }
                storage.set_section_snapshot(
                    cx,
                    cy,
                    cz,
                    cx * 16,
                    cy * 16,
                    cz * 16,
                    16,
                    16,
                    16,
                    &sample_blocks
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>(),
                    &(0..4096)
                        .map(|i| {
                            let gy = cy * 16 + (i / 256);
                            if gy < 16 {
                                0
                            } else if gy < 32 {
                                1
                            } else if gy < 48 {
                                2
                            } else if gy < 56 {
                                3
                            } else {
                                4
                            }
                        })
                        .collect::<Vec<_>>(),
                    None,
                    None,
                );
            }
        }
    }

    let config = MesherConfig {
        weld_vertices: true,
        origin_centered: true,
        enable_ao: true,
        ..Default::default()
    };
    let culler = FaceCuller::default();

    println!("\n[Stage Breakdown for 256 Chunk Sections Meshing]");

    // Stage 1: Padded array extraction
    let t_s1 = Instant::now();
    let non_empty = storage.get_all_non_empty_sections();
    let padded_arrays: Vec<PaddedVoxelArray> = non_empty
        .iter()
        .map(|&coord| storage.get_section_padded_array(coord))
        .collect();
    let d_s1 = t_s1.elapsed();
    println!(
        " 1. Padded Array Extraction (18x18x18 padding): {:.2} ms",
        d_s1.as_secs_f64() * 1000.0
    );

    // Stage 2: Section Meshing (parallel)
    let t_s2 = Instant::now();
    let section_meshes = SectionMesher::mesh_sections_parallel_with_source_and_progress(
        &padded_arrays,
        &culler,
        |_st| ModelSource::None,
        &config,
        None,
        None,
    )
    .unwrap();
    let d_s2 = t_s2.elapsed();
    println!(
        " 2. Section Meshing (Culling + AO + Shading + Grid Pool): {:.2} ms",
        d_s2.as_secs_f64() * 1000.0
    );

    // Stage 3: Section Mesh Cache Insertion & Assembly Prep
    let t_s3 = Instant::now();
    let mesh_refs: Vec<&MeshData> = section_meshes.iter().map(|(_, m)| m).collect();
    let d_s3 = t_s3.elapsed();
    println!(
        " 3. Mesh Reference Collection:                    {:.2} ms",
        d_s3.as_secs_f64() * 1000.0
    );

    // Stage 4: Single-pass World Merge & Boundary Seam Welding
    let t_s4 = Instant::now();
    let _merged_mesh = MeshData::merge_welded_sections(&mesh_refs, 1e-4);
    let d_s4 = t_s4.elapsed();
    println!(
        " 4. World Merge & Seam Welding (merge_welded):    {:.2} ms",
        d_s4.as_secs_f64() * 1000.0
    );

    // Stage 5: Comparison with Legacy 2-pass merge + full weld
    let t_s5 = Instant::now();
    let mut legacy_merged = MeshData::merge_all_refs(&mesh_refs);
    legacy_merged.weld_vertices(1e-4);
    let d_s5 = t_s5.elapsed();
    println!(
        " 5. [Legacy Comparison] merge_all_refs + weld:    {:.2} ms (vs {:.2} ms)",
        d_s5.as_secs_f64() * 1000.0,
        d_s4.as_secs_f64() * 1000.0
    );

    // Total end-to-end via VoxelWorld
    println!("\n[End-to-End VoxelWorld::rebuild_all Benchmark]");
    let mut world = VoxelWorld::from_storage(storage, Some(config), Some(culler), None);
    let t_total = Instant::now();
    let final_mesh = world.rebuild_all().unwrap();
    let d_total = t_total.elapsed();
    println!(
        " Total Rebuild All Time: {:.2} ms | Vertices: {} | Quads: {} | Triangles: {}",
        d_total.as_secs_f64() * 1000.0,
        final_mesh.vertex_count(),
        final_mesh.quad_count(),
        final_mesh.triangle_count()
    );

    println!("============================================================");
}
