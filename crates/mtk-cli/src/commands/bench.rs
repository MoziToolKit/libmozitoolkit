use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::{Args, Subcommand};
use mtk_model::{BlockState, BlockStateResolver, ModelBaker};

use crate::utils::UniversalAssetLoader;

#[derive(Subcommand, Debug)]
pub enum BenchSubcommand {
    /// Benchmark blockstate model baking and mesh generation
    Model(BenchModelArgs),
}

#[derive(Args, Debug)]
pub struct BenchModelArgs {
    /// Path to Minecraft vanilla JAR or assets directory
    #[arg(short, long, default_value = "26.2-Fabric.jar")]
    pub jar: PathBuf,
}

pub fn run_bench(cmd: BenchSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        BenchSubcommand::Model(args) => run_bench_model(args),
    }
}

fn run_bench_model(args: BenchModelArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" MoziToolKit - Model Baker Benchmark Suite");
    println!(" Assets path : {:?}", args.jar);
    println!("============================================================");

    let mut loader = UniversalAssetLoader::new(&args.jar)?;
    let all_blockstate_names = loader.list_all_blockstates();
    println!(
        " Total blockstate JSONs found: {}",
        all_blockstate_names.len()
    );

    // 1. Collect all blockstate variants across the entire vanilla jar
    let mut all_test_states = Vec::new();
    let scan_start = Instant::now();

    for bs_name in &all_blockstate_names {
        if let Some(def) = loader.load_blockstate(bs_name) {
            if let Some(ref variants) = def.variants {
                for k in variants.keys() {
                    if k.is_empty() {
                        all_test_states.push(bs_name.clone());
                    } else {
                        all_test_states.push(format!("{}[{}]", bs_name, k));
                    }
                }
            } else if def.multipart.is_some() {
                all_test_states.push(bs_name.clone());
                all_test_states.push(format!("{}[waterlogged=false]", bs_name));
                all_test_states.push(format!("{}[waterlogged=true]", bs_name));
            }
        }
    }
    let scan_duration = scan_start.elapsed();
    println!(
        "Collected {} unique blockstate variants in {:?}",
        all_test_states.len(),
        scan_duration
    );

    // 2. Full Jar Cold Bake (Single-threaded)
    let mut baker = ModelBaker::new();
    let cold_start = Instant::now();
    let mut successful_bakes = 0;
    let mut total_tris = 0;

    for state_str in &all_test_states {
        if let Ok(bs) = BlockState::parse(state_str) {
            let bs_def = loader.load_blockstate(&bs.name);
            if let Ok(baked) =
                baker.bake_blockstate(state_str, bs_def.as_ref(), |id| loader.load_model(id))
            {
                successful_bakes += 1;
                let mesh = baked.to_mesh(true);
                total_tris += mesh.triangle_count();
            }
        }
    }
    let cold_duration = cold_start.elapsed();
    println!("\n[Benchmark 1: Full Jar Single-Threaded Cold Bake (1 core)]");
    println!(
        " Successfully baked: {} / {} states",
        successful_bakes,
        all_test_states.len()
    );
    println!(" Total triangles generated: {}", total_tris);
    println!(" Total time: {:?}", cold_duration);
    if successful_bakes > 0 {
        println!(
            " Average time per model: {:.3} µs",
            (cold_duration.as_secs_f64() * 1_000_000.0) / successful_bakes as f64
        );
        println!(
            " Throughput: {:.2} models/sec",
            successful_bakes as f64 / cold_duration.as_secs_f64()
        );
    }

    // 2.1 Multi-Core Parallel Batch Bake Comparison
    println!("\n[Benchmark 2: Full Jar Multi-Core Parallel Batch Bake]");
    let available_parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let conservative_threads = ModelBaker::determine_conservative_threads();
    println!(" Detected hardware threads: {}", available_parallelism);
    println!(" Conservative default threads: {}", conservative_threads);

    let mut in_memory_states_json = HashMap::new();
    let mut in_memory_models_json = HashMap::new();

    for name in &all_blockstate_names {
        if let Some(def) = loader.load_blockstate(name) {
            in_memory_states_json.insert(name.clone(), def);
        }
    }
    for name in &all_blockstate_names {
        if let Ok(bs) = BlockState::parse(name) {
            if let Some(def) = in_memory_states_json.get(&bs.name) {
                let matches = BlockStateResolver::resolve(def, &bs);
                for m in matches {
                    if let Some(model) = loader.load_model(&m.model_id) {
                        in_memory_models_json.insert(m.model_id.clone(), model);
                    }
                }
            }
        }
    }

    let thread_configs = [
        ("Single-thread (1 thread)", Some(1)),
        ("Conservative Default", None),
        (
            "High Performance (8 threads)",
            Some(8.min(available_parallelism)),
        ),
        ("Max Cores (All threads)", Some(available_parallelism)),
    ];

    for (label, threads_opt) in thread_configs {
        let t_start = Instant::now();
        let results = ModelBaker::bake_batch_parallel(
            &all_test_states,
            threads_opt,
            |name| in_memory_states_json.get(name).cloned(),
            |id| in_memory_models_json.get(id).cloned(),
        )?;
        let t_duration = t_start.elapsed();
        let ok_count = results.iter().filter(|(_, r)| r.is_ok()).count();
        let eff_threads = match threads_opt {
            Some(t) if t > 0 => t,
            _ => conservative_threads,
        };
        let speedup = cold_duration.as_secs_f64() / t_duration.as_secs_f64();
        println!(
            " • {:<32} [{} threads] -> {:>7.2} ms ({:>8.1} models/sec) | Speedup vs 1-thread cold: {:.2}x",
            label,
            eff_threads,
            t_duration.as_secs_f64() * 1000.0,
            ok_count as f64 / t_duration.as_secs_f64(),
            speedup
        );
    }

    // 3. Preloaded In-Memory Model Baking (Raw Computation Speed, 100,000 iterations)
    println!("\n[Benchmark 3: In-Memory Raw Baker Computation (Single-Threaded, 100k iters)]");
    let sample_states = [
        "minecraft:stone",
        "minecraft:furnace[facing=north,lit=false]",
        "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]",
        "minecraft:oak_fence[north=true,east=true,south=false,west=false,waterlogged=false]",
        "minecraft:cobblestone_wall[up=true,north=low,east=none,south=low,west=none,waterlogged=false]",
        "minecraft:lantern[hanging=false,waterlogged=false]",
        "minecraft:iron_chain[axis=y,waterlogged=false]",
        "minecraft:observer[facing=south,powered=true]",
    ];

    let mut preloaded_defs = HashMap::new();
    let mut preloaded_models = HashMap::new();
    for &state_str in &sample_states {
        let bs = BlockState::parse(state_str)?;
        if let Some(def) = loader.load_blockstate(&bs.name) {
            preloaded_defs.insert(bs.name.clone(), def);
        }
    }
    let model_names = [
        "minecraft:block/stone",
        "minecraft:block/cube_all",
        "minecraft:block/furnace",
        "minecraft:block/orientable_with_bottom",
        "minecraft:block/oak_stairs",
        "minecraft:block/stairs",
        "minecraft:block/oak_fence_post",
        "minecraft:block/oak_fence_side",
        "minecraft:block/cobblestone_wall_post",
        "minecraft:block/cobblestone_wall_side",
        "minecraft:block/lantern",
        "minecraft:block/chain",
        "minecraft:block/observer",
    ];
    for name in model_names {
        if let Some(m) = loader.load_model(name) {
            preloaded_models.insert(name.to_string(), m);
        }
    }

    let iters = 100_000;
    let mut mem_baker = ModelBaker::new();
    let mem_start = Instant::now();

    for i in 0..iters {
        let state_str = sample_states[i % sample_states.len()];
        let bs = BlockState::parse(state_str).unwrap();
        let def = preloaded_defs.get(&bs.name);
        let _ = mem_baker.bake_blockstate(state_str, def, |id| preloaded_models.get(id).cloned());
        if i % 1000 == 0 {
            mem_baker.clear_cache();
        }
    }
    let mem_duration = mem_start.elapsed();
    println!(" Iterations: {}", iters);
    println!(" Total time: {:?}", mem_duration);
    println!(
        " Average time per bake: {:.3} µs",
        (mem_duration.as_secs_f64() * 1_000_000.0) / iters as f64
    );
    println!(
        " Throughput: {:.2} bakes/sec",
        iters as f64 / mem_duration.as_secs_f64()
    );

    // 4. Mesh Generation with 2D Hidden Volume Clipping
    println!("\n[Benchmark 4: Mesh Generation & Hidden Volume Culling (100k iters)]");
    let stairs_state = "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]";
    let bs = BlockState::parse(stairs_state)?;
    let def = loader.load_blockstate(&bs.name);
    let stairs_baked =
        baker.bake_blockstate(stairs_state, def.as_ref(), |id| loader.load_model(id))?;

    let mesh_iters = 100_000;
    let mesh_start = Instant::now();
    for _ in 0..mesh_iters {
        let _ = stairs_baked.to_mesh(true);
    }
    let mesh_duration = mesh_start.elapsed();
    println!(" Stairs to_mesh(true) iterations: {}", mesh_iters);
    println!(" Total time: {:?}", mesh_duration);
    println!(
        " Average time per mesh gen: {:.3} µs",
        (mesh_duration.as_secs_f64() * 1_000_000.0) / mesh_iters as f64
    );
    println!(
        " Throughput: {:.2} meshes/sec",
        mesh_iters as f64 / mesh_duration.as_secs_f64()
    );

    Ok(())
}
