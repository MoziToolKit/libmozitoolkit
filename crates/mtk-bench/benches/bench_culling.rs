#![forbid(unsafe_code)]
use glam::IVec3;
use mtk_core::Direction;
use mtk_cull::FaceCuller;
use std::time::Instant;

fn main() {
    let complex_blocks = [
        // Stairs
        "minecraft:oak_stairs[facing=north,half=bottom,shape=straight]",
        "minecraft:oak_stairs[facing=east,half=top,shape=inner_left]",
        "minecraft:oak_stairs[facing=south,half=bottom,shape=outer_right]",
        "minecraft:stone_stairs[facing=west,half=top,shape=straight]",
        // Slabs
        "minecraft:oak_slab[type=bottom,waterlogged=false]",
        "minecraft:oak_slab[type=top,waterlogged=false]",
        "minecraft:oak_slab[type=double,waterlogged=false]",
        "minecraft:cut_copper_slab[type=bottom,waterlogged=true]",
        // Snow
        "minecraft:snow[layers=1]",
        "minecraft:snow[layers=2]",
        "minecraft:snow[layers=3]",
        "minecraft:snow[layers=4]",
        "minecraft:snow[layers=7]",
        "minecraft:snow[layers=8]",
        // Fences & Walls
        "minecraft:oak_fence[north=true,east=true,south=false,west=false,waterlogged=false]",
        "minecraft:oak_fence[north=false,east=false,south=true,west=true,waterlogged=false]",
        "minecraft:nether_brick_fence[north=true,east=true,south=true,west=true,waterlogged=false]",
        "minecraft:cobblestone_wall[up=true,north=low,east=none,south=low,west=tall,waterlogged=false]",
        "minecraft:stone_brick_wall[up=false,north=tall,east=tall,south=none,west=none,waterlogged=false]",
        // Panes & Bars
        "minecraft:iron_bars[north=true,east=false,south=true,west=false,waterlogged=false]",
        "minecraft:iron_bars[north=false,east=true,south=false,west=true,waterlogged=false]",
        "minecraft:glass_pane[north=true,east=true,south=true,west=true,waterlogged=false]",
        "minecraft:red_stained_glass_pane[north=true,east=false,south=false,west=true,waterlogged=false]",
        // Carpets, Trapdoors, Doors, Redstone
        "minecraft:white_carpet",
        "minecraft:moss_carpet",
        "minecraft:oak_trapdoor[facing=north,half=bottom,open=false]",
        "minecraft:oak_trapdoor[facing=east,half=top,open=true]",
        "minecraft:iron_door[facing=south,half=lower,open=false,hinge=left]",
        "minecraft:repeater[facing=north,delay=1,locked=false,powered=false]",
        "minecraft:comparator[facing=south,mode=compare,powered=true]",
        "minecraft:daylight_detector[inverted=false,power=15]",
        // Containers & Utilities
        "minecraft:chest[facing=north,type=single,waterlogged=false]",
        "minecraft:trapped_chest[facing=east,type=left,waterlogged=false]",
        "minecraft:anvil[facing=north]",
        "minecraft:chipped_anvil[facing=east]",
        "minecraft:hopper[facing=down,enabled=true]",
        "minecraft:cauldron",
        "minecraft:water_cauldron[level=3]",
        "minecraft:lava_cauldron",
        "minecraft:powder_snow_cauldron[level=2]",
        "minecraft:bell[attachment=floor,facing=north]",
        "minecraft:grindstone[face=floor,facing=north]",
        "minecraft:stonecutter[facing=east]",
        "minecraft:lectern[facing=south,has_book=true]",
        // Translucent & Foliage
        "minecraft:glass",
        "minecraft:tinted_glass",
        "minecraft:white_stained_glass",
        "minecraft:cyan_stained_glass",
        "minecraft:ice",
        "minecraft:blue_ice",
        "minecraft:slime_block",
        "minecraft:honey_block",
        "minecraft:powder_snow",
        "minecraft:oak_leaves",
        "minecraft:cherry_leaves",
        "minecraft:mangrove_leaves",
        "minecraft:mangrove_roots[waterlogged=false]",
        "minecraft:mangrove_roots[waterlogged=true]",
        // Solids & Fluids
        "minecraft:stone",
        "minecraft:dirt",
        "minecraft:deepslate",
        "minecraft:oak_log[axis=y]",
        "minecraft:water[level=0]",
        "minecraft:lava[level=0]",
        "minecraft:air",
    ];

    let culler = FaceCuller::default();

    // 1. Warm-up metadata cache
    for &b in &complex_blocks {
        culler.get_meta(b, None, None);
    }

    let directions = Direction::ALL;
    let pos_a = Some(IVec3::new(10, 64, 20));
    let pos_b = Some(IVec3::new(11, 64, 20));

    // Measure raw throughput over millions of evaluations
    let total_iterations = 2_000_000;
    let n = complex_blocks.len();

    let start = Instant::now();
    let mut render_count = 0usize;

    for i in 0..total_iterations {
        let b1 = complex_blocks[i % n];
        let b2 = complex_blocks[(i * 7 + 13) % n];
        let dir = directions[i % 6];

        let m1 = culler.get_meta(b1, None, None);
        let m2 = culler.get_meta(b2, None, None);

        if culler.should_render_face(&m1, Some(&m2), dir, None, pos_a, pos_b) {
            render_count += 1;
        }
    }

    let duration = start.elapsed();
    let total_secs = duration.as_secs_f64();
    let throughput = total_iterations as f64 / total_secs;
    let avg_ns = (total_secs * 1e9) / total_iterations as f64;

    println!("============================================================");
    println!(" Rust mtk-cull Performance Benchmark");
    println!(" Complex Block States: {}", n);
    println!(" Total Evaluations: {}", total_iterations);
    println!(" Rendered Faces: {}", render_count);
    println!(
        " Total Elapsed: {:.4} s ({:.2} ms)",
        total_secs,
        total_secs * 1000.0
    );
    println!(" Average Latency: {:.2} ns / evaluation", avg_ns);
    println!(
        " Throughput: {:.2} million evaluations / sec",
        throughput / 1_000_000.0
    );
    println!("============================================================");
}
