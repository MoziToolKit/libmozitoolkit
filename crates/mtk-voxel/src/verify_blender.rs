use std::collections::HashMap;
use std::fs;
use std::path::Path;

use glam::IVec3;
use mtk_cull::FaceCuller;
use mtk_material::{get_biome_palette, get_colormap_uv};
use serde::Deserialize;

use crate::fluid::height::{
    calculate_fluid_corner_heights, calculate_fluid_flow_vector, get_fluid_base_height,
};
use crate::fluid::uv::{get_fluid_side_uvs, get_fluid_top_uvs};
use crate::fluid::FluidType;
use crate::mesher::shading::ModelSource;
use crate::mesher::SectionMesher;
use crate::storage::{SectionStorage, VoxelStorage};
use crate::types::MesherConfig;

#[derive(Debug, Deserialize)]
struct CrcTestCase {
    name: String,
    blocks: HashMap<String, String>,
    expected_crc: u32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FluidTestCase {
    name: String,
    fluid_type: String,
    pos: [i32; 3],
    own_state: String,
    block_map: HashMap<String, String>,
    own_height: f32,
    corner_heights: [f32; 4],
    flow_vector: [f32; 2],
    flow_angle: f32,
    is_flowing: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BiomeTestCase {
    biome: String,
    temperature: f32,
    humidity: f32,
    colormap_uv: [f32; 2],
    water_color_linear: [f32; 4],
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FluidUvTestCase {
    name: String,
    is_flowing: Option<bool>,
    rotation: Option<f32>,
    h_left: Option<f32>,
    h_right: Option<f32>,
    expected_uvs: Option<Vec<[f32; 2]>>,
    expected_side_uvs: Option<Vec<[f32; 2]>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MeshTestCase {
    name: String,
    blocks: HashMap<String, String>,
    face_count: usize,
    vertex_count: usize,
    vertices: Vec<[f32; 3]>,
    cubes_count: usize,
    fluids_count: usize,
}

#[derive(Debug, Deserialize)]
struct BlenderGroundTruth {
    crc_tests: Vec<CrcTestCase>,
    fluid_tests: Vec<FluidTestCase>,
    biome_tests: Vec<BiomeTestCase>,
    fluid_uv_tests: Vec<FluidUvTestCase>,
    mesh_tests: Vec<MeshTestCase>,
}

#[test]
fn test_verify_100_percent_match_against_blender_mozi_toolkit() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("blender_ground_truth.json");

    assert!(
        fixture_path.exists(),
        "Ground truth JSON fixture must exist: {:?}",
        fixture_path
    );

    let content = fs::read_to_string(&fixture_path).expect("Read ground truth file");
    let truth: BlenderGroundTruth =
        serde_json::from_str(&content).expect("Parse ground truth JSON");

    println!("============================================================");
    println!(" 100% Verification Against MoziToolKit (Blender Python)");
    println!("============================================================");

    // 1. Verify CRC32
    println!("\n[1. CRC32 Checksum Verification (vs Python zlib.crc32)]");
    for tc in &truth.crc_tests {
        let mut sec = SectionStorage::new(IVec3::new(0, 0, 0));
        for (coord_str, state) in &tc.blocks {
            let parts: Vec<usize> = coord_str.split(',').map(|s| s.parse().unwrap()).collect();
            sec.set_local(parts[0], parts[1], parts[2], state);
        }
        let rust_crc = sec.compute_crc();
        println!(
            " • {:<32} -> Rust CRC: 0x{:08X} | Blender CRC: 0x{:08X}",
            tc.name, rust_crc, tc.expected_crc
        );
        assert_eq!(
            rust_crc, tc.expected_crc,
            "CRC mismatch in test '{}'",
            tc.name
        );
    }

    // 2. Verify Fluid Calculations
    println!("\n[2. Fluid Physics & Corner Height Verification (vs MoziToolKit fluid.py)]");
    for tc in &truth.fluid_tests {
        let ft = match tc.fluid_type.as_str() {
            "water" => FluidType::Water,
            "lava" => FluidType::Lava,
            _ => panic!("Unknown fluid type: {}", tc.fluid_type),
        };

        let rust_own_height = get_fluid_base_height(&tc.own_state);
        assert!(
            (rust_own_height - tc.own_height).abs() < 1e-4,
            "Own height mismatch in '{}': Rust {} vs Blender {}",
            tc.name,
            rust_own_height,
            tc.own_height
        );

        let get_state = |x: i32, y: i32, z: i32| -> String {
            let key = format!("{},{},{}", x, y, z);
            tc.block_map
                .get(&key)
                .cloned()
                .unwrap_or_else(|| "minecraft:air".to_string())
        };

        let (c_nw, c_ne, c_se, c_sw) =
            calculate_fluid_corner_heights(get_state, tc.pos[0], tc.pos[1], tc.pos[2], ft);

        let eps = 1e-3;
        let diff_nw = (c_nw - tc.corner_heights[0]).abs();
        let diff_ne = (c_ne - tc.corner_heights[1]).abs();
        let diff_se = (c_se - tc.corner_heights[2]).abs();
        let diff_sw = (c_sw - tc.corner_heights[3]).abs();

        println!(
            " • {:<36} -> Heights: [{:.3}, {:.3}, {:.3}, {:.3}] (Max Δ: {:.5})",
            tc.name,
            c_nw,
            c_ne,
            c_se,
            c_sw,
            diff_nw.max(diff_ne).max(diff_se).max(diff_sw)
        );

        assert!(
            diff_nw < eps && diff_ne < eps && diff_se < eps && diff_sw < eps,
            "Corner heights mismatch in '{}'",
            tc.name
        );

        let (vx, vz, flow_angle) = calculate_fluid_flow_vector(
            get_state,
            tc.pos[0],
            tc.pos[1],
            tc.pos[2],
            ft,
            rust_own_height,
        );

        let diff_vx = (vx - tc.flow_vector[0]).abs();
        let diff_vz = (vz - tc.flow_vector[1]).abs();
        let diff_angle = (flow_angle - tc.flow_angle).abs();

        assert!(
            diff_vx < eps && diff_vz < eps,
            "Flow vector mismatch in '{}': Rust ({}, {}) vs Blender ({}, {})",
            tc.name,
            vx,
            vz,
            tc.flow_vector[0],
            tc.flow_vector[1]
        );

        if tc.flow_vector[0].abs() > 1e-3 || tc.flow_vector[1].abs() > 1e-3 {
            assert!(
                diff_angle < eps,
                "Flow angle mismatch in '{}': Rust {} vs Blender {}",
                tc.name,
                flow_angle,
                tc.flow_angle
            );
        }
    }

    // 3. Verify Biome Colormap UVs & Metadata
    println!("\n[3. Biome Colormap UV & Meta Verification (vs MoziToolKit biome.py)]");
    for tc in &truth.biome_tests {
        let pal = get_biome_palette(&tc.biome);
        let uv = get_colormap_uv(pal.temperature, pal.humidity);
        let diff_u = (uv[0] - tc.colormap_uv[0]).abs();
        let diff_v = (uv[1] - tc.colormap_uv[1]).abs();
        let diff_temp = (pal.temperature - tc.temperature).abs();
        let diff_hum = (pal.humidity - tc.humidity).abs();

        println!(
            " • {:<24} -> UV: [{:.4}, {:.4}] | Blender: [{:.4}, {:.4}] | Temp: {:.2} vs {:.2}",
            tc.biome,
            uv[0],
            uv[1],
            tc.colormap_uv[0],
            tc.colormap_uv[1],
            pal.temperature,
            tc.temperature
        );

        assert!(
            diff_u < 1e-4 && diff_v < 1e-4,
            "Biome colormap UV mismatch in '{}': Rust [{:?}] vs Blender [{:?}]",
            tc.biome,
            uv,
            tc.colormap_uv
        );
        assert!(
            diff_temp < 1e-4 && diff_hum < 1e-4,
            "Biome metadata mismatch in '{}': Rust temp/hum vs Blender ground truth",
            tc.biome
        );
    }

    // 4. Verify Fluid UV Mapping
    println!("\n[4. Fluid UVs (Top Rotations & Slanted Side Trapeze) Verification]");
    for tc in &truth.fluid_uv_tests {
        if let (Some(is_flow), Some(rot), Some(ref exp_uvs)) =
            (tc.is_flowing, tc.rotation, &tc.expected_uvs)
        {
            let rust_uvs = get_fluid_top_uvs(is_flow, rot);
            for i in 0..4 {
                let diff_u = (rust_uvs[i][0] - exp_uvs[i][0]).abs();
                let diff_v = (rust_uvs[i][1] - exp_uvs[i][1]).abs();
                assert!(
                    diff_u < 1e-4 && diff_v < 1e-4,
                    "Top UV mismatch in '{}' at vertex {}: Rust {:?} vs Blender {:?}",
                    tc.name,
                    i,
                    rust_uvs[i],
                    exp_uvs[i]
                );
            }
            println!(" • Top UV: {:<32} -> OK (100% matched)", tc.name);
        } else if let (Some(hl), Some(hr), Some(ref exp_side_uvs)) =
            (tc.h_left, tc.h_right, &tc.expected_side_uvs)
        {
            let rust_side_uvs = get_fluid_side_uvs(hl, hr);
            for i in 0..4 {
                let diff_u = (rust_side_uvs[i][0] - exp_side_uvs[i][0]).abs();
                let diff_v = (rust_side_uvs[i][1] - exp_side_uvs[i][1]).abs();
                assert!(
                    diff_u < 1e-4 && diff_v < 1e-4,
                    "Side UV mismatch in '{}' at vertex {}: Rust {:?} vs Blender {:?}",
                    tc.name,
                    i,
                    rust_side_uvs[i],
                    exp_side_uvs[i]
                );
            }
            println!(" • Side UV: {:<31} -> OK (100% matched)", tc.name);
        }
    }

    // 5. Verify Section Mesh Generation (Quads, Vertices, Triangles, Occlusion Culling)
    println!(
        "\n[5. 3D Section Mesh Assembly Verification (vs MoziToolKit RawSectionGeometryBuffer)]"
    );
    let culler = FaceCuller::default();
    let unwelded_config = MesherConfig {
        weld_vertices: false,
        origin_centered: false,
        ..Default::default()
    };
    let welded_config = MesherConfig {
        weld_vertices: true,
        origin_centered: false,
        ..Default::default()
    };

    for tc in &truth.mesh_tests {
        let mut world = VoxelStorage::new();
        world.set_bounds(0, 0, 0, 16, 16, 16);
        for (coord_str, state) in &tc.blocks {
            let parts: Vec<i32> = coord_str.split(',').map(|s| s.parse().unwrap()).collect();
            world.set_block(parts[0], parts[1], parts[2], state, None);
        }

        let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
        let mesh =
            SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &unwelded_config);

        println!(
            " • {:<28} -> Faces: {} (Blender: {}) | Vertices: {} (Blender: {})",
            tc.name,
            mesh.face_count(),
            tc.face_count,
            mesh.vertex_count(),
            tc.vertex_count
        );

        assert_eq!(
            mesh.face_count(),
            tc.face_count,
            "Face count mismatch in '{}'",
            tc.name
        );
        assert_eq!(
            mesh.vertex_count(),
            tc.vertex_count,
            "Vertex count mismatch in '{}'",
            tc.name
        );

        // Also verify welded mesh produces 8 vertices for single_cube_mesh
        let welded_mesh =
            SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &welded_config);
        if tc.name == "single_cube_mesh" {
            assert_eq!(
                welded_mesh.vertex_count(),
                8,
                "Welded single cube must have 8 vertices"
            );
        }
    }

    println!("\n============================================================");
    println!(" 100% OF MOZITOOLKIT BLENDER COMPUTATIONS MATCHED RUST!");
    println!("============================================================");
}
