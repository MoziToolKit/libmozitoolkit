use super::*;

#[test]
fn test_redstone_wire_signal_palettes() {
    let srgb_0 = get_redstone_wire_srgb(0);
    assert!((srgb_0[0] - 0.3).abs() < 1e-4);
    assert_eq!(srgb_0[1], 0.0);
    assert_eq!(srgb_0[2], 0.0);
    assert_eq!(get_redstone_wire_hex(0), "#4B0000");

    let srgb_15 = get_redstone_wire_srgb(15);
    assert!((srgb_15[0] - 1.0).abs() < 1e-4);
    assert!((srgb_15[1] - 0.15).abs() < 1e-4);
    assert_eq!(srgb_15[2], 0.0);
    assert_eq!(get_redstone_wire_hex(15), "#FF2600");

    let lin_0 = get_redstone_wire_color(0);
    let lin_15 = get_redstone_wire_color(15);
    assert!(lin_15[0] > lin_0[0]);
    assert_eq!(lin_0[3], 1.0);
    assert_eq!(lin_15[3], 1.0);
}

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
fn test_unit_cube_tint_indices() {
    let grass_indices = get_unit_cube_tint_indices("grass_block");
    assert_eq!(grass_indices[Direction::Up.to_index()], 0);
    assert_eq!(grass_indices[Direction::Down.to_index()], -1);
    assert_eq!(grass_indices[Direction::North.to_index()], -1);

    let leaves_indices = get_unit_cube_tint_indices("oak_leaves");
    assert_eq!(leaves_indices, [0; 6]);

    let dirt_indices = get_unit_cube_tint_indices("dirt");
    assert_eq!(dirt_indices, [-1; 6]);

    let stone_indices = get_unit_cube_tint_indices("stone");
    assert_eq!(stone_indices, [-1; 6]);
}
