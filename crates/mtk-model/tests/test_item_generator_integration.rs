#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use mtk_model::item::{bake_item_from_png_bytes, bake_item_generated_model, ItemMeshOptions};

fn find_item_textures_dir() -> Option<PathBuf> {
    // 1. Check Minecraft_Source adjacent in workspace
    let candidates = [
        PathBuf::from(
            "/Volumes/Data/code_projects/mtk/Minecraft_Source/assets/minecraft/textures/item",
        ),
        PathBuf::from("../Minecraft_Source/assets/minecraft/textures/item"),
        PathBuf::from("../../Minecraft_Source/assets/minecraft/textures/item"),
    ];

    for c in candidates {
        if c.is_dir() {
            return Some(c);
        }
    }

    if let Some(assets) = mtk_testkit::real_assets_root() {
        let p = assets.join("assets/minecraft/textures/item");
        if p.is_dir() {
            return Some(p);
        }
    }

    None
}

#[test]
fn test_item_generator_dimensions_and_manifold() {
    let mut pixels = vec![0u8; 16 * 16 * 4];
    // Draw an L-shaped sword-like pattern
    for i in 2..14 {
        let idx = (i * 16 + i) * 4;
        pixels[idx] = 200;
        pixels[idx + 1] = 200;
        pixels[idx + 2] = 255;
        pixels[idx + 3] = 255;
    }

    let model = bake_item_generated_model(
        &pixels,
        16,
        16,
        "minecraft:item/diagonal_line",
        &ItemMeshOptions::default(),
    )
    .expect("bake item");

    assert!(!model.elements.is_empty());
    for el in &model.elements {
        assert_eq!(el.faces.len(), 1);
        let face = el.faces.values().next().unwrap();
        // Thickness verification: -0.03125 to +0.03125 (total 0.0625 = 1/16)
        for v in &face.vertices {
            assert!(v.x >= -0.5001 && v.x <= 0.5001);
            assert!(v.y >= -0.5001 && v.y <= 0.5001);
            assert!(v.z >= -0.03126 && v.z <= 0.03126);
        }
        assert!(face.normal.length() > 0.99 && face.normal.length() < 1.01);
    }
}

#[test]
fn test_item_generator_bulk_vanilla_items_from_resource_pack() {
    let Some(item_dir) = find_item_textures_dir() else {
        eprintln!("Skipping bulk vanilla items test: item textures directory not found");
        return;
    };

    println!("Found item textures directory at: {:?}", item_dir);
    let mut entries: Vec<PathBuf> = fs::read_dir(&item_dir)
        .expect("read item dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "png"))
        .collect();

    entries.sort();
    assert!(!entries.is_empty(), "Must contain item PNG textures");

    let total = entries.len();
    let mut success = 0;
    let mut total_greedy_elements = 0;
    let mut total_unmerged_elements = 0;

    let options_greedy = ItemMeshOptions {
        use_greedy_meshing: true,
        ..Default::default()
    };
    let options_unmerged = ItemMeshOptions {
        use_greedy_meshing: false,
        ..Default::default()
    };

    let start_time = std::time::Instant::now();

    for path in &entries {
        let file_stem = path.file_stem().unwrap().to_string_lossy();
        let texture_id = format!("minecraft:item/{}", file_stem);
        let png_bytes = fs::read(path).expect("read png");

        let baked_greedy = match bake_item_from_png_bytes(&png_bytes, &texture_id, &options_greedy)
        {
            Ok(m) => m,
            Err(e) => panic!("Failed to bake item {}: {:?}", texture_id, e),
        };

        let baked_unmerged = bake_item_from_png_bytes(&png_bytes, &texture_id, &options_unmerged)
            .expect("bake unmerged");

        let greedy_count = baked_greedy.elements.len();
        let unmerged_count = baked_unmerged.elements.len();

        total_greedy_elements += greedy_count;
        total_unmerged_elements += unmerged_count;
        success += 1;
    }

    let elapsed = start_time.elapsed();
    let savings_percent = if total_unmerged_elements > 0 {
        100.0 * (1.0 - (total_greedy_elements as f64 / total_unmerged_elements as f64))
    } else {
        0.0
    };

    println!("============================================================");
    println!("  BULK VANILLA ITEM MODEL BAKING SUMMARY");
    println!("============================================================");
    println!("  Total Item Textures Tested: {}", total);
    println!("  Successfully Baked:         {} / {}", success, total);
    println!(
        "  Total Time Elapsed:         {:?} ({:.2} us/item)",
        elapsed,
        elapsed.as_micros() as f64 / total as f64
    );
    println!("  Total Quads (Unmerged):     {}", total_unmerged_elements);
    println!("  Total Quads (Greedy):       {}", total_greedy_elements);
    println!("  Greedy Face Reduction:      {:.2}%", savings_percent);
    println!("============================================================");

    assert_eq!(success, total);
    assert!(
        savings_percent > 30.0,
        "Greedy meshing must save substantial face count"
    );
}
