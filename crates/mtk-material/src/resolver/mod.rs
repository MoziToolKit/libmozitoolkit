use mtk_resource::ResourceLocation;
use mtk_texture::{AtlasAddressMap, AtlasSpriteLocation};
use std::collections::HashMap;

pub mod overrides;

use crate::types::GridAtlasSpec;

use self::overrides::{builtin_biome_suffixes, builtin_material_fallbacks, ResolverOverrides};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Active external material rules, installed when a compatibility profile/cache is opened.
static MATERIAL_OVERRIDES: RwLock<Option<Arc<ResolverOverrides>>> = RwLock::new(None);
static HAS_OVERRIDES: AtomicBool = AtomicBool::new(false);

/// Installs external material compatibility rules.
pub fn set_material_overrides(overrides: ResolverOverrides) {
    if let Ok(mut guard) = MATERIAL_OVERRIDES.write() {
        *guard = Some(Arc::new(overrides));
        HAS_OVERRIDES.store(true, Ordering::Release);
    }
}

/// Clears external material compatibility rules, restoring built-in defaults.
pub fn clear_material_overrides() {
    if let Ok(mut guard) = MATERIAL_OVERRIDES.write() {
        *guard = None;
        HAS_OVERRIDES.store(false, Ordering::Release);
    }
}

fn active_material_overrides() -> Option<Arc<ResolverOverrides>> {
    if !HAS_OVERRIDES.load(Ordering::Acquire) {
        return None;
    }
    MATERIAL_OVERRIDES
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().map(Arc::clone))
}

/// Generic helper to clean a raw material or texture name into a normalized identifier stem.
pub fn clean_identifier(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();

    // 1. Strip file extensions
    if let Some(stripped) = s.strip_suffix(".png").or_else(|| s.strip_suffix(".jpg")) {
        s = stripped.to_string();
    }

    // 2. Strip blender duplicate suffixes (".001", "_001")
    if let Some(idx) = s.rfind('.') {
        if s[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
            s = s[..idx].to_string();
        }
    }

    // 3. Strip standard namespace / path / exporter prefixes
    let prefixes = [
        "tex/minecraft/",
        "textures/block/",
        "textures/entity/",
        "textures/item/",
        "textures/banner/",
        "textures/",
        "minecraft_block-",
        "minecraft_entity-",
        "minecraft_item-",
        "minecraft_banner-",
        "minecraft_block_",
        "minecraft_entity_",
        "minecraft_item_",
        "jmc2obj_block-",
        "jmc2obj_block_",
        "jmc2obj_entity-",
        "jmc2obj_item-",
        "jmc2obj_",
        "minecraft:",
        "minecraft-",
        "mw_",
        "mwo_",
        "tile_",
        "tile-",
        "tile.",
        "block_",
        "block-",
        "block.",
        "block/",
        "entity/",
        "item/",
    ];

    for prefix in prefixes {
        if s.starts_with(prefix) {
            s = s[prefix.len()..].to_string();
            break;
        }
    }

    // 4. Strip exporter biome suffixes (external config overrides the built-ins)
    if s.contains('-') || s.contains('_') {
        let active = active_material_overrides();
        let biome_suffixes: &[String] = match active.as_ref() {
            Some(overrides) => overrides.effective_biome_suffixes(),
            None => builtin_biome_suffixes(),
        };
        for suffix in biome_suffixes {
            if s.ends_with(suffix) {
                s = s[..s.len() - suffix.len()].to_string();
                break;
            }
        }
    }

    if !s.contains('/') {
        s = s.replace([' ', '-'], "_");
    }

    s
}

/// Convert a grid atlas UV coordinate (u, v) to its local [0, 1] swatch coordinate based on GridAtlasSpec.
#[inline]
pub fn remap_grid_atlas_uv_to_local(u: f32, v: f32, spec: &GridAtlasSpec) -> [f32; 2] {
    let tex_w = spec.image_width.max(1) as f32;
    let tex_h = spec.image_height.max(1) as f32;

    let px = u * tex_w;
    let py = (1.0 - v) * tex_h;

    let atlas_col = (px / spec.swatch_size).floor();
    let atlas_row = (py / spec.swatch_size).floor();

    let lu = (px - (atlas_col * spec.swatch_size + spec.border)) / spec.tile_size;
    let lv = 1.0 - ((py - (atlas_row * spec.swatch_size + spec.border)) / spec.tile_size);

    [lu, lv]
}

/// Decode a polygon UV coordinate on a grid atlas to its candidate identifiers and local [0, 1] UVs.
pub fn decode_grid_atlas_uv(
    u: f32,
    v: f32,
    spec: &GridAtlasSpec,
) -> (Option<&Vec<String>>, [f32; 2]) {
    let tex_w = spec.image_width.max(1) as f32;
    let tex_h = spec.image_height.max(1) as f32;
    let swatches_per_row = (spec.image_width as f32 / spec.swatch_size)
        .floor()
        .max(1.0) as usize;

    let px = u * tex_w;
    let py = (1.0 - v) * tex_h;

    let atlas_col = (px / spec.swatch_size).floor() as usize;
    let atlas_row = (py / spec.swatch_size).floor() as usize;
    let swatch_id = atlas_col + atlas_row * swatches_per_row;

    let candidates = spec.swatch_to_candidates.get(&swatch_id);
    let local_uv = remap_grid_atlas_uv_to_local(u, v, spec);
    (candidates, local_uv)
}

#[inline]
fn get_fluid_candidates(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "water" | "minecraft:water" | "stationary_water" | "water_still" => {
            Some(&["block/water_still", "block/water_flow"])
        }
        "flowing_water" | "minecraft:flowing_water" | "water_flow" => {
            Some(&["block/water_flow", "block/water_still"])
        }
        "lava" | "minecraft:lava" | "stationary_lava" | "lava_still" => {
            Some(&["block/lava_still", "block/lava_flow"])
        }
        "flowing_lava" | "minecraft:flowing_lava" | "lava_flow" => {
            Some(&["block/lava_flow", "block/lava_still"])
        }
        _ => None,
    }
}

/// Resolve a raw material name to a dedicated animated sprite, if one exists.
///
/// This is the shared animation-routing policy used by both the voxel mesher and
/// the external-mesh material remapper. It checks the exact string first, then the
/// parsed resource location, the cleaned identifier, and finally user aliases.
pub fn resolve_animated_sprite<'a>(
    raw_material_name: &str,
    custom_aliases: Option<&HashMap<String, Vec<String>>>,
    address_map: &'a AtlasAddressMap,
) -> Option<(ResourceLocation, &'a AtlasSpriteLocation)> {
    if address_map.anim_sprites.is_empty() {
        return None;
    }
    let raw = raw_material_name.trim();
    if raw.is_empty() {
        return None;
    }

    // 0. Direct O(1) lookup on the raw string first (avoids cleaning for standard texture paths).
    if let Some(sprite) = address_map.lookup_animated_str(raw) {
        if let Ok(loc) = ResourceLocation::parse_texture_path(raw) {
            return Some((loc, sprite));
        }
        if let Ok(loc) = ResourceLocation::parse(raw) {
            return Some((loc, sprite));
        }
    }

    // 1. Parsed resource location.
    if let Ok(loc) = ResourceLocation::parse(raw) {
        if let Some(sprite) = address_map.lookup_animated(&loc) {
            return Some((loc, sprite));
        }
    }

    // 2. Built-in fluid alias prioritization
    let cleaned = clean_identifier(raw);
    if let Some(candidates) = get_fluid_candidates(raw).or_else(|| get_fluid_candidates(&cleaned)) {
        for &cand in candidates {
            if let Some(sprite) = address_map.lookup_animated_str(cand) {
                let loc = ResourceLocation::parse(cand)
                    .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                return Some((loc, sprite));
            }
        }
    }

    // 3. Cleaned identifier.
    if cleaned != raw {
        if let Some(sprite) = address_map.lookup_animated_str(&cleaned) {
            if let Ok(loc) = ResourceLocation::parse(&cleaned) {
                return Some((loc, sprite));
            }
        }
    }

    // 4. Custom alias table.
    if let Some(aliases) = custom_aliases {
        if let Some(candidates) = aliases.get(raw).or_else(|| aliases.get(&cleaned)) {
            for cand in candidates {
                if let Some(sprite) = address_map.lookup_animated_str(cand) {
                    let loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand.as_str()));
                    return Some((loc, sprite));
                }
            }
        }
    }

    None
}

/// Resolve a face material using the caller's animation policy.
///
/// When `enable_animated_textures` is true, a dedicated animated sprite wins over
/// the static Frame-0 baseline. When false, the static sprite is always preferred.
pub fn resolve_routed_sprite<'a>(
    raw_material_name: &str,
    custom_aliases: Option<&HashMap<String, Vec<String>>>,
    address_map: &'a AtlasAddressMap,
    enable_animated_textures: bool,
) -> Option<(ResourceLocation, &'a AtlasSpriteLocation)> {
    if enable_animated_textures {
        if let Some(animated) =
            resolve_animated_sprite(raw_material_name, custom_aliases, address_map)
        {
            return Some(animated);
        }
    }
    MaterialResolver::resolve(raw_material_name, custom_aliases, address_map)
}

/// Extract `[total_frames, frametime, interpolate]` and `[frame_width, frame_height, 0]`
/// from an atlas sprite. Static sprites return safe defaults.
pub fn extract_animation_metrics(sprite: Option<&AtlasSpriteLocation>) -> ([f32; 3], [f32; 3]) {
    if let Some(sprite) = sprite {
        if sprite.is_animated {
            let total_frames = sprite.frame_count.max(1) as f32;
            let frametime = sprite
                .animation
                .as_ref()
                .map(|a| a.frametime as f32)
                .unwrap_or(1.0)
                .max(1.0);
            let interpolate = if sprite
                .animation
                .as_ref()
                .map(|a| a.interpolate)
                .unwrap_or(false)
            {
                1.0
            } else {
                0.0
            };
            return (
                [total_frames, frametime, interpolate],
                [
                    sprite.frame_size[0] as f32,
                    sprite.frame_size[1] as f32,
                    0.0,
                ],
            );
        }
    }
    ([1.0, 1.0, 0.0], [16.0, 16.0, 0.0])
}

/// Unified data-driven resolver for mapping raw DCC material names to canonical AtlasSpriteLocations.
pub struct MaterialResolver;

impl MaterialResolver {
    /// Resolve a raw material name to a target Atlas Sprite Location using optional external alias table.
    pub fn resolve<'a>(
        raw_material_name: &str,
        custom_aliases: Option<&HashMap<String, Vec<String>>>,
        address_map: &'a AtlasAddressMap,
    ) -> Option<(ResourceLocation, &'a AtlasSpriteLocation)> {
        let cleaned = clean_identifier(raw_material_name);

        // 0. Built-in fluid alias prioritization (prevents "water" / "lava" from colliding with paintings or particles)
        if let Some(candidates) =
            get_fluid_candidates(raw_material_name).or_else(|| get_fluid_candidates(&cleaned))
        {
            for &cand in candidates {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        // 1. Direct O(1) lookup attempt on raw name
        if let Some(loc) = address_map.lookup_str(raw_material_name) {
            if let Ok(res_loc) = ResourceLocation::parse(raw_material_name) {
                return Some((res_loc, loc));
            }
        }

        // 2. Direct O(1) lookup attempt on cleaned name
        if let Some(loc) = address_map.lookup_str(&cleaned) {
            if let Ok(res_loc) = ResourceLocation::parse(&cleaned) {
                return Some((res_loc, loc));
            }
        }

        // 3. Search external / user-provided alias table
        if let Some(alias_map) = custom_aliases {
            // Check raw name
            if let Some(candidates) = alias_map.get(raw_material_name) {
                for cand in candidates {
                    if let Some(sprite_loc) = address_map.lookup_str(cand) {
                        let res_loc = ResourceLocation::parse(cand)
                            .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                        return Some((res_loc, sprite_loc));
                    }
                }
            }

            // Check cleaned name
            if cleaned != raw_material_name {
                if let Some(candidates) = alias_map.get(&cleaned) {
                    for cand in candidates {
                        if let Some(sprite_loc) = address_map.lookup_str(cand) {
                            let res_loc = ResourceLocation::parse(cand)
                                .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                            return Some((res_loc, sprite_loc));
                        }
                    }
                }
            }
        }

        // 3.5. Built-in legacy redstone material alias fallbacks (Mineways / Jmc2Obj)
        if cleaned.starts_with("redstone_dust") || cleaned.starts_with("redstone_wire") {
            let base = cleaned
                .strip_suffix("_on")
                .or_else(|| cleaned.strip_suffix("_off"))
                .unwrap_or(&cleaned);
            let candidates: &[&str] = match base {
                "redstone_dust_line0" | "redstone_wire_line0" => &[
                    "block/redstone_dust_line0",
                    "block/redstone_dust_overlay",
                    "block/redstone_dust_dot",
                ],
                "redstone_dust_line1" | "redstone_wire_line1" => &[
                    "block/redstone_dust_line1",
                    "block/redstone_dust_overlay",
                    "block/redstone_dust_dot",
                ],
                "redstone_dust_dot" | "redstone_wire_dot" => {
                    &["block/redstone_dust_dot", "block/redstone_dust_overlay"]
                }
                "redstone_dust_angled" | "redstone_wire_angled" => &[
                    "block/redstone_dust_line0",
                    "block/redstone_dust_line1",
                    "block/redstone_dust_overlay",
                ],
                "redstone_dust_three_way" | "redstone_wire_three_way" => &[
                    "block/redstone_dust_line1",
                    "block/redstone_dust_overlay",
                    "block/redstone_dust_dot",
                ],
                "redstone_dust_four_way" | "redstone_dust_cross" | "redstone_wire_four_way" => &[
                    "block/redstone_dust_dot",
                    "block/redstone_dust_line1",
                    "block/redstone_dust_overlay",
                ],
                "redstone_dust_overlay" | "redstone_wire_overlay" => {
                    &["block/redstone_dust_overlay"]
                }
                _ => &[],
            };

            for &cand in candidates {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        // 3.6 Special built-in canonical aliases (End Portal, Gateway, Conduit, Banner, Hanging Signs)
        if raw_material_name.contains("end_portal")
            || raw_material_name.contains("end_gateway")
            || cleaned == "end_portal"
            || cleaned == "end_gateway"
        {
            for &cand in &[
                "entity/end_portal/end_portal",
                "entity/end_portal",
                "block/end_portal",
            ] {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        if cleaned == "conduit" || raw_material_name.contains("conduit") {
            for &cand in &["entity/conduit/base", "block/conduit"] {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        if cleaned == "banner" || cleaned == "banner_base" || cleaned == "base" {
            for &cand in &["entity/banner/banner_base", "entity/banner/base"] {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        if cleaned == "decorated_pot"
            || cleaned == "decorated_pot_base"
            || cleaned == "decorated_pot_side"
            || raw_material_name.contains("decorated_pot")
        {
            for &cand in &[
                "entity/decorated_pot/decorated_pot_side",
                "entity/decorated_pot/decorated_pot_base",
            ] {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        if cleaned.contains("pottery_pattern") {
            let cand = format!("entity/decorated_pot/{}", cleaned);
            if let Some(sprite_loc) = address_map.lookup_str(&cand) {
                let res_loc = ResourceLocation::parse(&cand)
                    .unwrap_or_else(|_| ResourceLocation::new("minecraft", &cand));
                return Some((res_loc, sprite_loc));
            }
        }

        if raw_material_name.contains("sign") {
            let clean_raw = raw_material_name
                .strip_prefix("minecraft:")
                .unwrap_or(raw_material_name);
            let raw_stem = clean_raw.rsplit('/').next().unwrap_or("");
            let pure_wood = raw_stem
                .strip_suffix("_wall_hanging_sign")
                .or_else(|| raw_stem.strip_suffix("_hanging_sign"))
                .or_else(|| raw_stem.strip_suffix("_wall_hanging"))
                .or_else(|| raw_stem.strip_suffix("_hanging"))
                .or_else(|| raw_stem.strip_suffix("_wall_sign"))
                .or_else(|| raw_stem.strip_suffix("_sign"))
                .unwrap_or(raw_stem);

            let is_hanging = raw_material_name.contains("hanging");
            let candidates = if is_hanging {
                vec![
                    format!("entity/signs/hanging/{}", pure_wood),
                    format!("block/{}_hanging_sign", pure_wood),
                    format!("entity/signs/{}", pure_wood),
                    format!("block/{}_sign", pure_wood),
                    format!("block/{}_planks", pure_wood),
                ]
            } else {
                vec![
                    format!("entity/signs/{}", pure_wood),
                    format!("block/{}_sign", pure_wood),
                    format!("entity/signs/hanging/{}", pure_wood),
                    format!("block/{}_hanging_sign", pure_wood),
                    format!("block/{}_planks", pure_wood),
                ]
            };

            for cand in &candidates {
                if let Some(sprite_loc) = address_map.lookup_str(cand) {
                    let res_loc = ResourceLocation::parse(cand)
                        .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                    return Some((res_loc, sprite_loc));
                }
            }
        }

        // 3.7 Configurable legacy/modded material fallbacks.
        let active = active_material_overrides();
        let fallback_rules: &[overrides::MaterialFallbackRule] = match active.as_ref() {
            Some(overrides) => overrides.effective_material_fallbacks(),
            None => builtin_material_fallbacks(),
        };
        for rule in fallback_rules {
            let keyword = rule.keyword.to_lowercase();
            if raw_material_name.to_lowercase().contains(&keyword) || cleaned.contains(&keyword) {
                for cand in &rule.candidates {
                    if let Some(sprite_loc) = address_map.lookup_str(cand) {
                        let res_loc = ResourceLocation::parse(cand)
                            .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                        return Some((res_loc, sprite_loc));
                    }
                }
            }
        }

        // 4. Default standard Minecraft category fallbacks
        let fallbacks = [
            format!("block/{}", cleaned),
            format!("entity/{}", cleaned),
            format!("item/{}", cleaned),
            cleaned.clone(),
        ];

        for cand in &fallbacks {
            if let Some(sprite_loc) = address_map.lookup_str(cand) {
                let res_loc = ResourceLocation::parse(cand)
                    .unwrap_or_else(|_| ResourceLocation::new("minecraft", cand));
                return Some((res_loc, sprite_loc));
            }
        }

        None
    }

    /// Resolve a face on a grid-based atlas to canonical sprite location and local UVs.
    pub fn resolve_grid_atlas_face<'a>(
        u: f32,
        v: f32,
        spec: &GridAtlasSpec,
        custom_aliases: Option<&HashMap<String, Vec<String>>>,
        address_map: &'a AtlasAddressMap,
    ) -> Option<(ResourceLocation, &'a AtlasSpriteLocation, [f32; 2])> {
        let (candidates_opt, local_uv) = decode_grid_atlas_uv(u, v, spec);

        if let Some(candidates) = candidates_opt {
            for cand in candidates {
                if let Some((res_loc, sprite_loc)) =
                    Self::resolve(cand, custom_aliases, address_map)
                {
                    return Some((res_loc, sprite_loc, local_uv));
                }
            }
        }

        None
    }
}
