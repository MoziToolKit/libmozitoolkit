use super::ResourcePackStack;
use crate::identifier::ResourceLocation;
use crate::meta::{AnimationMetadata, TextureMetadata};

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

/// Returns true if the given texture asset path is a companion texture (_n, _s, _overlay)
/// that must not become an independent standalone sprite in the atlas.
pub(crate) fn is_companion_asset_path(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with("_n.png")
        || lower.ends_with("_s.png")
        || lower.ends_with("_overlay.png")
        || lower.ends_with("_overlay_anim.png")
        || lower.ends_with("_emissive.png")
        || lower.ends_with("_emissive_anim.png")
        || lower.ends_with("grass_block_side_overlay.png")
        || lower.ends_with("grass_side_overlay.png")
}

impl ResourcePackStack {
    /// Resolve all PBR companions (`_n`, `_s`, `.mcmeta`) using granular per-channel fallback.
    pub fn resolve_pbr_companions(&self, location: &ResourceLocation) -> PbrCompanions {
        let mut companions = PbrCompanions::default();

        // Try standard textures/ prefix as well as raw path (for optifine/ctm/ assets)
        let candidates = [
            location.to_asset_path("textures", "png"),
            format!("assets/{}/{}.png", location.namespace, location.path),
        ];

        // 1. Albedo
        for path in &candidates {
            if let Some(bytes) = self.open_asset_raw(path) {
                companions.albedo = Some(bytes);
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
            if let Some(bytes) = self.open_asset_raw(path) {
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
            if let Some(bytes) = self.open_asset_raw(path) {
                companions.specular = Some(bytes);
                break;
            }
        }

        // 4. Overlay (e.g. grass_block_side_overlay, _overlay.png)
        let overlay_candidates = [
            location
                .with_suffix("_overlay")
                .to_asset_path("textures", "png"),
            format!(
                "assets/{}/{}_overlay.png",
                location.namespace, location.path
            ),
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
                if let Some(bytes) = self.open_asset_raw(path) {
                    companions.overlay = Some(bytes);
                    break;
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
        for path in &emissive_candidates {
            if path != &candidates[0] && path != &candidates[1] {
                if let Some(bytes) = self.open_asset_raw(path) {
                    companions.emissive = Some(bytes);
                    break;
                }
            }
        }

        // Emissive MCMETA animation (if emissive has distinct animation e.g. firefly_bush_emissive.png.mcmeta)
        let emissive_meta_candidates = [
            location
                .with_suffix("_emissive")
                .to_mcmeta_asset_path("textures", "png"),
            format!(
                "assets/{}/{}_emissive.png.mcmeta",
                location.namespace, location.path
            ),
        ];
        for path in &emissive_meta_candidates {
            if let Some(meta_bytes) = self.open_asset_raw(path) {
                if let Ok(meta_str) = std::str::from_utf8(&meta_bytes) {
                    if let Ok(tex_meta) = TextureMetadata::parse_json(meta_str) {
                        companions.emissive_mcmeta = tex_meta.animation;
                        break;
                    }
                }
            }
        }

        // 6. MCMETA animation
        let meta_candidates = [
            location.to_mcmeta_asset_path("textures", "png"),
            format!("assets/{}/{}.png.mcmeta", location.namespace, location.path),
        ];
        for path in &meta_candidates {
            if let Some(meta_bytes) = self.open_asset_raw(path) {
                if let Ok(meta_str) = std::str::from_utf8(&meta_bytes) {
                    if let Ok(tex_meta) = TextureMetadata::parse_json(meta_str) {
                        companions.mcmeta = tex_meta.animation;
                        break;
                    }
                }
            }
        }

        companions
    }
}
