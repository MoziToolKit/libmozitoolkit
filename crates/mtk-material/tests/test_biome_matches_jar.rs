//! `CANONICAL_BIOMES` must agree with the biome files the game actually ships.
//!
//! Ground truth: `data/minecraft/worldgen/biome/*.json` (one file per biome, from the unpacked
//! jar). Authoritative fields are copied verbatim (`temperature`, `downfall`, `effects.*_color`);
//! the rest are *derived* and must agree with the same rules Minecraft applies:
//!
//! ```java
//! // ColorMapColorUtil.get (grass/foliage/dry_foliage defaults)
//! int x = (int)((1.0 - temp) * 255.0);
//! int y = (int)((1.0 - rain) * 255.0);   // rain = downfall * temp
//! // BiomeSpecialEffects.GrassColorModifier
//! DARK_FOREST: (baseColor & 0x00FEFEFE) + 0x0028340A >> 1
//! SWAMP:       noise(x * 0.0225, z * 0.0225) < -0.1 ? 0xFF4C763C : 0xFF6A7039
//! ```
//!
//! The table used to be maintained by hand, so the derived colours drifted by a few units and
//! two ids were wrong (`sparce_jungle` does not exist, `sulfur_caves` was missing).

use mtk_material::{BiomePalette, VanillaColormaps, CANONICAL_BIOMES};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

fn jar_biomes() -> Option<BTreeMap<String, Value>> {
    let Some(root) = mtk_testkit::real_assets_root() else {
        mtk_testkit::skip("no unpacked jar (set MTK_TEST_ASSETS or place ../Minecraft_Source)");
        return None;
    };
    let dir = Path::new(&root).join("data/minecraft/worldgen/biome");
    if !dir.is_dir() {
        mtk_testkit::skip("no data/minecraft/worldgen/biome in the asset root");
        return None;
    }
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).expect("biome dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("stem")
            .to_string();
        let value: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("biome json");
        out.insert(id, value);
    }
    Some(out)
}

fn palette(id: &str) -> Option<&'static BiomePalette> {
    CANONICAL_BIOMES.iter().find(|palette| palette.id == id)
}

fn rgb(hex: &str) -> [u8; 3] {
    let clean = hex.trim().trim_start_matches('#');
    [
        u8::from_str_radix(&clean[0..2], 16).expect("r"),
        u8::from_str_radix(&clean[2..4], 16).expect("g"),
        u8::from_str_radix(&clean[4..6], 16).expect("b"),
    ]
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn sampled(colormaps: &VanillaColormaps, kind: &str, value: &Value) -> [u8; 3] {
    let temp = value["temperature"].as_f64().expect("temperature") as f32;
    let downfall = value["downfall"].as_f64().expect("downfall") as f32;
    let color = match kind {
        "grass" => colormaps.sample_grass_srgb(temp, downfall),
        "foliage" => colormaps.sample_foliage_srgb(temp, downfall),
        _ => colormaps.sample_dry_foliage_srgb(temp, downfall),
    };
    [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

/// Minecraft's `GrassColorModifier.DARK_FOREST`, on the packed ARGB value:
/// `((base & 0x00FEFEFE) + 0x0028340A) >> 1`. The per-channel offsets differ (0x28/0x34/0x0A).
fn dark_forest(base: [u8; 3]) -> [u8; 3] {
    let [r, g, b] = base.map(u32::from);
    let packed = 0xFF00_0000 | (r << 16) | (g << 8) | b;
    let packed = ((packed & 0x00FE_FEFE) + 0x0028_340A) >> 1;
    [
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

#[test]
fn the_biome_id_set_matches_the_jar() {
    let Some(jar) = jar_biomes() else { return };
    let jar_ids: Vec<&String> = jar.keys().collect();
    let table_ids: Vec<&str> = CANONICAL_BIOMES.iter().map(|b| b.id).collect();

    let missing: Vec<&&String> = jar_ids
        .iter()
        .filter(|id| !table_ids.contains(&id.as_str()))
        .collect();
    let extra: Vec<&&str> = table_ids
        .iter()
        .filter(|id| !jar.contains_key(**id))
        .collect();
    assert!(
        missing.is_empty(),
        "biomes shipped by the game but missing from CANONICAL_BIOMES: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "CANONICAL_BIOMES lists biomes the game does not ship: {extra:?}"
    );
    assert_eq!(
        CANONICAL_BIOMES.len(),
        jar.len(),
        "one palette per biome, no duplicates"
    );

    let mut ids = table_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        table_ids.len(),
        "duplicate ids in CANONICAL_BIOMES"
    );
}

#[test]
fn climate_and_water_match_the_jar() {
    let Some(jar) = jar_biomes() else { return };
    for (id, value) in &jar {
        let Some(pal) = palette(id) else { continue };
        let temp = value["temperature"].as_f64().expect("temperature") as f32;
        let downfall = value["downfall"].as_f64().expect("downfall") as f32;
        assert!(
            (pal.temperature - temp).abs() < 1e-6,
            "{id}: temperature {} != {temp}",
            pal.temperature
        );
        assert!(
            (pal.humidity - downfall).abs() < 1e-6,
            "{id}: downfall {} != {downfall}",
            pal.humidity
        );
        if let Some(water) = value["effects"]["water_color"].as_str() {
            assert_eq!(
                pal.water_hex.to_ascii_lowercase(),
                water.to_ascii_lowercase(),
                "{id}: water colour"
            );
        }
    }
}

/// `effects.grass_color` / `foliage_color` / `dry_foliage_color` are authoritative.
#[test]
fn explicit_effect_colours_match_the_jar() {
    let Some(jar) = jar_biomes() else { return };
    for (id, value) in &jar {
        let Some(pal) = palette(id) else { continue };
        for (key, flag, table_hex) in [
            ("grass_color", pal.has_custom_grass, pal.grass_hex),
            ("foliage_color", pal.has_custom_foliage, pal.foliage_hex),
            (
                "dry_foliage_color",
                pal.has_custom_dry_foliage,
                pal.dry_foliage_hex,
            ),
        ] {
            let explicit = value["effects"][key].as_str();
            // `grass_color_modifier` biomes bake their fixed colour into the table and are marked
            // custom today, so the flag there means "fixed colour" rather than "explicit in the
            // jar". Backlog C-10 replaces the flag with the modifier itself.
            let modifier =
                key == "grass_color" && !value["effects"]["grass_color_modifier"].is_null();
            let flag = flag && !modifier;
            match (explicit, flag) {
                (Some(json_hex), true) => assert_eq!(
                    table_hex.to_ascii_lowercase(),
                    json_hex.to_ascii_lowercase(),
                    "{id}: {key} is explicit in the jar but differs in the table"
                ),
                (Some(json_hex), false) => panic!(
                    "{id}: the jar declares {key} = {json_hex} but the table marks it as derived"
                ),
                (None, true) => panic!(
                    "{id}: the table marks {key} as explicit ({table_hex}) but the jar declares none"
                ),
                (None, false) => {}
            }
        }
    }
}

/// Derived colours must equal the colormap sample, so the table cannot drift again.
#[test]
fn derived_colours_match_the_colormap() {
    let Some(jar) = jar_biomes() else { return };
    let colormaps = VanillaColormaps::load_embedded();
    for (id, value) in &jar {
        let Some(pal) = palette(id) else { continue };
        if !value["effects"]["grass_color_modifier"].is_null() {
            continue; // covered by the modifier test
        }
        if !pal.has_custom_grass {
            assert_eq!(
                rgb(pal.grass_hex),
                sampled(&colormaps, "grass", value),
                "{id}: grass"
            );
        }
        if !pal.has_custom_foliage {
            assert_eq!(
                rgb(pal.foliage_hex),
                sampled(&colormaps, "foliage", value),
                "{id}: foliage"
            );
        }
        if !pal.has_custom_dry_foliage {
            assert_eq!(
                rgb(pal.dry_foliage_hex),
                sampled(&colormaps, "dry_foliage", value),
                "{id}: dry foliage"
            );
        }
    }
}

/// The three biomes with a `grass_color_modifier` follow its rule, and no other biome declares one.
#[test]
fn grass_colour_modifiers_match_the_jar() {
    let Some(jar) = jar_biomes() else { return };
    let colormaps = VanillaColormaps::load_embedded();
    let with_modifier: Vec<&String> = jar
        .iter()
        .filter(|(_, value)| !value["effects"]["grass_color_modifier"].is_null())
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        with_modifier,
        vec!["dark_forest", "mangrove_swamp", "swamp"],
        "the jar declares a modifier on exactly these biomes"
    );

    let dark = palette("dark_forest").expect("dark_forest palette");
    assert_eq!(
        rgb(dark.grass_hex),
        dark_forest(sampled(&colormaps, "grass", &jar["dark_forest"])),
        "dark_forest must darken the colormap sample"
    );

    for swamp_id in ["swamp", "mangrove_swamp"] {
        let swamp = palette(swamp_id).expect("swamp palette");
        assert_eq!(
            hex(rgb(swamp.grass_hex)).to_ascii_lowercase(),
            "#6a7039",
            "{swamp_id}: the table stores the high-noise swamp colour"
        );
    }
}
