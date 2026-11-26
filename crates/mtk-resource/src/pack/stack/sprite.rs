use super::ResourcePackStack;
use crate::error::ResourceError;
use crate::identifier::ResourceLocation;
use crate::meta::{AnimationMetadata, TextureMetadata};

/// Sub-rectangle crop descriptor from an `unstitch` atlas source.
#[derive(Debug, Clone, PartialEq)]
pub struct UnstitchRegionSpec {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub divisor_x: f64,
    pub divisor_y: f64,
}

/// Palette permutation parameters for a `paletted_permutations` atlas source.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PalettedPermutationSpec {
    pub raw_palette_key: Vec<u8>,
    pub raw_permutation: Vec<u8>,
}

/// Discovered single sprite entry ready for decoding and stitching.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredSprite {
    /// Canonical sprite identifier, e.g. `"minecraft:block/stone"`.
    pub sprite_id: ResourceLocation,
    /// Texture file location in resource pack, e.g. `"minecraft:block/stone"`.
    pub texture_location: ResourceLocation,
    /// Direct raw PNG bytes if already loaded (e.g. from unstitch or memory).
    pub raw_albedo: Option<Vec<u8>>,
    pub raw_normal: Option<Vec<u8>>,
    pub raw_specular: Option<Vec<u8>>,
    pub raw_overlay: Option<Vec<u8>>,
    pub raw_emissive: Option<Vec<u8>>,
    pub metadata: Option<AnimationMetadata>,
    pub emissive_metadata: Option<AnimationMetadata>,
    /// Optional sub-rectangle crop (populated by `AtlasSource::Unstitch`).
    pub unstitch: Option<UnstitchRegionSpec>,
    /// Optional palette permutation specification (populated by `AtlasSource::PalettedPermutations`).
    pub palette: Option<PalettedPermutationSpec>,
}

/// Composite PBR companion data extracted for a texture resource.
#[derive(Debug, Clone, Default)]
pub struct PbrCompanions {
    pub albedo: Option<Vec<u8>>,
    pub normal: Option<Vec<u8>>,
    pub specular: Option<Vec<u8>>,
    pub overlay: Option<Vec<u8>>,
    pub emissive: Option<Vec<u8>>,
    pub mcmeta: Option<AnimationMetadata>,
    pub emissive_mcmeta: Option<AnimationMetadata>,
}

/// Returns true if the given texture asset path is a LabPBR / grass-side companion texture
/// (`_n`, `_s`, `grass_*_side_overlay`) that must not become an independent standalone sprite
/// in directory scans.
///
/// General `*_overlay.png` and `*_emissive.png` textures (e.g. `redstone_dust_overlay`,
/// `water_overlay`, `potion_overlay`, `leather_*_overlay`, `firefly_bush_emissive`) are real
/// standalone Minecraft sprites and MUST NOT be excluded here.
pub(crate) fn is_companion_asset_path(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with("_n.png")
        || lower.ends_with("_s.png")
        || lower.ends_with("_overlay_anim.png")
        || lower.ends_with("_emissive_anim.png")
        || lower.ends_with("grass_block_side_overlay.png")
        || lower.ends_with("grass_side_overlay.png")
}

impl ResourcePackStack {
    /// Resolve all PBR companions (`_n`, `_s`, `.mcmeta`) using granular per-channel fallback.
    ///
    /// `.mcmeta` animation metadata is loaded strictly from the same pack that provided the
    /// texture PNG (matching Minecraft's `FallbackResourceManager.getResource`), so a static
    /// override texture in a higher pack never inherits a lower pack's `.mcmeta`.
    ///
    /// Fails when a pack holds a companion texture that cannot be read, so a broken pack does not
    /// silently downgrade to vanilla companions.
    pub fn resolve_pbr_companions(
        &self,
        location: &ResourceLocation,
    ) -> Result<PbrCompanions, ResourceError> {
        let mut companions = PbrCompanions::default();

        // Try standard textures/ prefix as well as raw path (for optifine/ctm/ assets)
        let candidates = [
            location.to_asset_path("textures", "png"),
            format!("assets/{}/{}.png", location.namespace, location.path),
        ];

        // 1. Albedo + same-pack MCMETA animation
        let mut albedo_pack_idx: Option<usize> = None;
        let mut albedo_mcmeta_path: Option<String> = None;
        for path in &candidates {
            if let Some((pack_idx, bytes)) = self.open_asset_with_pack_index(path)? {
                companions.albedo = Some(bytes);
                albedo_pack_idx = Some(pack_idx);
                albedo_mcmeta_path = Some(format!("{path}.mcmeta"));
                break;
            }
        }

        // 2. Normal (try _n.png then _N.png)
        let normal_candidates = [
            location.with_suffix("_n").to_asset_path("textures", "png"),
            location.with_suffix("_N").to_asset_path("textures", "png"),
            format!("assets/{}/{}_n.png", location.namespace, location.path),
            format!("assets/{}/{}_N.png", location.namespace, location.path),
        ];
        for path in &normal_candidates {
            if let Some(bytes) = self.open_asset_raw(path)? {
                companions.normal = Some(bytes);
                break;
            }
        }

        // 3. Specular (try _s.png then _S.png)
        let spec_candidates = [
            location.with_suffix("_s").to_asset_path("textures", "png"),
            location.with_suffix("_S").to_asset_path("textures", "png"),
            format!("assets/{}/{}_s.png", location.namespace, location.path),
            format!("assets/{}/{}_S.png", location.namespace, location.path),
        ];
        for path in &spec_candidates {
            if let Some(bytes) = self.open_asset_raw(path)? {
                companions.specular = Some(bytes);
                break;
            }
        }

        // 4. Overlay: only for block side overlays (grass_block_side / grass_side / block/*_side)
        // so standalone item/entity/block overlays (potion_overlay, leather_*_overlay,
        // redstone_dust_overlay, wolf_armor_overlay) are never hijacked as base aliases.
        if location.path.starts_with("block/") && location.path.ends_with("side") {
            let overlay_candidates = [
                location
                    .with_suffix("_overlay")
                    .to_asset_path("textures", "png"),
                format!(
                    "assets/{}/textures/{}.png",
                    location.namespace,
                    location
                        .path
                        .replace("grass_block_side", "grass_block_side_overlay")
                ),
                format!(
                    "assets/{}/textures/{}.png",
                    location.namespace,
                    location.path.replace("grass_side", "grass_side_overlay")
                ),
            ];
            for path in &overlay_candidates {
                if path != &candidates[0] && path != &candidates[1] {
                    if let Some(bytes) = self.open_asset_raw(path)? {
                        companions.overlay = Some(bytes);
                        break;
                    }
                }
            }
        }

        // 5. Emissive (e.g. firefly_bush_emissive, open_eyeblossom_emissive, _e.png)
        let emissive_candidates = [
            location
                .with_suffix("_emissive")
                .to_asset_path("textures", "png"),
            format!(
                "assets/{}/{}_emissive.png",
                location.namespace, location.path
            ),
            location.with_suffix("_e").to_asset_path("textures", "png"),
            location.with_suffix("_E").to_asset_path("textures", "png"),
            format!("assets/{}/{}_e.png", location.namespace, location.path),
            format!("assets/{}/{}_E.png", location.namespace, location.path),
        ];
        let mut emissive_pack_idx: Option<usize> = None;
        let mut emissive_mcmeta_path: Option<String> = None;
        for path in &emissive_candidates {
            if path != &candidates[0] && path != &candidates[1] {
                if let Some((pack_idx, bytes)) = self.open_asset_with_pack_index(path)? {
                    companions.emissive = Some(bytes);
                    emissive_pack_idx = Some(pack_idx);
                    emissive_mcmeta_path = Some(format!("{path}.mcmeta"));
                    break;
                }
            }
        }

        // Emissive MCMETA animation (same pack as emissive PNG)
        if let (Some(pack_idx), Some(meta_path)) = (emissive_pack_idx, emissive_mcmeta_path) {
            if let Some(meta_bytes) = self.open_pack_asset_raw(pack_idx, &meta_path)? {
                if let Ok(meta_str) = std::str::from_utf8(&meta_bytes) {
                    if let Ok(tex_meta) = TextureMetadata::parse_json(meta_str) {
                        companions.emissive_mcmeta = tex_meta.animation;
                    }
                }
            }
        }

        // 6. Albedo MCMETA animation (same pack as albedo PNG)
        if let (Some(pack_idx), Some(meta_path)) = (albedo_pack_idx, albedo_mcmeta_path) {
            if let Some(meta_bytes) = self.open_pack_asset_raw(pack_idx, &meta_path)? {
                if let Ok(meta_str) = std::str::from_utf8(&meta_bytes) {
                    if let Ok(tex_meta) = TextureMetadata::parse_json(meta_str) {
                        companions.mcmeta = tex_meta.animation;
                    }
                }
            }
        }

        Ok(companions)
    }
}
