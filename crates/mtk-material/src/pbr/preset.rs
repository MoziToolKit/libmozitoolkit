//! # Built-in Material Physical Presets (Transmission / IOR / Roughness Fallback)
//!
//! Provides authoritative physical parameters for materials that lack LabPBR transmission
//! support (water, glass, stained glass, ice, honey, slime, etc.).

use super::labpbr::{encode_ior, encode_unit};

/// Physical material parameters for transparent / refractive / translucent blocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalMaterialPreset {
    /// Transmission weight in [0.0, 1.0], packed to `aux.A`.
    pub transmission: f32,
    /// Physical index of refraction (e.g. 1.333 for water, 1.5 for glass), packed to `aux.R`.
    pub ior: f32,
    /// Perceptual roughness in [0.0, 1.0], packed to `orm.G` when no custom specular exists.
    pub roughness: f32,
}

impl PhysicalMaterialPreset {
    #[inline]
    pub fn encoded_transmission(&self) -> u8 {
        encode_unit(self.transmission)
    }

    #[inline]
    pub fn encoded_ior(&self) -> u8 {
        encode_ior(self.ior)
    }

    #[inline]
    pub fn encoded_roughness(&self) -> u8 {
        encode_unit(self.roughness)
    }
}

/// Match a texture path (e.g. `"block/water_still"`, `"block/glass"`, `"optifine/ctm/glass/0"`)
/// to a built-in physical preset. Supports standard Minecraft paths as well as OptiFine / Continuity
/// CTM sub-tile textures.
pub fn get_physical_preset(path: &str) -> Option<PhysicalMaterialPreset> {
    let mut clean = path.strip_prefix("minecraft:").unwrap_or(path);
    clean = clean.strip_prefix("textures/").unwrap_or(clean);
    clean = clean.strip_suffix(".png").unwrap_or(clean);

    // 1. Water: refractive fluid, IOR 1.333, ultra-smooth, 100% transmission
    if clean.contains("water_still")
        || clean.contains("water_flow")
        || (clean.contains("water")
            && !clean.contains("water_bottle")
            && !clean.contains("underwater")
            && !clean.contains("bucket"))
    {
        return Some(PhysicalMaterialPreset {
            transmission: 1.0,
            ior: 1.333,
            roughness: 0.02,
        });
    }

    // 2. Glass & stained glass & tinted glass (including CTM tiles): IOR 1.5, smooth, 100% transmission
    if clean.contains("glass") && !clean.contains("spyglass") && !clean.contains("glass_bottle") {
        return Some(PhysicalMaterialPreset {
            transmission: 1.0,
            ior: 1.5,
            roughness: 0.05,
        });
    }

    // 3. Ice & frosted ice (including CTM tiles): IOR 1.309, 92% transmission, slight roughness
    if !clean.contains("packed_ice") && !clean.contains("blue_ice") && !clean.contains("stage") {
        let is_ice = clean.split('/').any(|seg| {
            seg == "ice"
                || seg.starts_with("ice_")
                || seg.ends_with("_ice")
                || seg.starts_with("frosted_ice")
        });
        if is_ice {
            return Some(PhysicalMaterialPreset {
                transmission: 0.92,
                ior: 1.309,
                roughness: 0.1,
            });
        }
    }

    // 4. Honey block: viscous translucent, IOR 1.53, 80% transmission
    if clean.contains("honey_block")
        || (clean.contains("honey") && !clean.contains("bottle") && !clean.contains("comb"))
    {
        return Some(PhysicalMaterialPreset {
            transmission: 0.8,
            ior: 1.53,
            roughness: 0.15,
        });
    }

    // 5. Slime block: translucent elastic, IOR 1.45, 75% transmission
    if clean.contains("slime_block") || (clean.contains("slime") && !clean.contains("ball")) {
        return Some(PhysicalMaterialPreset {
            transmission: 0.75,
            ior: 1.45,
            roughness: 0.2,
        });
    }

    None
}

/// Match a CTM rule definition to a built-in physical preset by inspecting its matchBlocks, matchTiles,
/// rule name, and source properties path.
pub fn rule_matches_preset(rule: &mtk_resource::CtmRule) -> Option<PhysicalMaterialPreset> {
    // 1. Check match_blocks (e.g. "glass", "white_stained_glass", "ice")
    for mb in &rule.match_blocks {
        let block_name = format!("block/{}", mb.block);
        if let Some(preset) = get_physical_preset(&block_name) {
            return Some(preset);
        }
        if let Some(preset) = get_physical_preset(&mb.block) {
            return Some(preset);
        }
    }

    // 2. Check match_tiles (e.g. "minecraft:block/glass")
    for mt in &rule.match_tiles {
        if let Some(preset) = get_physical_preset(&mt.path) {
            return Some(preset);
        }
    }

    // 3. Check rule name and source path (e.g. "optifine/ctm/glass/glass.properties")
    if let Some(preset) = get_physical_preset(&rule.name) {
        return Some(preset);
    }
    if let Some(preset) = get_physical_preset(&rule.source_path) {
        return Some(preset);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_water_preset() {
        let preset = get_physical_preset("minecraft:block/water_still").expect("water preset");
        assert_eq!(preset.transmission, 1.0);
        assert_eq!(preset.encoded_transmission(), 255);
        assert!((preset.ior - 1.333).abs() < 1e-4);
        assert_eq!(preset.encoded_ior(), 12);
        assert_eq!(preset.encoded_roughness(), 5);
    }

    #[test]
    fn test_glass_presets() {
        let glass = get_physical_preset("block/glass").expect("glass");
        assert_eq!(glass.transmission, 1.0);
        assert_eq!(glass.encoded_ior(), 18);

        let red_glass = get_physical_preset("block/red_stained_glass").expect("red glass");
        assert_eq!(red_glass.transmission, 1.0);
        assert_eq!(red_glass.ior, 1.5);

        let tinted =
            get_physical_preset("minecraft:textures/block/tinted_glass.png").expect("tinted");
        assert_eq!(tinted.transmission, 1.0);
    }

    #[test]
    fn test_opaque_block_returns_none() {
        assert!(get_physical_preset("block/stone").is_none());
        assert!(get_physical_preset("block/dirt").is_none());
        assert!(get_physical_preset("block/diamond_block").is_none());
    }
}
