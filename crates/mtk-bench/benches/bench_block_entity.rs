#![forbid(unsafe_code)]
//! Benchmark for block entity meshing performance and bit-for-bit mesh regression verification.

use std::time::Instant;

use glam::IVec3;
use mtk_core::MeshData;
use mtk_cull::FaceCuller;
use mtk_voxel::mesher::shading::ModelSource;
use mtk_voxel::storage::{block_index, SectionStorage};
use mtk_voxel::{BlockEntityData, MesherConfig, SectionMesher};

/// FNV-1a checksum over all mesh buffers and attributes for bit-for-bit verification.
pub fn mesh_checksum(m: &MeshData) -> u64 {
    fn feed(h: &mut u64, bytes: &[u8]) {
        for &b in bytes {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    fn feed_f32(h: &mut u64, values: &[f32]) {
        for v in values {
            feed(h, &v.to_bits().to_le_bytes());
        }
    }

    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    feed_f32(
        &mut h,
        &m.positions.iter().flatten().copied().collect::<Vec<_>>(),
    );
    feed_f32(
        &mut h,
        &m.normals.iter().flatten().copied().collect::<Vec<_>>(),
    );
    feed_f32(&mut h, &m.uvs.iter().flatten().copied().collect::<Vec<_>>());
    for i in &m.indices {
        feed(&mut h, &i.to_le_bytes());
    }
    for slot in &m.face_materials {
        feed(&mut h, &slot.to_le_bytes());
    }
    for idx in &m.face_tint_indices {
        feed(&mut h, &idx.to_le_bytes());
    }
    let mut names: Vec<_> = m.custom_attributes.keys().cloned().collect();
    names.sort();
    for name in names {
        feed(&mut h, name.as_bytes());
        feed(
            &mut h,
            format!("{:?}", m.custom_attributes[&name]).as_bytes(),
        );
    }
    h
}

/// Creates a pure terrain section with 4,096 solid voxels and 0 block entities.
fn create_sample_a_pure_terrain() -> SectionStorage {
    let mut sec = SectionStorage::new(IVec3::new(0, 0, 0));
    sec.palette.push("minecraft:stone".to_string());
    for i in 0..4096 {
        sec.voxels[i] = 1;
    }
    sec.non_air_count = 4096;
    sec
}

/// Creates a section containing 16 decorated pots with various facings and sherd patterns.
fn create_sample_b_decorated_pots() -> SectionStorage {
    let mut sec = SectionStorage::new(IVec3::new(0, 0, 0));

    // Ground floor
    let stone_idx = sec.palette.len() as u16;
    sec.palette.push("minecraft:stone".to_string());
    for lx in 0..16 {
        for lz in 0..16 {
            let idx = block_index(lx, 0, lz);
            sec.voxels[idx] = stone_idx;
        }
    }

    let facings = ["north", "south", "east", "west"];
    let sherd_combos: [&[Option<&str>; 4]; 4] = [
        &[Some("angler"), None, Some("miner"), Some("blade")],
        &[Some("archer"), Some("burn"), None, Some("danger")],
        &[None, Some("explorer"), Some("friend"), Some("heart")],
        &[Some("prize"), Some("snort"), Some("skull"), None],
    ];

    let mut pot_pal_indices = Vec::new();
    for f in &facings {
        let p_idx = sec.palette.len() as u16;
        sec.palette
            .push(format!("minecraft:decorated_pot[facing={}]", f));
        pot_pal_indices.push(p_idx);
    }

    // Place 16 pots (4x4 grid at Y=1)
    for (i, &f_idx) in pot_pal_indices.iter().enumerate() {
        for (j, combo) in sherd_combos.iter().enumerate() {
            let lx = i * 4 + 1;
            let lz = j * 4 + 1;
            let idx = block_index(lx, 1, lz) as u16;
            sec.voxels[idx as usize] = f_idx;

            let sherds = [
                combo[0].map(|s| format!("minecraft:{}_pottery_sherd", s)),
                combo[1].map(|s| s.to_string()),
                combo[2].map(|s| format!("{}_pottery_sherd", s)),
                combo[3].map(|s| format!("minecraft:{}", s)),
            ];

            sec.set_block_entity(idx, Some(BlockEntityData::DecoratedPot { sherds }));
        }
    }

    sec.non_air_count = 256 + 16;
    sec
}

fn bench_section(name: &str, sec: &SectionStorage, iters: usize) -> (f64, MeshData) {
    let padded = sec.build_padded_array(|_, _, _| "minecraft:air");
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    // Warm-up
    let _ = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    let start = Instant::now();
    let mut last_mesh = MeshData::default();
    for _ in 0..iters {
        last_mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);
    }
    let elapsed = start.elapsed();
    let avg_us = (elapsed.as_secs_f64() * 1_000_000.0) / iters as f64;

    println!(
        "{:<35} | iters: {:<4} | avg: {:>7.2} µs | quads: {:<5} | verts: {:<5}",
        name,
        iters,
        avg_us,
        last_mesh.quad_count(),
        last_mesh.vertex_count()
    );

    (avg_us, last_mesh)
}

fn main() {
    println!("============================================================");
    println!(" BlockEntity & SectionMesher Performance Baseline Benchmark ");
    println!("============================================================");

    // 1. Sample A: Pure Terrain (4096 solid voxels, 0 entities)
    let sec_a = create_sample_a_pure_terrain();
    let (_, mesh_a) = bench_section("Sample A (4096 Terrain, 0 Entities)", &sec_a, 500);
    let checksum_a = mesh_checksum(&mesh_a);
    println!("  -> Checksum A: 0x{:016x}", checksum_a);

    // 2. Sample B: 16 Decorated Pots with different facings and sherds
    let sec_b = create_sample_b_decorated_pots();
    let (_, mesh_b) = bench_section("Sample B (16 Decorated Pots + Floor)", &sec_b, 500);
    let checksum_b = mesh_checksum(&mesh_b);
    println!("  -> Checksum B: 0x{:016x}", checksum_b);

    // 3. Sample C: Ghost entity edge case check
    println!("\n--- Checking Sample C: Ghost Entity Persistence in SectionStorage ---");
    let mut sec_c = SectionStorage::new(IVec3::new(0, 0, 0));
    let pot_pos = block_index(3, 1, 3) as u16;
    sec_c.set_local(3, 1, 3, "minecraft:decorated_pot[facing=north]");
    sec_c.set_block_entity(
        pot_pos,
        Some(BlockEntityData::DecoratedPot {
            sherds: [Some("angler".to_string()), None, None, None],
        }),
    );
    assert!(sec_c.get_block_entity(pot_pos).is_some());

    // Replace pot with air (simulating player breaking pot)
    sec_c.set_local(3, 1, 3, "minecraft:air");
    let still_has_entity = sec_c.get_block_entity(pot_pos).is_some();
    println!(
        "  -> After setting to air: block_entity still in storage? {}",
        if still_has_entity {
            "YES (BUG / GHOST LEAK DETECTED)"
        } else {
            "NO (CLEANED)"
        }
    );

    println!("\nBaseline collection complete.\n");
}
