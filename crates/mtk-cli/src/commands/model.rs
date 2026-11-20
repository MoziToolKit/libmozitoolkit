use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use mtk_model::{BlockState, ModelBaker};
use serde::{Deserialize, Serialize};

use crate::utils::AssetStack;

#[derive(Subcommand, Debug)]
pub enum ModelSubcommand {
    /// Dump baked model geometry and attributes for a single blockstate
    Dump(ModelDumpArgs),

    /// Batch dump baked models from a JSON array of blockstates
    DumpBatch(ModelDumpBatchArgs),
}

#[derive(Args, Debug)]
pub struct ModelDumpArgs {
    /// Path to Minecraft assets directory or vanilla JAR file
    pub assets: PathBuf,

    /// Blockstate string, e.g. "minecraft:furnace[facing=north,lit=false]"
    pub state: String,

    /// Optional output file path (defaults to stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Format JSON with indentation
    #[arg(short, long, default_value = "true")]
    pub pretty: bool,
}

#[derive(Args, Debug)]
pub struct ModelDumpBatchArgs {
    /// Path to Minecraft assets directory or vanilla JAR file
    pub assets: PathBuf,

    /// Input JSON file containing an array of blockstate strings
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output JSON file to save all baked results
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct RustBakeResult {
    block_state: String,
    element_count: usize,
    mesh_vertex_count: usize,
    mesh_tri_count: usize,
    is_cube: bool,
    is_opaque: bool,
    is_emissive: bool,
    textures: Vec<String>,
    elements: Vec<RustElementInfo>,
    six_faces: Vec<RustFaceInfo>,
}

#[derive(Serialize, Deserialize)]
struct RustElementInfo {
    from: [f32; 3],
    to: [f32; 3],
    faces: HashMap<String, RustFaceInfo>,
}

#[derive(Serialize, Deserialize)]
struct RustFaceInfo {
    direction: String,
    texture: String,
    uv_rot: f32,
    uv_bounds: [f32; 4],
    tint_index: i16,
    cullface: Option<String>,
    vertices: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
}

pub fn run_model(cmd: ModelSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ModelSubcommand::Dump(args) => run_dump(args),
        ModelSubcommand::DumpBatch(args) => run_dump_batch(args),
    }
}

fn run_dump(args: ModelDumpArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut loader = AssetStack::new(&args.assets)?;
    let blockstate = BlockState::parse(&args.state)?;
    let block_id = format!("{}:{}", blockstate.namespace, blockstate.name);
    let def = loader.load_blockstate(&block_id);

    let mut baker = ModelBaker::new();
    let baked = baker.bake_blockstate(&args.state, def.as_ref(), |m_id| loader.load_model(m_id))?;

    let json_str = if args.pretty {
        serde_json::to_string_pretty(&baked)?
    } else {
        serde_json::to_string(&baked)?
    };

    if let Some(out_path) = &args.output {
        fs::write(out_path, json_str)?;
        println!("Exported baked model to {}", out_path.display());
    } else {
        println!("{}", json_str);
    }

    Ok(())
}

fn run_dump_batch(args: ModelDumpBatchArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut loader = AssetStack::new(&args.assets)?;
    let states_json = fs::read_to_string(&args.input)?;
    let states: Vec<String> = serde_json::from_str(&states_json)?;

    let mut results = HashMap::new();
    let mut baker = ModelBaker::new();

    println!(
        "Batch baking {} blockstates from {:?}...",
        states.len(),
        args.assets
    );

    for state_str in &states {
        let bs = match BlockState::parse(state_str) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Skipping invalid state '{}': {}", state_str, e);
                continue;
            }
        };

        let bs_def = loader.load_blockstate(&bs.name);
        match baker.bake_blockstate(state_str, bs_def.as_ref(), |id| loader.load_model(id)) {
            Ok(baked) => {
                let (mesh, textures) = baked.to_mesh_with_textures(true);
                let mut elements = Vec::new();
                for el in &baked.elements {
                    let mut faces = HashMap::new();
                    for (d, f) in &el.faces {
                        faces.insert(
                            d.as_str().to_string(),
                            RustFaceInfo {
                                direction: f.direction.as_str().to_string(),
                                texture: f.texture.clone(),
                                uv_rot: f.uv_rot,
                                uv_bounds: f.uv_bounds,
                                tint_index: f.tint_index,
                                cullface: f.cullface.map(|c| c.as_str().to_string()),
                                vertices: [
                                    [f.vertices[0].x, f.vertices[0].y, f.vertices[0].z],
                                    [f.vertices[1].x, f.vertices[1].y, f.vertices[1].z],
                                    [f.vertices[2].x, f.vertices[2].y, f.vertices[2].z],
                                    [f.vertices[3].x, f.vertices[3].y, f.vertices[3].z],
                                ],
                                uvs: [
                                    [f.uvs[0].x, f.uvs[0].y],
                                    [f.uvs[1].x, f.uvs[1].y],
                                    [f.uvs[2].x, f.uvs[2].y],
                                    [f.uvs[3].x, f.uvs[3].y],
                                ],
                            },
                        );
                    }
                    elements.push(RustElementInfo {
                        from: el.from_pos,
                        to: el.to_pos,
                        faces,
                    });
                }

                let mut six_faces = Vec::new();
                for f in &baked.faces {
                    six_faces.push(RustFaceInfo {
                        direction: f.direction.as_str().to_string(),
                        texture: f.texture.clone(),
                        uv_rot: f.uv_rot,
                        uv_bounds: f.uv_bounds,
                        tint_index: f.tint_index,
                        cullface: f.cullface.map(|c| c.as_str().to_string()),
                        vertices: [
                            [f.vertices[0].x, f.vertices[0].y, f.vertices[0].z],
                            [f.vertices[1].x, f.vertices[1].y, f.vertices[1].z],
                            [f.vertices[2].x, f.vertices[2].y, f.vertices[2].z],
                            [f.vertices[3].x, f.vertices[3].y, f.vertices[3].z],
                        ],
                        uvs: [
                            [f.uvs[0].x, f.uvs[0].y],
                            [f.uvs[1].x, f.uvs[1].y],
                            [f.uvs[2].x, f.uvs[2].y],
                            [f.uvs[3].x, f.uvs[3].y],
                        ],
                    });
                }

                results.insert(
                    state_str.clone(),
                    RustBakeResult {
                        block_state: state_str.clone(),
                        element_count: baked.elements.len(),
                        mesh_vertex_count: mesh.vertex_count(),
                        mesh_tri_count: mesh.triangle_count(),
                        is_cube: baked.is_cube,
                        is_opaque: baked.is_opaque,
                        is_emissive: baked.is_emissive,
                        textures,
                        elements,
                        six_faces,
                    },
                );
            }
            Err(e) => {
                eprintln!("Error baking '{}': {}", state_str, e);
            }
        }
    }

    let json_data = serde_json::to_string_pretty(&results)?;
    fs::write(&args.output, json_data)?;
    println!(
        "Exported {} baked states to {}",
        results.len(),
        args.output.display()
    );

    Ok(())
}
