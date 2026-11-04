use mtk_material::biome::hardcoded::{
    get_redstone_wire_color, get_redstone_wire_hex, get_redstone_wire_srgb,
};
use mtk_material::resolver::MaterialResolver;
use mtk_material::BiomeResolver;
use mtk_resource::ResourceLocation;
use mtk_texture::{AtlasAddressMap, AtlasSpriteLocation, SpriteKind};

fn make_sprite_loc(chunk_id: u16, texture_id: u32, uv_bounds: [f32; 4]) -> AtlasSpriteLocation {
    AtlasSpriteLocation {
        chunk_id,
        category: "blocks".to_string(),
        is_animated: false,
        sprite_kind: SpriteKind::StaticAtlas,
        texture_id,
        uv_bounds,
        frame_0_uv_bounds: uv_bounds,
        local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
        frame_uv_step: [0.0, 0.0],
        pixel_rect: [0, 0, 16, 16],
        strip_pixel_rect: [0, 0, 16, 16],
        frame_size: [16, 16],
        frame_count: 1,
        animation: None,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        average_color: None,
        slot_index: None,
    }
}

#[test]
fn test_redstone_wire_signal_palettes() {
    // Power 0: #4B0000 / rgb(0.3, 0.0, 0.0)
    let srgb_0 = get_redstone_wire_srgb(0);
    assert!((srgb_0[0] - 0.3).abs() < 1e-4);
    assert_eq!(srgb_0[1], 0.0);
    assert_eq!(srgb_0[2], 0.0);
    assert_eq!(get_redstone_wire_hex(0), "#4B0000");

    // Power 15: #FF2600 / rgb(1.0, 0.15, 0.0)
    let srgb_15 = get_redstone_wire_srgb(15);
    assert!((srgb_15[0] - 1.0).abs() < 1e-4);
    assert!((srgb_15[1] - 0.15).abs() < 1e-4);
    assert_eq!(srgb_15[2], 0.0);
    assert_eq!(get_redstone_wire_hex(15), "#FF2600");

    // Linear conversions
    let lin_0 = get_redstone_wire_color(0);
    let lin_15 = get_redstone_wire_color(15);
    assert!(lin_15[0] > lin_0[0]);
    assert_eq!(lin_0[3], 1.0);
    assert_eq!(lin_15[3], 1.0);
}

#[test]
fn test_biome_resolver_redstone_power_tint_info() {
    let resolver = BiomeResolver::new();

    // 1. Off state (power 0)
    let info_off = resolver.get_tint_info(
        "redstone_dust_line0",
        Some("minecraft:redstone_wire[power=0]"),
        Some(0),
    );
    assert!(info_off.is_hardcoded);
    assert_eq!(info_off.hardcoded_hex.as_deref(), Some("#4B0000"));

    // 2. On state (power 15)
    let info_on = resolver.get_tint_info(
        "redstone_dust_line0",
        Some("minecraft:redstone_wire[power=15]"),
        Some(0),
    );
    assert!(info_on.is_hardcoded);
    assert_eq!(info_on.hardcoded_hex.as_deref(), Some("#FF2600"));

    // 3. Mid state (power 7)
    let info_mid = resolver.get_tint_info(
        "redstone_dust_dot",
        Some("minecraft:redstone_wire[power=7]"),
        Some(0),
    );
    assert!(info_mid.is_hardcoded);
    assert_eq!(info_mid.hardcoded_hex.as_deref(), Some("#A01100"));

    // 4. Redstone overlay companion must NEVER be tinted
    let info_overlay = resolver.get_tint_info(
        "redstone_dust_overlay",
        Some("minecraft:redstone_wire[power=15]"),
        None,
    );
    assert_eq!(info_overlay.tint_category, "none");
    assert!(!info_overlay.is_hardcoded);
}

#[test]
fn test_material_resolver_redstone_aliases() {
    let mut address_map = AtlasAddressMap::new();
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/redstone_dust_line0"),
        make_sprite_loc(0, 10, [0.0, 0.0, 0.25, 0.25]),
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/redstone_dust_line1"),
        make_sprite_loc(0, 11, [0.25, 0.0, 0.5, 0.25]),
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/redstone_dust_dot"),
        make_sprite_loc(0, 12, [0.5, 0.0, 0.75, 0.25]),
    );
    address_map.sprites.insert(
        ResourceLocation::new("minecraft", "block/redstone_dust_overlay"),
        make_sprite_loc(0, 13, [0.75, 0.0, 1.0, 0.25]),
    );

    // Test resolution via external custom aliases (provided by Python layer)
    let mut custom_aliases = std::collections::HashMap::new();
    custom_aliases.insert(
        "redstone_dust_line0_on".to_string(),
        vec!["block/redstone_dust_line0".to_string()],
    );
    custom_aliases.insert(
        "redstone_dust_line1_off".to_string(),
        vec!["block/redstone_dust_line1".to_string()],
    );
    custom_aliases.insert(
        "redstone_dust_angled_on".to_string(),
        vec!["block/redstone_dust_line0".to_string()],
    );
    custom_aliases.insert(
        "redstone_dust_three_way_off".to_string(),
        vec!["block/redstone_dust_line1".to_string()],
    );
    custom_aliases.insert(
        "redstone_dust_four_way_on".to_string(),
        vec!["block/redstone_dust_dot".to_string()],
    );
    custom_aliases.insert(
        "redstone_dust_cross_off".to_string(),
        vec!["block/redstone_dust_dot".to_string()],
    );

    let res_line0_on = MaterialResolver::resolve(
        "redstone_dust_line0_on",
        Some(&custom_aliases),
        &address_map,
    );
    assert!(res_line0_on.is_some());
    assert_eq!(res_line0_on.unwrap().0.path, "block/redstone_dust_line0");

    let res_line1_off = MaterialResolver::resolve(
        "redstone_dust_line1_off",
        Some(&custom_aliases),
        &address_map,
    );
    assert!(res_line1_off.is_some());
    assert_eq!(res_line1_off.unwrap().0.path, "block/redstone_dust_line1");

    // Test angled, three_way, four_way, and cross aliases
    let res_angled = MaterialResolver::resolve(
        "redstone_dust_angled_on",
        Some(&custom_aliases),
        &address_map,
    );
    assert_eq!(res_angled.unwrap().0.path, "block/redstone_dust_line0");

    let res_three_way = MaterialResolver::resolve(
        "redstone_dust_three_way_off",
        Some(&custom_aliases),
        &address_map,
    );
    assert_eq!(res_three_way.unwrap().0.path, "block/redstone_dust_line1");

    let res_four_way = MaterialResolver::resolve(
        "redstone_dust_four_way_on",
        Some(&custom_aliases),
        &address_map,
    );
    assert_eq!(res_four_way.unwrap().0.path, "block/redstone_dust_dot");

    let res_cross = MaterialResolver::resolve(
        "redstone_dust_cross_off",
        Some(&custom_aliases),
        &address_map,
    );
    assert_eq!(res_cross.unwrap().0.path, "block/redstone_dust_dot");
}
