use mtk_material::biome::{
    classify_tint_category, BiomeResolver, TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE,
    TINT_TYPE_GRASS, TINT_TYPE_HARDCODED, TINT_TYPE_NONE,
};

#[test]
fn test_classify_tint_category_authoritative() {
    // 1. None (Untinted natural textures)
    assert_eq!(
        classify_tint_category("dead_bush", Some("dead_bush"), None),
        "none"
    );
    assert_eq!(
        classify_tint_category("dead_bush", Some("dead_bush"), Some(0)),
        "none"
    );
    assert_eq!(
        classify_tint_category("potted_dead_bush", Some("potted_dead_bush"), None),
        "none"
    );
    assert_eq!(
        classify_tint_category("azalea_leaves", Some("azalea_leaves"), None),
        "none"
    );
    assert_eq!(
        classify_tint_category("cherry_leaves", Some("cherry_leaves"), None),
        "none"
    );
    assert_eq!(
        classify_tint_category("pale_oak_leaves", Some("pale_oak_leaves"), None),
        "none"
    );
    assert_eq!(
        classify_tint_category("yellow_poplar_leaves", Some("yellow_poplar_leaves"), None),
        "none"
    );
    assert_eq!(
        classify_tint_category("red_poplar_leaves", None, None),
        "none"
    );
    assert_eq!(classify_tint_category("leaves_notint", None, None), "none");

    // 2. Dry Foliage
    assert_eq!(
        classify_tint_category("leaf_litter", Some("leaf_litter"), Some(0)),
        "dry_foliage"
    );
    assert_eq!(
        classify_tint_category("short_dry_grass", Some("short_dry_grass"), Some(0)),
        "dry_foliage"
    );
    assert_eq!(
        classify_tint_category("tall_dry_grass", Some("tall_dry_grass"), Some(0)),
        "dry_foliage"
    );

    // 3. Grass
    assert_eq!(
        classify_tint_category("short_grass", Some("short_grass"), Some(0)),
        "grass"
    );
    assert_eq!(
        classify_tint_category("bush", Some("bush"), Some(0)),
        "grass"
    );
    assert_eq!(
        classify_tint_category("grass_block_top", Some("grass_block"), Some(0)),
        "grass"
    );
    assert_eq!(
        classify_tint_category("sugar_cane", Some("sugar_cane"), Some(0)),
        "grass"
    );

    // 4. Foliage
    assert_eq!(
        classify_tint_category("oak_leaves", Some("oak_leaves"), Some(0)),
        "foliage"
    );
    assert_eq!(
        classify_tint_category("jungle_leaves", Some("jungle_leaves"), Some(0)),
        "foliage"
    );
    assert_eq!(
        classify_tint_category("vine", Some("vine"), Some(0)),
        "foliage"
    );

    // 5. Hardcoded
    assert_eq!(
        classify_tint_category("spruce_leaves", Some("spruce_leaves"), Some(0)),
        "hardcoded"
    );
    assert_eq!(
        classify_tint_category("birch_leaves", Some("birch_leaves"), Some(0)),
        "hardcoded"
    );
    assert_eq!(
        classify_tint_category("lily_pad", Some("lily_pad"), Some(0)),
        "hardcoded"
    );
}

#[test]
fn test_biome_resolver_seeded_and_tint_info() {
    let resolver = BiomeResolver::new();

    // dead_bush must be TINT_TYPE_NONE with tint_weight 0.0
    let db_info = resolver.get_tint_info("dead_bush", Some("dead_bush"), None);
    assert_eq!(db_info.tint_type, TINT_TYPE_NONE);
    assert_eq!(db_info.tint_weight, 0.0);
    assert_eq!(db_info.tint_category, "none");

    // leaf_litter must be TINT_TYPE_DRY_FOLIAGE
    let ll_info = resolver.get_tint_info("leaf_litter", Some("leaf_litter"), Some(0));
    assert_eq!(ll_info.tint_type, TINT_TYPE_DRY_FOLIAGE);
    assert_eq!(ll_info.tint_weight, 1.0);
    assert_eq!(ll_info.tint_category, "dry_foliage");

    // bush must be TINT_TYPE_GRASS
    let bush_info = resolver.get_tint_info("bush", Some("bush"), Some(0));
    assert_eq!(bush_info.tint_type, TINT_TYPE_GRASS);
    assert_eq!(bush_info.tint_weight, 1.0);
    assert_eq!(bush_info.tint_category, "grass");

    // oak_leaves must be TINT_TYPE_FOLIAGE
    let oak_info = resolver.get_tint_info("oak_leaves", Some("oak_leaves"), Some(0));
    assert_eq!(oak_info.tint_type, TINT_TYPE_FOLIAGE);
    assert_eq!(oak_info.tint_weight, 1.0);
    assert_eq!(oak_info.tint_category, "foliage");

    // spruce_leaves must be TINT_TYPE_HARDCODED
    let spruce_info = resolver.get_tint_info("spruce_leaves", Some("spruce_leaves"), Some(0));
    assert_eq!(spruce_info.tint_type, TINT_TYPE_HARDCODED);
    assert!(spruce_info.is_hardcoded);
    assert!(spruce_info.hardcoded_color.is_some());
    assert_eq!(spruce_info.hardcoded_hex.as_deref(), Some("#619961"));

    // grass_block_side has overlay
    assert_eq!(
        resolver.get_overlay_texture("grass_block_side"),
        Some("grass_block_side_overlay")
    );
}

#[test]
fn test_grass_block_dirt_untinted_and_negative_tint_index() {
    let resolver = BiomeResolver::new();

    // Bottom dirt face of grass block with tint_index = -1 must NOT be tinted
    let dirt_info = resolver.get_tint_info("dirt", Some("grass_block"), Some(-1));
    assert_eq!(dirt_info.tint_type, TINT_TYPE_NONE);
    assert_eq!(dirt_info.tint_weight, 0.0);
    assert_eq!(dirt_info.tint_category, "none");

    // Face with explicit negative tint_index is always untinted
    assert_eq!(
        classify_tint_category("dirt", Some("grass_block"), Some(-1)),
        "none"
    );
    assert_eq!(
        classify_tint_category("grass_block_top", Some("grass_block"), Some(-1)),
        "none"
    );
    assert_eq!(
        classify_tint_category("oak_leaves", Some("oak_leaves"), Some(-1)),
        "none"
    );

    // Multi-layer flora: pink petals (layer 0 = petals untinted, layer 1 = stem grass)
    assert_eq!(
        classify_tint_category("pink_petals", Some("pink_petals"), Some(0)),
        "none"
    );
    assert_eq!(
        classify_tint_category("pink_petals_stem", Some("pink_petals"), Some(1)),
        "grass"
    );
    assert_eq!(
        classify_tint_category("pink_petals", Some("pink_petals"), Some(-1)),
        "none"
    );
}

#[test]
fn test_unit_cube_tint_index() {
    use mtk_core::Direction;

    // Grass block: Up is 0, Down (dirt) and sides are -1
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("grass_block", Direction::Up),
        0
    );
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("grass_block", Direction::Down),
        -1
    );
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("grass_block", Direction::North),
        -1
    );

    // Leaves: All directions are 0
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("oak_leaves", Direction::Up),
        0
    );
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("oak_leaves", Direction::Down),
        0
    );

    // Untinted blocks: -1
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("dirt", Direction::Up),
        -1
    );
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("stone", Direction::Up),
        -1
    );
    assert_eq!(
        mtk_material::get_unit_cube_tint_index("dead_bush", Direction::Up),
        -1
    );
}

#[test]
fn test_biome_resolver_model_inheritance() {
    use serde_json::json;
    use std::collections::HashMap;

    let mut resolver = BiomeResolver::new();
    let mut models = HashMap::new();

    // Parent model defining textures and elements
    models.insert(
        "block/parent_leaves".to_string(),
        json!({
            "textures": {
                "all": "custom_oak_leaves"
            },
            "elements": [
                {
                    "faces": {
                        "all": {
                            "texture": "#all",
                            "tintindex": 0
                        }
                    }
                }
            ]
        }),
    );

    // Child model inheriting from parent_leaves
    models.insert(
        "block/oak_leaves".to_string(),
        json!({
            "parent": "block/parent_leaves",
            "textures": {
                "all": "05m_oak_leaves_cube"
            }
        }),
    );

    resolver.set_models(models);

    // Child should inherit elements and resolve custom texture as foliage
    let info = resolver.get_tint_info("05m_oak_leaves_cube", Some("oak_leaves"), Some(0));
    assert_eq!(info.tint_type, TINT_TYPE_FOLIAGE);
    assert_eq!(info.tint_category, "foliage");
    assert_eq!(info.tint_weight, 1.0);
}

#[test]
fn test_reapply_uniform_biome_tint_preserves_hardcoded_and_custom_override() {
    use mtk_material::biome::{
        reapply_uniform_biome_tint, BiomeColorSet, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS,
        TINT_TYPE_HARDCODED, TINT_TYPE_NONE, TINT_TYPE_WATER,
    };

    let base = vec![
        TINT_TYPE_NONE,
        TINT_TYPE_GRASS,
        TINT_TYPE_FOLIAGE,
        TINT_TYPE_WATER,
        TINT_TYPE_HARDCODED,
    ];
    let packed = vec![
        [0.0, 0.0, 0.0, 0.0],
        [0.5, 0.0, 1.0, 1.0],
        [0.25, 0.25, 0.5, 2.0],
        [0.0, 0.0, 1.0, 3.0],
        [0.0, 0.0, 1.0, 4.0],
    ];
    let existing = vec![
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.9, 0.1, 0.2, 1.0],
    ];
    let colors = BiomeColorSet {
        grass: [0.2, 0.8, 0.2, 1.0],
        foliage: [0.1, 0.6, 0.1, 1.0],
        dry_foliage: [0.5, 0.4, 0.1, 1.0],
        water: [0.1, 0.2, 0.9, 1.0],
    };

    let out = reapply_uniform_biome_tint(&base, &packed, &existing, colors, false, false, false);
    assert_eq!(out.packed_tint_data.len(), 5);
    assert_eq!(out.packed_tint_data[1], [0.5, 0.0, 1.0, 1.0]);
    assert_eq!(out.tint_colors[1], colors.grass);
    assert_eq!(out.tint_colors[2], colors.foliage);
    assert_eq!(out.tint_colors[3], colors.water);
    // Hardcoded color is preserved.
    assert_eq!(out.tint_colors[4], existing[4]);
    assert_eq!(out.packed_tint_data[4][3], TINT_TYPE_HARDCODED as f32);

    // Custom grass override keeps the new grass color but routes to HARDCODED.
    let custom = reapply_uniform_biome_tint(&base, &packed, &existing, colors, true, false, false);
    assert_eq!(custom.packed_tint_data[1][3], TINT_TYPE_HARDCODED as f32);
    assert_eq!(custom.tint_colors[1], colors.grass);
    // Non-grass categories are untouched by the grass flag.
    assert_eq!(custom.packed_tint_data[2][3], TINT_TYPE_FOLIAGE as f32);
    assert_eq!(custom.tint_colors[3], colors.water);
}

#[test]
fn unit_cube_tint_indices_match_per_direction_api() {
    use mtk_core::Direction;
    use mtk_material::biome::{get_unit_cube_tint_index, get_unit_cube_tint_indices};

    let mut corpus: Vec<String> = Vec::new();
    for &(name, _) in mtk_material::BLOCK_TINT_REGISTRY {
        corpus.push(format!("minecraft:{name}"));
        corpus.push(format!("minecraft:{name}[waterlogged=false]"));
    }
    for &(name, _) in mtk_material::HARDCODED_BLOCK_TINTS {
        corpus.push(format!("minecraft:{name}"));
    }
    corpus.extend(
        [
            "minecraft:stone",
            "minecraft:dirt",
            "minecraft:grass_block[snowy=false]",
            "minecraft:grass_block[snowy=true]",
            "minecraft:grass",
            "minecraft:oak_stairs[facing=north,half=bottom,shape=straight]",
            "minecraft:oak_slab[type=double]",
            "minecraft:water[level=0]",
            "minecraft:lava[level=3]",
            "minecraft:glass",
            "mod:custom_block",
            "minecraft:unknown_plant",
        ]
        .map(str::to_string),
    );

    for state in &corpus {
        let indices = get_unit_cube_tint_indices(state);
        for dir in Direction::ALL {
            assert_eq!(
                indices[dir.to_index()],
                get_unit_cube_tint_index(state, dir),
                "per-direction tint index mismatch for {state}"
            );
        }
    }

    // Spot-check the documented semantics.
    let grass = get_unit_cube_tint_indices("minecraft:grass_block[snowy=false]");
    assert_eq!(grass[Direction::Up.to_index()], 0);
    assert_eq!(grass[Direction::Down.to_index()], -1);
    assert_eq!(grass[Direction::North.to_index()], -1);

    assert_eq!(get_unit_cube_tint_indices("minecraft:oak_leaves"), [0; 6]);
    assert_eq!(get_unit_cube_tint_indices("minecraft:stone"), [-1; 6]);
}

#[test]
fn test_resident_colormap_sampling_and_modifiers() {
    use mtk_material::biome::{
        compute_resolved_tint_color, get_default_colormaps, linear_to_srgb, TINT_TYPE_GRASS,
    };

    let cms = get_default_colormaps();
    assert_eq!(cms.grass_pixels.len(), 256 * 256 * 3);
    assert_eq!(cms.foliage_pixels.len(), 256 * 256 * 3);
    assert_eq!(cms.dry_foliage_pixels.len(), 256 * 256 * 3);

    // 1. Plains sample at temp=0.8, downfall=0.4
    let plains_srgb = cms.sample_grass_srgb(0.8, 0.4);
    let pr = (plains_srgb[0] * 255.0).round() as u8;
    let pg = (plains_srgb[1] * 255.0).round() as u8;
    let pb = (plains_srgb[2] * 255.0).round() as u8;
    // Canonical Plains grass: #91BD59 -> (145, 189, 89)
    assert_eq!((pr, pg, pb), (145, 189, 89));

    // 2. Dark forest grass modifier:
    // Base forest (temp=0.7, downfall=0.8) is #79C05A -> (121, 192, 90)
    // Modified: ((base & 0xFEFEFE) + 0x28340A) >> 1 -> #507A32 -> (80, 122, 50)
    let df_grass = compute_resolved_tint_color(TINT_TYPE_GRASS, 0.7, 0.8, true);
    let df_r = (linear_to_srgb(df_grass[0]) * 255.0).round() as u8;
    let df_g = (linear_to_srgb(df_grass[1]) * 255.0).round() as u8;
    let df_b = (linear_to_srgb(df_grass[2]) * 255.0).round() as u8;
    assert_eq!((df_r, df_g, df_b), (80, 122, 50));

    // 3. Foliage sample in dark forest directly from colormap: (89, 174, 48)
    // In vanilla ColorMapColorUtil, temp=0.7, downfall=0.8 truncates to x=76, y=112 -> (89, 174, 48)
    let fol_srgb = cms.sample_foliage_srgb(0.7, 0.8);
    let fol_r = (fol_srgb[0] * 255.0).round() as u8;
    let fol_g = (fol_srgb[1] * 255.0).round() as u8;
    let fol_b = (fol_srgb[2] * 255.0).round() as u8;
    assert_eq!((fol_r, fol_g, fol_b), (89, 174, 48));
}
