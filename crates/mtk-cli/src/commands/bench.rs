use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::{Args, Subcommand};
use glam::IVec3;
use mtk_model::{BlockState, BlockStateResolver, ModelBaker};
use mtk_voxel::{MesherConfig, VoxelStorage, VoxelWorld};

use crate::utils::assets::ReadFailureCell;
use crate::utils::AssetStack;

#[derive(Subcommand, Debug)]
pub enum BenchSubcommand {
    /// Benchmark blockstate model baking and mesh generation from a real jar
    Model(BenchModelArgs),

    /// Benchmark the real world-meshing pipeline: save I/O → padded extraction → models/atlas/CTM
    /// meshing → weld assembly, per stage, using a real `.mtkcache`
    World(BenchWorldArgs),
}

#[derive(Args, Debug)]
pub struct BenchModelArgs {
    /// Path to Minecraft vanilla JAR or assets directory
    #[arg(short, long, default_value = "26.2-Fabric.jar")]
    pub jar: PathBuf,
}

#[derive(Args, Debug)]
pub struct BenchWorldArgs {
    /// Asset cache package (`.mtkcache`) to mesh with. Required: the benchmark refuses to
    /// measure the fallback-cube path, which is not what a configured world renders.
    #[arg(long)]
    pub cache: PathBuf,

    /// Minecraft save directory (contains `level.dat`)
    #[arg(long)]
    pub save: PathBuf,

    /// Dimension id to load
    #[arg(long, default_value = "overworld")]
    pub dimension: String,

    /// Selection bounds `min_x,min_y,min_z,max_x,max_y,max_z` (inclusive)
    #[arg(
        long,
        default_value = "-64,-64,-64,512,320,512",
        allow_hyphen_values = true
    )]
    pub bounds: String,

    /// Number of rebuild iterations to time (the first one warms caches)
    #[arg(long, default_value_t = 3)]
    pub iterations: usize,

    /// Rayon worker count (default: available parallelism)
    #[arg(long)]
    pub threads: Option<usize>,

    /// Ignore baked CTM rules (useful to attribute their per-face cost)
    #[arg(long)]
    pub no_ctm: bool,

    /// Disable vertex welding (section pool + seam merge), to attribute its cost
    #[arg(long)]
    pub no_weld: bool,

    /// Emit machine-readable JSON instead of the table
    #[arg(long)]
    pub json: bool,
}

pub fn run_bench(cmd: BenchSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        BenchSubcommand::Model(args) => run_bench_model(args),
        BenchSubcommand::World(args) => run_bench_world(args),
    }
}

fn run_bench_model(args: BenchModelArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" MoziToolKit - Model Baker Benchmark Suite");
    println!(" Assets path : {:?}", args.jar);
    println!("============================================================");

    let mut loader = AssetStack::new(&args.jar)?;
    let all_blockstate_names = loader.list_all_blockstates();
    println!(
        " Total blockstate JSONs found: {}",
        all_blockstate_names.len()
    );

    // 1. Collect all blockstate variants across the entire vanilla jar
    let mut all_test_states = Vec::new();
    let scan_start = Instant::now();

    for bs_name in &all_blockstate_names {
        if let Some(def) = loader.load_blockstate(bs_name)? {
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
            let bs_def = loader.load_blockstate(&bs.name)?;
            let read_failures = ReadFailureCell::default();
            let baked = baker.bake_blockstate(state_str, bs_def.as_ref(), |id| {
                read_failures.capture(loader.load_model(id))
            });
            read_failures.into_result()?;
            if let Ok(baked) = baked {
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
        if let Some(def) = loader.load_blockstate(name)? {
            in_memory_states_json.insert(name.clone(), def);
        }
    }
    for name in &all_blockstate_names {
        if let Ok(bs) = BlockState::parse(name) {
            if let Some(def) = in_memory_states_json.get(&bs.name) {
                let matches = BlockStateResolver::resolve(def, &bs);
                for m in matches {
                    if let Some(model) = loader.load_model(&m.model_id)? {
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
        if let Some(def) = loader.load_blockstate(&bs.name)? {
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
        if let Some(m) = loader.load_model(name)? {
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
    let def = loader.load_blockstate(&bs.name)?;
    let read_failures = ReadFailureCell::default();
    let stairs_baked = baker.bake_blockstate(stairs_state, def.as_ref(), |id| {
        read_failures.capture(loader.load_model(id))
    })?;
    read_failures.into_result()?;

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

/// Parses `min_x,min_y,min_z,max_x,max_y,max_z` into an inclusive `(min, max)` block box.
fn parse_bounds(raw: &str) -> Result<(IVec3, IVec3), Box<dyn std::error::Error>> {
    let values: Vec<i32> = raw
        .split(',')
        .map(|part| part.trim().parse::<i32>())
        .collect::<Result<_, _>>()
        .map_err(|error| format!("invalid --bounds '{raw}': {error}"))?;
    if values.len() != 6 {
        return Err(format!(
            "--bounds needs 6 comma-separated integers, got {}",
            values.len()
        )
        .into());
    }
    Ok((
        IVec3::new(values[0], values[1], values[2]),
        IVec3::new(values[3], values[4], values[5]),
    ))
}

fn run_bench_world(args: BenchWorldArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" MoziToolKit - Real World Meshing Benchmark");
    println!(" cache      : {:?}", args.cache);
    println!(" save       : {:?}", args.save);
    println!("============================================================");

    // 1. Open the real asset cache. Every payload is used exactly as the host uses it.
    let reader = libmtk::AssetCacheReader::open(&args.cache)?;
    let manifest = reader.manifest()?;
    let contents = reader.contents();

    let asset_start = Instant::now();
    let models = contents.models()?.ok_or(
        "asset cache has no baked model database; refusing to benchmark the fallback-cube path",
    )?;
    let atlas = contents.atlas_mapping()?;
    let biome_resolver = contents.biome_resolver()?;
    let colormaps = contents.colormaps()?;
    let ctm_solver = if args.no_ctm {
        None
    } else {
        contents.ctm_solver()?
    };
    let banner_baker = reader.load_banner_baker()?;
    let asset_ms = asset_start.elapsed().as_secs_f64() * 1000.0;

    println!(
        " manifest   : fmt={} packs={} models={} atlas_chunks={}",
        manifest.format_version, manifest.pack_count, manifest.baked_models, manifest.atlas_chunks
    );
    println!(
        " assets     : {:.0} ms (models={} atlas={} biome={} colormaps={} ctm={})",
        asset_ms,
        models.len(),
        atlas.is_some(),
        biome_resolver.is_some(),
        colormaps.is_some(),
        ctm_solver.is_some()
    );

    // 2. Load the save selection.
    let (min, max) = parse_bounds(&args.bounds)?;
    let mut storage = VoxelStorage::new();
    let load_start = Instant::now();
    let section_count = mtk_save::SaveLoader::load_box_into_storage(
        &args.save,
        &args.dimension,
        min,
        max,
        &mut storage,
    )?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    println!(
        " load       : {:.1} ms ({} sections, bounds {:?}..{:?})",
        load_ms, section_count, min, max
    );

    // 3. Mesh it with the same config the addon builds.
    let mut config = MesherConfig {
        enable_ao: true,
        mesh_fluids: true,
        weld_mode: if args.no_weld {
            mtk_voxel::WeldMode::None
        } else {
            mtk_voxel::WeldMode::Manifold
        },
        origin_centered: true,
        ctm_solver: ctm_solver.map(Arc::new),
        atlas_address_map: atlas.map(Arc::new),
        biome_resolver: biome_resolver.map(Arc::new),
        colormaps,
        ..MesherConfig::default()
    };
    config.selection_bounds = Some((
        [min.x, min.y, min.z],
        [max.x - min.x + 1, max.y - min.y + 1, max.z - min.z + 1],
    ));

    let mut world = VoxelWorld::from_storage(storage, Some(config), None, Some(Arc::new(models)));
    world.num_threads = args.threads;
    let mut textures = mtk_block_entity_textures::BlockEntityTextures::default();
    textures.set_banner_baker(banner_baker);
    world.set_block_entity_textures(Some(Box::new(textures)));

    println!();
    println!(
        " {:<9} {:>10} {:>10} {:>12} {:>10} {:>10} {:>10}",
        "iteration", "total ms", "quads", "quads/s", "extract", "meshing", "assemble"
    );

    let mut runs: Vec<serde_json::Value> = Vec::with_capacity(args.iterations);
    let mut best_ms = f64::MAX;
    let mut best_quads = 0usize;

    for iteration in 0..args.iterations.max(1) {
        let started = Instant::now();
        let mesh = world.rebuild_all()?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let quads = mesh.quad_count();
        let verts = mesh.vertex_count();

        let mut stage_ms: HashMap<String, f64> = HashMap::new();
        let mut stages_json = Vec::new();
        if let Some(profile) = world.last_profile.as_ref() {
            for stage in &profile.stages {
                let name = stage.name.to_string();
                let ms = stage.duration_us as f64 / 1000.0;
                stage_ms.insert(name.clone(), ms);
                stages_json.push(serde_json::json!({
                    "name": name,
                    "category": stage.category,
                    "depth": stage.depth,
                    "ms": ms,
                    "items": stage.items_count,
                }));
            }
        }

        println!(
            " {:<9} {:>10.1} {:>10} {:>12.0} {:>10.1} {:>10.1} {:>10.1}",
            iteration,
            elapsed_ms,
            quads,
            quads as f64 / (elapsed_ms / 1000.0),
            stage_ms.get("extract_padded").copied().unwrap_or(0.0),
            stage_ms.get("parallel_meshing").copied().unwrap_or(0.0),
            stage_ms.get("assemble").copied().unwrap_or(0.0),
        );

        if elapsed_ms < best_ms {
            best_ms = elapsed_ms;
            best_quads = quads;
        }

        runs.push(serde_json::json!({
            "iteration": iteration,
            "total_ms": elapsed_ms,
            "quads": quads,
            "verts": verts,
            "quads_per_sec": quads as f64 / (elapsed_ms / 1000.0),
            "stages": stages_json,
        }));
    }

    println!();
    println!(
        " best       : {:.1} ms -> {:.0} quads/s ({})",
        best_ms,
        best_quads as f64 / (best_ms / 1000.0),
        match (args.no_ctm, args.no_weld) {
            (true, true) => "no CTM, no weld",
            (true, false) => "no CTM",
            (false, true) => "no weld",
            (false, false) => "CTM on",
        }
    );
    println!(
        " note       : 'best' excludes save I/O ({:.1} ms) and host injection.",
        load_ms
    );

    if args.json {
        let payload = serde_json::json!({
            "cache": args.cache,
            "save": args.save,
            "dimension": args.dimension,
            "bounds": { "min": [min.x, min.y, min.z], "max": [max.x, max.y, max.z] },
            "manifest": {
                "format_version": manifest.format_version,
                "pack_count": manifest.pack_count,
                "baked_models": manifest.baked_models,
                "atlas_chunks": manifest.atlas_chunks,
            },
            "load_ms": load_ms,
            "runs": runs,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_bounds;
    use glam::IVec3;

    #[test]
    fn bounds_require_six_integers_in_min_max_order() {
        let (min, max) = parse_bounds("-64,-64,-64,512,320,512").expect("valid bounds");
        assert_eq!(min, IVec3::new(-64, -64, -64));
        assert_eq!(max, IVec3::new(512, 320, 512));
    }

    #[test]
    fn bounds_reject_wrong_arity_and_garbage() {
        assert!(parse_bounds("-64,-64,-64,512,320").is_err());
        assert!(parse_bounds("a,b,c,d,e,f").is_err());
        assert!(parse_bounds("").is_err());
    }
}
