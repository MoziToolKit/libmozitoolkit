use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use mtk_model::{BlockState, ModelBaker};

use crate::utils::UniversalAssetLoader;

#[derive(Subcommand, Debug)]
pub enum ExportSubcommand {
    /// Export representative blockstate samples (stairs, walls, lanterns, etc.) to OBJ/MTL
    Samples(ExportSamplesArgs),

    /// Export builtin hardcoded models (chests, shulkers, skulls, beds, signs, bell, etc.) to OBJ/MTL
    BuiltinObjs(ExportBuiltinObjsArgs),
}

#[derive(Args, Debug)]
pub struct ExportSamplesArgs {
    /// Path to Minecraft vanilla JAR or assets directory
    #[arg(short, long, default_value = "26.2-Fabric.jar")]
    pub jar: PathBuf,

    /// Directory where exported .obj and .mtl files will be saved
    #[arg(short, long, default_value = "manual_test/samples")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct ExportBuiltinObjsArgs {
    /// Path to Minecraft assets directory or vanilla JAR
    #[arg(short, long, default_value = "26.2-Fabric.jar")]
    pub assets: PathBuf,

    /// Directory where exported .obj and .mtl files will be saved
    #[arg(short, long, default_value = "manual_test/models")]
    pub output: PathBuf,

    /// Directory where extracted textures will be saved
    #[arg(short, long, default_value = "manual_test/textures")]
    pub textures_dir: PathBuf,
}

pub fn run_export(cmd: ExportSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ExportSubcommand::Samples(args) => run_export_samples(args),
        ExportSubcommand::BuiltinObjs(args) => run_export_builtin_objs(args),
    }
}

fn run_export_samples(args: ExportSamplesArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("==================================================");
    println!(" MoziToolKit - Universal Model Baker OBJ Exporter");
    println!(" Assets Source : {:?}", args.jar);
    println!(" Output Dir    : {:?}", args.output);
    println!("==================================================");

    if !args.jar.exists() {
        eprintln!("Error: Assets path not found at {:?}", args.jar);
        std::process::exit(1);
    }

    fs::create_dir_all(&args.output)?;

    let mut loader = UniversalAssetLoader::new(&args.jar)?;
    let mut baker = ModelBaker::new();

    let test_states = [
        // 1. Simple full cube
        "minecraft:stone",
        "minecraft:oak_planks",
        "minecraft:glass",
        // 2. Directional block with unique faces and state properties
        "minecraft:furnace[facing=north,lit=false]",
        "minecraft:furnace[facing=east,lit=true]",
        "minecraft:observer[facing=up,powered=false]",
        "minecraft:observer[facing=south,powered=true]",
        // 3. Stairs (rotation, UVLock, internal volume culling)
        "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]",
        "minecraft:oak_stairs[facing=north,half=top,shape=outer_left]",
        "minecraft:oak_stairs[facing=south,half=bottom,shape=inner_right]",
        // 4. Multipart blocks
        "minecraft:oak_fence[north=true,east=true,south=false,west=false,waterlogged=false]",
        "minecraft:cobblestone_wall[up=true,north=low,east=none,south=low,west=none,waterlogged=false]",
        // 5. Non-full complex elements and rotations
        "minecraft:lantern[hanging=false,waterlogged=false]",
        "minecraft:lantern[hanging=true,waterlogged=false]",
        "minecraft:iron_chain[axis=y,waterlogged=false]",
        "minecraft:iron_chain[axis=x,waterlogged=false]",
        "minecraft:torch",
        "minecraft:redstone_torch[lit=true]",
        "minecraft:oak_trapdoor[facing=north,half=bottom,open=false,powered=false,waterlogged=false]",
        "minecraft:oak_trapdoor[facing=north,half=bottom,open=true,powered=false,waterlogged=false]",
    ];

    println!(
        "\nBaking and exporting {} sample models...",
        test_states.len()
    );

    for &state_str in &test_states {
        let bs = BlockState::parse(state_str)?;
        let bs_def = loader.load_blockstate(&bs.name);

        let baked = baker.bake_blockstate(state_str, bs_def.as_ref(), |model_id| {
            loader.load_model(model_id)
        })?;

        let sanitized = state_str
            .replace("minecraft:", "")
            .replace('[', "_")
            .replace(']', "")
            .replace(',', "_")
            .replace('=', "-");

        let obj_filename = format!("{}.obj", sanitized);
        let mtl_filename = format!("{}.mtl", sanitized);

        let obj_path = args.output.join(&obj_filename);
        let mtl_path = args.output.join(&mtl_filename);

        let (mesh, textures) = baked.to_mesh_with_textures(true);

        let mut obj_content = String::new();
        obj_content.push_str(&format!("# BlockState: {}\n", state_str));
        obj_content.push_str(&format!("mtllib {}\n", mtl_filename));
        obj_content.push_str(&mtk_model::mesh_to_obj_string(&mesh, &sanitized, &textures));

        let mut mtl_content = String::new();
        mtl_content.push_str("# Materials generated for manual inspection in Blender\n");
        for tex in &textures {
            mtl_content.push_str(&format!("newmtl {}\n", tex));
            mtl_content.push_str("Kd 0.8 0.8 0.8\n");
            mtl_content.push_str("d 1.0\n");
            mtl_content.push_str("illum 1\n\n");
        }

        fs::write(&obj_path, obj_content)?;
        fs::write(&mtl_path, mtl_content)?;

        println!(
            " ✓ Baked {:<65} -> {:>3} verts, {:>3} tris, {:>2} textures [cube: {:<5}]",
            state_str,
            mesh.vertex_count(),
            mesh.triangle_count(),
            textures.len(),
            baked.is_cube
        );
    }

    println!("\n==================================================");
    println!(" All models successfully exported to: {:?}", args.output);
    println!("==================================================");

    Ok(())
}

fn run_export_builtin_objs(args: ExportBuiltinObjsArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" MoziToolKit - Exporting Builtin Models to Wavefront OBJ");
    println!("============================================================");
    println!("Assets Source:       {}", args.assets.display());
    println!("Models Output Dir:   {}", args.output.display());
    println!("Textures Output Dir: {}", args.textures_dir.display());
    println!("------------------------------------------------------------");

    fs::create_dir_all(&args.output)?;
    fs::create_dir_all(&args.textures_dir)?;

    let mut loader = UniversalAssetLoader::new(&args.assets)?;
    let mut baker = ModelBaker::new();

    let test_cases = vec![
        // Chests
        ("chest_single", "minecraft:chest[facing=north,type=single]"),
        ("chest_left", "minecraft:chest[facing=north,type=left]"),
        ("chest_right", "minecraft:chest[facing=north,type=right]"),
        (
            "trapped_chest_single",
            "minecraft:trapped_chest[facing=north,type=single]",
        ),
        ("ender_chest", "minecraft:ender_chest[facing=north]"),
        // Shulker Boxes
        ("shulker_box_default", "minecraft:shulker_box[facing=up]"),
        ("shulker_box_red", "minecraft:red_shulker_box[facing=up]"),
        (
            "shulker_box_white_north",
            "minecraft:white_shulker_box[facing=north]",
        ),
        // Skulls & Heads
        ("skeleton_skull", "minecraft:skeleton_skull[rotation=0]"),
        (
            "skeleton_wall_skull",
            "minecraft:skeleton_wall_skull[facing=north]",
        ),
        (
            "wither_skeleton_skull",
            "minecraft:wither_skeleton_skull[rotation=0]",
        ),
        ("zombie_head", "minecraft:zombie_head[rotation=0]"),
        ("creeper_head", "minecraft:creeper_head[rotation=0]"),
        ("player_head", "minecraft:player_head[rotation=0]"),
        // Bell
        (
            "bell_floor",
            "minecraft:bell[attachment=floor,facing=north]",
        ),
        (
            "bell_ceiling",
            "minecraft:bell[attachment=ceiling,facing=north]",
        ),
        (
            "bell_single_wall",
            "minecraft:bell[attachment=single_wall,facing=north]",
        ),
        // End Portal
        ("end_portal", "minecraft:end_portal"),
        // Signs & Hanging Signs
        ("oak_sign", "minecraft:oak_sign[rotation=0]"),
        ("oak_wall_sign", "minecraft:oak_wall_sign[facing=north]"),
        (
            "oak_hanging_sign",
            "minecraft:oak_hanging_sign[attached=false,rotation=0,waterlogged=false]",
        ),
        (
            "oak_wall_hanging_sign",
            "minecraft:oak_wall_hanging_sign[facing=north,waterlogged=false]",
        ),
        // Beds
        ("red_bed_foot", "minecraft:red_bed[facing=north,part=foot]"),
        ("red_bed_head", "minecraft:red_bed[facing=north,part=head]"),
        (
            "blue_bed_foot",
            "minecraft:blue_bed[facing=north,part=foot]",
        ),
        // Standard comparison models
        (
            "oak_stairs",
            "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]",
        ),
        (
            "oak_door_lower",
            "minecraft:oak_door[facing=north,half=lower,hinge=left,open=false]",
        ),
    ];

    let mut copied_textures: HashMap<String, PathBuf> = HashMap::new();

    for (label, state_str) in test_cases {
        let bs = BlockState::parse(state_str)?;
        let def = loader.load_blockstate(&bs.name);

        let baked =
            baker.bake_blockstate(state_str, def.as_ref(), |m_id| loader.load_model(m_id))?;
        let (mesh, textures) = baked.to_mesh_with_textures(false);

        println!(
            "Exporting '{}' ({} elements, {} vertices, {} faces)...",
            label,
            baked.elements.len(),
            mesh.vertex_count(),
            mesh.triangle_count()
        );

        // 1. Resolve and copy textures
        let mut mat_map: Vec<String> = Vec::new();
        for (idx, tex_name) in textures.iter().enumerate() {
            let mat_name = format!("mat_{}_{}", label, idx);
            mat_map.push(mat_name.clone());

            let clean_tex = tex_name.strip_prefix("minecraft:").unwrap_or(tex_name);
            let rel_candidates = [
                format!("assets/minecraft/textures/{}.png", clean_tex),
                format!("assets/minecraft/textures/entity/{}.png", clean_tex),
                format!("assets/minecraft/textures/block/{}.png", clean_tex),
                format!("assets/minecraft/textures/item/{}.png", clean_tex),
            ];

            let file_safe_name = clean_tex.replace('/', "_") + ".png";
            let dest_path = args.textures_dir.join(&file_safe_name);

            let mut found_bytes = None;
            for rel in &rel_candidates {
                if let Some(bytes) = loader.read_entry_bytes(rel) {
                    found_bytes = Some(bytes);
                    break;
                }
            }

            if let Some(bytes) = found_bytes {
                if !dest_path.exists() {
                    let _ = fs::write(&dest_path, bytes);
                }
                copied_textures.insert(tex_name.clone(), dest_path);
            } else {
                if !dest_path.exists() {
                    create_fallback_png(&dest_path, idx)?;
                }
                copied_textures.insert(tex_name.clone(), dest_path);
            }
        }

        // 2. Write MTL
        let mtl_filename = format!("{}.mtl", label);
        let mtl_path = args.output.join(&mtl_filename);
        let mut mtl_content = String::new();
        for (idx, tex_name) in textures.iter().enumerate() {
            let mat_name = &mat_map[idx];
            let clean_tex = tex_name.strip_prefix("minecraft:").unwrap_or(tex_name);
            let file_safe_name = clean_tex.replace('/', "_") + ".png";
            mtl_content.push_str(&format!("newmtl {}\n", mat_name));
            mtl_content.push_str("Kd 1.0 1.0 1.0\n");
            mtl_content.push_str("Ka 0.2 0.2 0.2\n");
            mtl_content.push_str("Ks 0.0 0.0 0.0\n");
            mtl_content.push_str("d 1.0\n");
            mtl_content.push_str("illum 1\n");
            mtl_content.push_str(&format!("map_Kd ../textures/{}\n\n", file_safe_name));
        }
        fs::write(&mtl_path, mtl_content)?;

        // 3. Write OBJ
        let obj_filename = format!("{}.obj", label);
        let obj_path = args.output.join(&obj_filename);
        let mut obj_content = String::new();
        obj_content.push_str(&format!("# MoziToolKit Exported OBJ - {}\n", label));
        obj_content.push_str(&format!("mtllib {}\n", mtl_filename));
        obj_content.push_str(&format!("o {}\n", label));

        for p in &mesh.positions {
            obj_content.push_str(&format!("v {:.6} {:.6} {:.6}\n", p[0], p[1], p[2]));
        }
        for uv in &mesh.uvs {
            obj_content.push_str(&format!("vt {:.6} {:.6}\n", uv[0], uv[1]));
        }
        for n in &mesh.normals {
            obj_content.push_str(&format!("vn {:.6} {:.6} {:.6}\n", n[0], n[1], n[2]));
        }

        let mut current_slot = None;
        let num_triangles = mesh.indices.len() / 3;
        for tri_idx in 0..num_triangles {
            let quad_face_idx = tri_idx / 2;
            let slot = mesh.face_materials.get(quad_face_idx).copied().unwrap_or(0);
            if current_slot != Some(slot) {
                current_slot = Some(slot);
                let mat_name = mat_map
                    .get(slot as usize)
                    .map(|s| s.as_str())
                    .unwrap_or("default");
                obj_content.push_str(&format!("usemtl {}\n", mat_name));
            }

            let i0 = mesh.indices[tri_idx * 3] + 1;
            let i1 = mesh.indices[tri_idx * 3 + 1] + 1;
            let i2 = mesh.indices[tri_idx * 3 + 2] + 1;

            obj_content.push_str(&format!(
                "f {}/{}/{} {}/{}/{} {}/{}/{}\n",
                i0, i0, i0, i1, i1, i1, i2, i2, i2
            ));
        }

        fs::write(&obj_path, obj_content)?;
        println!(" -> Saved: {}", obj_path.display());
    }

    println!("------------------------------------------------------------");
    println!(
        "Export completed successfully! Total textures resolved: {}",
        copied_textures.len()
    );
    println!("Output location: {}", args.output.display());

    Ok(())
}

fn create_fallback_png(path: &Path, color_idx: usize) -> Result<(), Box<dyn std::error::Error>> {
    let colors = [
        image::Rgba([200u8, 100, 100, 255]),
        image::Rgba([100, 200, 100, 255]),
        image::Rgba([100, 100, 200, 255]),
        image::Rgba([200, 200, 100, 255]),
        image::Rgba([200, 100, 200, 255]),
        image::Rgba([100, 200, 200, 255]),
    ];
    let c = colors[color_idx % colors.len()];

    let mut img = image::RgbaImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let is_border = x == 0 || y == 0 || x == 15 || y == 15;
            if is_border {
                img.put_pixel(x, y, image::Rgba([50, 50, 50, 255]));
            } else {
                img.put_pixel(x, y, c);
            }
        }
    }

    img.save(path)?;
    Ok(())
}
