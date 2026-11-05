use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use clap::{Args, Subcommand};
use mtk_resource::{AtlasCategory, DirectoryPack, ResourceLocation, ResourcePackStack, ZipPack};
use mtk_texture::{AtlasAddressMap, AtlasBuilder, AtlasBuilderConfig};

#[derive(Subcommand, Debug)]
pub enum AtlasSubcommand {
    /// Bake vanilla blocks atlas from directory or jar
    BakeVanilla(AtlasBakeVanillaArgs),

    /// Bake dual-atlases (static & animated) for all 14 Minecraft categories
    BakeAll(AtlasBakeAllArgs),
}

#[derive(Args, Debug)]
pub struct AtlasBakeVanillaArgs {
    /// Path to Minecraft assets folder or directory
    #[arg(short, long, default_value = "mc")]
    pub mc_dir: PathBuf,

    /// Max atlas texture dimension (width/height)
    #[arg(long, default_value = "4096")]
    pub max_size: u32,
}

#[derive(Args, Debug)]
pub struct AtlasBakeAllArgs {
    /// Path to base vanilla JAR file
    #[arg(short, long, default_value = "26.2-Fabric.jar")]
    pub jar: PathBuf,

    /// Optional top resource pack ZIP file (e.g. SPBR)
    #[arg(short, long, default_value = "SPBR-21.zip")]
    pub pack: Option<PathBuf>,

    /// Directory where baked PNGs and JSON maps will be written
    #[arg(short, long, default_value = "manual_test/atlases")]
    pub output: PathBuf,

    /// Max atlas texture dimension (width/height)
    #[arg(long, default_value = "4096")]
    pub max_size: u32,
}

pub fn run_atlas(cmd: AtlasSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        AtlasSubcommand::BakeVanilla(args) => run_bake_vanilla(args),
        AtlasSubcommand::BakeAll(args) => run_bake_all(args),
    }
}

fn run_bake_vanilla(args: AtlasBakeVanillaArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "=== Testing Vanilla Atlas Baking on {} ===",
        args.mc_dir.display()
    );

    if !args.mc_dir.exists() {
        println!(
            "Warning: {} does not exist, skipping live test.",
            args.mc_dir.display()
        );
        return Ok(());
    }

    let t0 = Instant::now();
    let mut stack = ResourcePackStack::new();
    if args.mc_dir.is_file() {
        stack.push_pack(Box::new(ZipPack::from_file(
            "vanilla_client",
            &args.mc_dir,
        )?));
    } else {
        stack.push_pack(Box::new(DirectoryPack::new("vanilla_client", &args.mc_dir)));
    }

    let blocks_loc = ResourceLocation::parse("minecraft:blocks")?;
    println!("Loading atlas definition: {}", blocks_loc);
    let definition = stack.load_atlas_definition(&blocks_loc)?;
    println!(
        "Loaded atlas definition with {} sources in {:?}",
        definition.sources.len(),
        t0.elapsed()
    );

    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: args.max_size,
        max_height: args.max_size,
        mip_level: 0,
        padding: 0,
    });

    let t_bake = Instant::now();
    let baked = builder.build(&stack, &definition)?;
    let bake_elapsed = t_bake.elapsed();

    println!("--------------------------------------------------");
    println!("Atlas Baking Complete in {:?}", bake_elapsed);
    println!("Total Chunks: {}", baked.chunks.len());
    println!(
        "Total Registered Sprites: {}",
        baked.address_map.sprites.len()
    );
    for chunk in &baked.chunks {
        println!(
            " - Chunk #{}: {}x{} px (Albedo: {} bytes, Normal: {}, Specular: {})",
            chunk.chunk_id,
            chunk.width,
            chunk.height,
            chunk.albedo.pixels.len(),
            chunk.normal.is_some(),
            chunk.specular.is_some()
        );
    }
    println!("--------------------------------------------------");

    for sample in &[
        "minecraft:block/stone",
        "minecraft:block/diamond_ore",
        "minecraft:block/water_still",
        "minecraft:block/oak_planks",
    ] {
        if let Some(loc) = baked.address_map.lookup_str(sample) {
            println!(
                "Sample [{}]: rect={:?}, uv={:?}, frame_size={:?}, frames={}",
                sample, loc.pixel_rect, loc.uv_bounds, loc.frame_size, loc.frame_count
            );
        } else {
            println!("Warning: Sample [{}] not found in atlas!", sample);
        }
    }

    Ok(())
}

fn run_bake_all(args: AtlasBakeAllArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" MoziToolKit - Multi-Category Atlas Baking Pipeline");
    println!("============================================================");
    println!("Vanilla JAR : {}", args.jar.display());
    if let Some(ref p) = args.pack {
        println!("Top Pack ZIP: {}", p.display());
    }
    println!("Output Dir  : {}", args.output.display());
    println!("------------------------------------------------------------");

    if args.output.exists() {
        let _ = fs::remove_dir_all(&args.output);
    }
    fs::create_dir_all(&args.output)?;

    let t_start = Instant::now();

    // 1. Load Resource Packs into Stack
    println!("Step 1: Loading resource pack archives...");
    let t_load = Instant::now();

    let mut stack = ResourcePackStack::new();
    if args.jar.exists() {
        let vanilla_pack = ZipPack::from_file("vanilla_26.2", &args.jar)?;
        println!(" - Loaded vanilla JAR in {:?}", t_load.elapsed());
        stack.append_pack(Box::new(vanilla_pack));
    } else {
        println!("Warning: Vanilla JAR not found at {}", args.jar.display());
    }

    if let Some(ref pack_path) = args.pack {
        if pack_path.exists() {
            let t_pack = Instant::now();
            let pack = ZipPack::from_file("top_pack", pack_path)?;
            println!(" - Loaded top pack ZIP in {:?}", t_pack.elapsed());
            stack.push_pack(Box::new(pack));
        } else {
            println!("Warning: Pack ZIP not found at {}", pack_path.display());
        }
    }

    println!(
        "ResourcePackStack ready ({} packs in stack, total loading time: {:?})\n",
        stack.len(),
        t_start.elapsed()
    );

    let categories = [
        AtlasCategory::Blocks,
        AtlasCategory::Items,
        AtlasCategory::Chests,
        AtlasCategory::ShulkerBoxes,
        AtlasCategory::BannerPatterns,
        AtlasCategory::ShieldPatterns,
        AtlasCategory::ArmorTrims,
        AtlasCategory::DecoratedPot,
        AtlasCategory::Paintings,
        AtlasCategory::Particles,
        AtlasCategory::Celestials,
        AtlasCategory::Gui,
        AtlasCategory::MapDecorations,
        AtlasCategory::Entities,
    ];

    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: args.max_size,
        max_height: args.max_size,
        mip_level: 0,
        padding: 0,
    });

    let mut global_address_map = AtlasAddressMap::new();
    let mut total_static_chunks = 0;
    let mut total_anim_chunks = 0;
    let mut total_baked_sprites = 0;

    println!("Step 2: Baking Isolated Dual-Atlases (Static & Animated) for each category...");
    println!("------------------------------------------------------------");

    for category in &categories {
        let cat_name = category.as_str();
        let t_cat = Instant::now();

        let mut baked = match builder.build_category(&stack, category) {
            Ok(b) => b,
            Err(e) => {
                println!(" [SKIP] Category '{}': {:?}", cat_name, e);
                continue;
            }
        };

        if baked.address_map.sprites.is_empty() {
            println!(
                " [EMPTY] Category '{}': 0 sprites found, skipping output.",
                cat_name
            );
            continue;
        }

        // Decode raw LabPBR companions into canonical MtkPackedV1 channels.
        mtk_material::decode_baked_atlas(&mut baked)?;

        let cat_elapsed = t_cat.elapsed();
        let sprite_count = baked.address_map.sprites.len();
        let mut static_chunks = 0;
        let mut anim_chunks = 0;

        for chunk in &baked.chunks {
            if chunk.is_animated {
                anim_chunks += 1;
            } else {
                static_chunks += 1;
            }
        }
        total_static_chunks += static_chunks;
        total_anim_chunks += anim_chunks;
        total_baked_sprites += sprite_count;

        let mut normal_count = 0;
        let mut specular_count = 0;
        let mut anim_count = 0;
        for loc in baked.address_map.sprites.values() {
            if loc.has_normal {
                normal_count += 1;
            }
            if loc.has_specular {
                specular_count += 1;
            }
            if loc.is_animated {
                anim_count += 1;
            }
        }

        println!(
            " [BAKED] Category '{: <15}' -> {:>4} sprites | Static: {} chk, Anim: {} chk | PBR(N:{:>3}, S:{:>3}) | Anim Sprites:{:>3} in {:?}",
            cat_name, sprite_count, static_chunks, anim_chunks, normal_count, specular_count, anim_count, cat_elapsed
        );

        // Save Baked Chunks to output_dir
        for chunk in &baked.chunks {
            let file_stem = chunk.file_stem();
            let albedo_path = args.output.join(format!("{}_albedo.png", file_stem));
            let albedo_png = chunk.albedo.to_png_bytes()?;
            fs::write(&albedo_path, albedo_png)?;

            if let Some(ref norm_buf) = chunk.normal_gl {
                let norm_path = args.output.join(format!("{}_normal.png", file_stem));
                let norm_png = norm_buf.to_png_bytes()?;
                fs::write(&norm_path, norm_png)?;
            }

            if let Some(ref orm_buf) = chunk.orm {
                let orm_path = args.output.join(format!("{}_orm.png", file_stem));
                let orm_png = orm_buf.to_png_bytes()?;
                fs::write(&orm_path, orm_png)?;
            }

            if let Some(ref aux_buf) = chunk.aux {
                let aux_path = args.output.join(format!("{}_aux.png", file_stem));
                let aux_png = aux_buf.to_png_bytes()?;
                fs::write(&aux_path, aux_png)?;
            }
        }

        // Save Category Mapping JSON
        let cat_json_path = args.output.join(format!("{}_mapping.json", cat_name));
        let cat_json = baked.address_map.to_json()?;
        fs::write(&cat_json_path, cat_json)?;

        // Merge into global address map
        global_address_map.merge(baked.address_map);
    }

    // Save Global Combined Mapping JSON
    let global_json_path = args.output.join("global_atlas_mapping.json");
    let global_json = global_address_map.to_json()?;
    fs::write(&global_json_path, global_json)?;

    println!("------------------------------------------------------------");
    println!("All Dual-Atlases Baked Successfully!");
    println!(" - Total Categories Processed: {}", categories.len());
    println!(" - Total Static Chunks:        {}", total_static_chunks);
    println!(" - Total Animated Chunks:      {}", total_anim_chunks);
    println!(" - Total Sprites Registered:   {}", total_baked_sprites);
    println!(" - Output Artifacts Saved To:  {}", args.output.display());
    println!(" - Total Execution Time:       {:?}", t_start.elapsed());
    println!("============================================================\n");

    Ok(())
}
