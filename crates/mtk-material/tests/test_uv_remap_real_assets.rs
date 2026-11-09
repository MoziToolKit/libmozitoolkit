use mtk_material::MaterialResolver;
use mtk_resource::{AtlasCategory, ResourcePackStack, ZipPack};
use mtk_testkit::{fabric_jar, read_fixture, resource_pack_zip};
use mtk_texture::{AtlasBuilder, AtlasBuilderConfig, AtlasSpriteLocation};

fn remap_local_to_atlas(u: f32, v: f32, sprite: &AtlasSpriteLocation) -> [f32; 2] {
    let [u_min, v_min, u_max, v_max] = sprite.frame_0_uv_bounds;
    [u_min + u * (u_max - u_min), v_min + v * (v_max - v_min)]
}

#[test]
fn test_uv_remap_real_fabric_jar_and_spbr() {
    let Some(jar_path) = fabric_jar() else {
        eprintln!(
            "Skipping test: Minecraft jar not found (set MTK_TEST_JAR or place ../26.2-Fabric.jar)"
        );
        return;
    };

    let mut stack = ResourcePackStack::new();

    // 1. Mount base vanilla jar
    let jar_pack = ZipPack::from_file("vanilla", &jar_path).expect("Failed to open vanilla jar");
    stack.push_pack(Box::new(jar_pack));

    // 2. Mount SPBR resource pack if present
    if let Some(zip_path) = resource_pack_zip() {
        if let Ok(spbr_pack) = ZipPack::from_file("spbr", &zip_path) {
            stack.push_pack(Box::new(spbr_pack));
        }
    }

    // 3. Bake full Atlas for Blocks category
    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: 2048,
        max_height: 2048,
        mip_level: 0,
        padding: 0,
    });

    let baked_atlas = builder
        .build_category(&stack, &AtlasCategory::Blocks)
        .expect("Failed to bake blocks atlas from real resource packs");

    let address_map = &baked_atlas.address_map;
    assert!(!address_map.chunks.is_empty());
    assert!(!address_map.sprites.is_empty());

    println!(
        "Bake success: {} chunks, {} sprites",
        address_map.chunks.len(),
        address_map.sprites.len()
    );

    // =========================================================================
    // Scenario 1: Standard Static Block (e.g. Stone, Dirt, Oak Planks)
    // =========================================================================
    let (stone_res, stone_sprite) = MaterialResolver::resolve("block/stone", None, address_map)
        .expect("Failed to resolve canonical stone");

    assert_eq!(stone_res.path, "block/stone");
    assert!(!stone_sprite.is_animated);
    assert_eq!(stone_sprite.frame_count, 1);

    // Normal UV [0.0..1.0] -> Atlas Bounds
    let stone_bl = remap_local_to_atlas(0.0, 0.0, stone_sprite);
    let stone_tr = remap_local_to_atlas(1.0, 1.0, stone_sprite);
    assert_eq!(
        stone_bl,
        [
            stone_sprite.frame_0_uv_bounds[0],
            stone_sprite.frame_0_uv_bounds[1]
        ]
    );
    assert_eq!(
        stone_tr,
        [
            stone_sprite.frame_0_uv_bounds[2],
            stone_sprite.frame_0_uv_bounds[3]
        ]
    );

    // Bedrock must resolve to block/bedrock, NOT red_bed!
    let (bedrock_res, _) = MaterialResolver::resolve("block/bedrock", None, address_map)
        .expect("Failed to resolve canonical bedrock");
    assert_eq!(bedrock_res.path, "block/bedrock");
    assert_ne!(bedrock_res.path, "block/red_bed");

    // =========================================================================
    // Scenario 2: Animated Texture Block (e.g. Water Still, Fire, Lava, Sea Lantern)
    // CRITICAL: Must map strictly into Frame 0, NOT stretch over full strip!
    // =========================================================================
    let animated_names = ["water_still", "lava_still", "fire_0", "sea_lantern"];
    for anim_name in animated_names {
        if let Some((anim_res, _)) =
            MaterialResolver::resolve(&format!("block/{}", anim_name), None, address_map)
        {
            if let Some(anim_sprite) = address_map.lookup_animated(&anim_res) {
                println!(
                    "Testing animated sprite: {} (frames={}, is_anim={})",
                    anim_res.as_string(),
                    anim_sprite.frame_count,
                    anim_sprite.is_animated
                );

                assert!(
                    anim_sprite.is_animated,
                    "Expected {} to be marked as animated",
                    anim_name
                );
                assert!(
                    anim_sprite.frame_count > 1,
                    "Expected {} to have multiple frames",
                    anim_name
                );

                // Verify that frame_0_uv_bounds height is exactly 1 / frame_count of strip height
                let strip_h = (anim_sprite.uv_bounds[3] - anim_sprite.uv_bounds[1]).abs();
                let frame_0_h =
                    (anim_sprite.frame_0_uv_bounds[3] - anim_sprite.frame_0_uv_bounds[1]).abs();
                let expected_f0_h = strip_h / (anim_sprite.frame_count as f32);

                assert!(
                    (frame_0_h - expected_f0_h).abs() < 1e-4,
                    "Frame 0 height ratio mismatch for {}: got {}, expected {}",
                    anim_name,
                    frame_0_h,
                    expected_f0_h
                );

                // Remap full [0..1] input quad
                let uv_bl = remap_local_to_atlas(0.0, 0.0, anim_sprite);
                let uv_tr = remap_local_to_atlas(1.0, 1.0, anim_sprite);

                // Top-Right mapped V should match Frame 0 max V, NEVER strip max V!
                assert_eq!(uv_bl[1], anim_sprite.frame_0_uv_bounds[1]);
                assert_eq!(uv_tr[1], anim_sprite.frame_0_uv_bounds[3]);

                // Mapped height MUST be frame_0_h, NOT strip_h!
                let mapped_height = (uv_tr[1] - uv_bl[1]).abs();
                assert!(
                    (mapped_height - frame_0_h).abs() < 1e-5,
                    "Mapped height {} must equal frame 0 height {}",
                    mapped_height,
                    frame_0_h
                );
            }
        }
    }

    // =========================================================================
    // Scenario 3: Non-Cube / Special Model Sub-Rectangle UV (e.g. Torch, Lever, Slab)
    // CRITICAL: Sub-rectangle UVs (e.g. 7..9 px) must NOT stretch to full sprite cell!
    // =========================================================================
    if let Some((torch_res, torch_sprite)) = MaterialResolver::resolve("torch", None, address_map) {
        println!(
            "Testing torch sub-rectangle UV on {}",
            torch_res.as_string()
        );

        // A standard torch side in Minecraft is 2 pixels wide (7/16 to 9/16) and 10 pixels high (6/16 to 16/16)
        let local_u_min = 7.0 / 16.0;
        let local_u_max = 9.0 / 16.0;
        let local_v_min = 6.0 / 16.0;
        let local_v_max = 16.0 / 16.0;

        let remapped_bl = remap_local_to_atlas(local_u_min, local_v_min, torch_sprite);
        let remapped_tr = remap_local_to_atlas(local_u_max, local_v_max, torch_sprite);

        let cell_w = torch_sprite.frame_0_uv_bounds[2] - torch_sprite.frame_0_uv_bounds[0];
        let cell_h = torch_sprite.frame_0_uv_bounds[3] - torch_sprite.frame_0_uv_bounds[1];

        let remapped_w = remapped_tr[0] - remapped_bl[0];
        let remapped_h = remapped_tr[1] - remapped_bl[1];

        // Torch width should be exactly 2/16 (1/8) of the cell width
        assert!(
            (remapped_w - cell_w * (2.0 / 16.0)).abs() < 1e-5,
            "Torch remapped width mismatch: got {}, expected {}",
            remapped_w,
            cell_w * (2.0 / 16.0)
        );

        // Torch height should be exactly 10/16 of the cell height
        assert!(
            (remapped_h - cell_h * (10.0 / 16.0)).abs() < 1e-5,
            "Torch remapped height mismatch: got {}, expected {}",
            remapped_h,
            cell_h * (10.0 / 16.0)
        );
    }

    // =========================================================================
    // Scenario 4: Real Material Names from Ice Cube Asset Library (2,949 materials)
    // =========================================================================
    if let Some(content) = read_fixture("icecube_materials.json") {
        let mat_names: Vec<String> = serde_json::from_str(&content).unwrap_or_default();

        let mut resolved_count = 0usize;
        let mut unmapped_samples = Vec::new();

        for name in &mat_names {
            if let Some((_, sprite)) = MaterialResolver::resolve(name, None, address_map) {
                resolved_count += 1;
                // Verify that remapping random UV always stays within bounds
                let uv_mid = remap_local_to_atlas(0.5, 0.5, sprite);
                assert!(
                    uv_mid[0] >= sprite.frame_0_uv_bounds[0]
                        && uv_mid[0] <= sprite.frame_0_uv_bounds[2]
                );
            } else if unmapped_samples.len() < 10 {
                unmapped_samples.push(name.clone());
            }
        }

        println!(
            "\n[Ice Cube Library Resolution Stats]: {} / {} materials resolved ({:.1}%)",
            resolved_count,
            mat_names.len(),
            (resolved_count as f64) / (mat_names.len() as f64) * 100.0
        );
        if !unmapped_samples.is_empty() {
            println!(
                "Unmapped material samples (entities/custom): {:?}",
                unmapped_samples
            );
        }
    }
}
