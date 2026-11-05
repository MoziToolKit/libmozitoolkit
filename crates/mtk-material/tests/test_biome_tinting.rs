use mtk_material::{
    BiomeResolver, TINT_TYPE_DRY_FOLIAGE, TINT_TYPE_FOLIAGE, TINT_TYPE_GRASS, TINT_TYPE_HARDCODED,
    TINT_TYPE_NONE,
};

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
    let dirt_neg = resolver.get_tint_info("dirt", Some("grass_block"), Some(-1));
    assert_eq!(dirt_neg.tint_category, "none");
    let top_neg = resolver.get_tint_info("grass_block_top", Some("grass_block"), Some(-1));
    assert_eq!(top_neg.tint_category, "none");
    let oak_neg = resolver.get_tint_info("oak_leaves", Some("oak_leaves"), Some(-1));
    assert_eq!(oak_neg.tint_category, "none");

    // Multi-layer flora: pink petals (layer 0 = petals untinted, layer 1 = stem grass)
    let pp_0 = resolver.get_tint_info("pink_petals", Some("pink_petals"), Some(0));
    assert_eq!(pp_0.tint_category, "none");
    let pp_1 = resolver.get_tint_info("pink_petals_stem", Some("pink_petals"), Some(1));
    assert_eq!(pp_1.tint_category, "grass");
    let pp_neg = resolver.get_tint_info("pink_petals", Some("pink_petals"), Some(-1));
    assert_eq!(pp_neg.tint_category, "none");
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
    use mtk_material::{
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
fn test_unit_cube_tint_indices() {
    use mtk_core::Direction;
    use mtk_material::get_unit_cube_tint_indices;

    let grass = get_unit_cube_tint_indices("minecraft:grass_block[snowy=false]");
    assert_eq!(grass[Direction::Up.to_index()], 0);
    assert_eq!(grass[Direction::Down.to_index()], -1);
    assert_eq!(grass[Direction::North.to_index()], -1);

    assert_eq!(get_unit_cube_tint_indices("minecraft:oak_leaves"), [0; 6]);
    assert_eq!(get_unit_cube_tint_indices("minecraft:stone"), [-1; 6]);
}
