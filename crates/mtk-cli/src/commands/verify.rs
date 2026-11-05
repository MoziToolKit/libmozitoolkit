use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use clap::Args;
use glam::IVec3;
use mtk_core::Direction;
use mtk_cull::{FaceCuller, GlassCullMode, LeavesCullMode};
use serde::{Deserialize, Serialize};

#[derive(Args, Debug)]
pub struct VerifyCullArgs {
    /// Path to input JSON test cases file (if not provided, reads from stdin)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Path to output JSON file (if not provided, writes to stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Override leaves culling mode (FANCY, FAST, NONE, SINGLE_FACE)
    #[arg(long, default_value = "SINGLE_FACE")]
    pub leaves: String,

    /// Override glass culling mode (SAME_BLOCK, NONE, GROUP)
    #[arg(long, default_value = "GROUP")]
    pub glass: String,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    state: String,
    neighbor: Option<String>,
    direction: String,
    #[serde(default)]
    leaves_mode: Option<String>,
    #[serde(default)]
    glass_mode: Option<String>,
    pos_a: Option<[i32; 3]>,
    pos_b: Option<[i32; 3]>,
}

#[derive(Debug, Serialize)]
struct TestResult {
    render: bool,
}

pub fn run_verify_cull(args: VerifyCullArgs) -> Result<(), Box<dyn std::error::Error>> {
    let input_str = if let Some(path) = &args.input {
        fs::read_to_string(path)?
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    };

    let test_cases: Vec<TestCase> = serde_json::from_str(&input_str)?;
    let mut results: Vec<TestResult> = Vec::with_capacity(test_cases.len());

    let default_leaves_mode = match args.leaves.to_uppercase().as_str() {
        "FANCY" => LeavesCullMode::Fancy,
        "FAST" => LeavesCullMode::Fast,
        "NONE" => LeavesCullMode::None,
        _ => LeavesCullMode::SingleFace,
    };

    let default_glass_mode = match args.glass.to_uppercase().as_str() {
        "SAME_BLOCK" => GlassCullMode::SameBlock,
        "NONE" => GlassCullMode::None,
        _ => GlassCullMode::Group,
    };

    let culler = FaceCuller::default();

    for tc in test_cases {
        let leaves_mode = tc
            .leaves_mode
            .as_deref()
            .map(|m| match m.to_uppercase().as_str() {
                "FANCY" => LeavesCullMode::Fancy,
                "FAST" => LeavesCullMode::Fast,
                "NONE" => LeavesCullMode::None,
                _ => LeavesCullMode::SingleFace,
            })
            .unwrap_or(default_leaves_mode);

        let glass_mode = tc
            .glass_mode
            .as_deref()
            .map(|m| match m.to_uppercase().as_str() {
                "SAME_BLOCK" => GlassCullMode::SameBlock,
                "NONE" => GlassCullMode::None,
                _ => GlassCullMode::Group,
            })
            .unwrap_or(default_glass_mode);

        let mut instance = culler.clone();
        instance.leaves_cull_mode = leaves_mode;
        instance.glass_cull_mode = glass_mode;

        let state_meta = instance.get_meta(&tc.state, None, None);
        let neighbor_meta = tc
            .neighbor
            .as_ref()
            .map(|n| instance.get_meta(n, None, None));

        let dir = Direction::parse_loose(&tc.direction).unwrap_or(Direction::East);
        let pos_a = tc.pos_a.map(|p| IVec3::new(p[0], p[1], p[2]));
        let pos_b = tc.pos_b.map(|p| IVec3::new(p[0], p[1], p[2]));

        let render = instance.should_render_face(
            &state_meta,
            neighbor_meta.as_deref(),
            dir,
            None,
            pos_a,
            pos_b,
        );

        results.push(TestResult { render });
    }

    let output_json = serde_json::to_string(&results)?;
    if let Some(out_path) = &args.output {
        fs::write(out_path, output_json)?;
    } else {
        io::stdout().write_all(output_json.as_bytes())?;
    }

    Ok(())
}
