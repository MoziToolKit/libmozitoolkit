//! The manifest chunk, shared by every package profile.
//!
//! `.mtkcache` and `.mtkscene` are the same container and now carry the same manifest shape: one
//! struct that both writers fill in, with the fields a profile does not use left at their defaults.
//! Readers validate `format_version` against their own profile constant, which is why the version
//! numbers stay profile-specific even though the layout is shared.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Payload revision of an asset-cache package.
///
/// Bump whenever the baked payloads change meaning (geometry, atlas layout, manifest layout): old
/// packages are rejected rather than served.
/// v14: CTM rules gained `inner_seams`/`biomes_negated` and the method enum lost its per-variant
///      `inner_seams` field (invalid rules are dropped, so rule sets can shrink).
/// v13: animated atlas strips follow `.mcmeta`'s `frames` list (reordered / repeated), so the
///      played strip is no longer the file order and `animation.frames` is no longer exported.
/// v12: every atlas sprite is padded (replicated edges) so bilinear/mip taps cannot read a
///      neighbour sprite; sprite rects and UVs are unchanged.
/// v11: unified manifest (scene fields are present but defaulted).
/// v10: chest up/down faces carry the corrected v orientation; copper/ender sheet selection.
pub const ASSET_CACHE_FORMAT_VERSION: u32 = 14;

/// Payload revision of a scene interchange package.
///
/// v6: CTM rule payload matches asset-cache v14 (per-rule `inner_seams`, `biomes_negated`).
/// v5: animated strips follow `.mcmeta`'s `frames` list, matching asset-cache v13.
/// v4: every atlas sprite is padded (replicated edges), matching asset-cache v12.
/// v3: unified manifest; the continuous visual entity payload is gone.
/// v2: removed the continuous visual entity payload.
pub const SCENE_PACKAGE_FORMAT_VERSION: u32 = 6;

/// Capability flags a package may advertise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SceneCapabilities {
    /// Whether custom colormaps (grass, foliage, dry_foliage) are bundled.
    #[serde(default)]
    pub has_colormaps: bool,
    /// Whether connected textures (CTM) rule definitions are bundled.
    #[serde(default)]
    pub has_ctm: bool,
    /// Whether canonical PBR textures (_orm, _aux) are bundled.
    #[serde(default)]
    pub has_pbr: bool,
    /// Whether dynamic entity slots or textures (skins, banners) are bundled.
    #[serde(default)]
    pub has_dynamic_entities: bool,
}

/// Manifest of a package, whatever its profile.
///
/// Every field beyond the identity triple is defaulted, so an asset cache writes the counts it
/// knows and a scene package writes its voxel description without either one inventing values for
/// the other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    /// Payload revision for the package's profile (see the two constants above).
    pub format_version: u32,
    /// Fingerprint of the resource-pack stack the package was built from.
    #[serde(default)]
    pub fingerprint: String,
    /// Generator identifier (e.g. "MoziToolKit 2.1.0 / libmozitoolkit").
    #[serde(default)]
    pub generator: String,
    /// Unix epoch seconds when the package was written.
    #[serde(default)]
    pub created_at_epoch_secs: u64,

    // ---- asset-cache counts ----
    /// Number of resource packs in the stack.
    #[serde(default)]
    pub pack_count: usize,
    /// Number of atlas chunk images.
    #[serde(default)]
    pub atlas_chunks: usize,
    /// Number of baked models.
    #[serde(default)]
    pub baked_models: usize,

    // ---- scene description ----
    /// Spatial bounding box `[min_x, min_y, min_z, size_x, size_y, size_z]`.
    #[serde(default)]
    pub bounds: [i32; 6],
    /// Total count of non-air sparse voxels.
    #[serde(default)]
    pub voxel_count: usize,
    /// Number of distinct blockstates in the palette.
    #[serde(default)]
    pub unique_states_count: usize,
    /// Number of distinct biomes in the palette.
    #[serde(default)]
    pub unique_biomes_count: usize,
    /// Number of block entities in the sparse voxel palette.
    #[serde(default)]
    pub unique_entities_count: usize,
    /// Resolution of the accompanying mini-atlas `[width, height]`, `[0, 0]` when absent.
    #[serde(default)]
    pub atlas_resolution: [u32; 2],
    /// Whether dedicated animated atlas chunks (fluid/fire) are bundled.
    #[serde(default)]
    pub has_animated_atlas: bool,
    /// Active feature and asset capabilities.
    #[serde(default)]
    pub capabilities: crate::package::SceneCapabilities,
    /// Custom key-value properties (title, author, tags, ...).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[serde(serialize_with = "mtk_core::serialize_sorted_map")]
    pub extra_props: HashMap<String, String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn generator_id() -> String {
    format!("libmozitoolkit {}", env!("CARGO_PKG_VERSION"))
}

impl PackageManifest {
    /// Empty manifest at the asset-cache revision.
    pub fn new_cache(fingerprint: String, pack_count: usize) -> Self {
        Self {
            format_version: ASSET_CACHE_FORMAT_VERSION,
            fingerprint,
            generator: generator_id(),
            created_at_epoch_secs: now_secs(),
            pack_count,
            ..Default::default()
        }
    }

    /// Empty manifest at the scene revision.
    pub fn new_scene(bounds: [i32; 6], voxel_count: usize) -> Self {
        Self {
            format_version: SCENE_PACKAGE_FORMAT_VERSION,
            generator: generator_id(),
            created_at_epoch_secs: now_secs(),
            bounds,
            voxel_count,
            ..Default::default()
        }
    }

    /// Reads the manifest chunk of a package file without loading anything else.
    pub fn read_from_package(path: &std::path::Path) -> Option<Self> {
        let reader = mtk_package::MtkPackageReader::open_file(path).ok()?;
        reader.read_chunk_json("manifest").ok()
    }

    /// Whether this manifest describes the asset cache `fingerprint` at the current revision.
    pub fn is_valid_cache_for(&self, fingerprint: &str) -> bool {
        self.format_version == ASSET_CACHE_FORMAT_VERSION && self.fingerprint == fingerprint
    }

    /// Whether this manifest is a scene package this build can read.
    pub fn is_valid_scene(&self) -> bool {
        self.format_version == SCENE_PACKAGE_FORMAT_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_profile_specific() {
        assert_ne!(ASSET_CACHE_FORMAT_VERSION, SCENE_PACKAGE_FORMAT_VERSION);
    }

    #[test]
    fn a_cache_manifest_is_not_a_scene_manifest() {
        let cache = PackageManifest::new_cache("mtk_fp_0011223344556677".to_string(), 2);
        assert!(cache.is_valid_cache_for("mtk_fp_0011223344556677"));
        assert!(!cache.is_valid_cache_for("mtk_fp_other"));
        assert!(!cache.is_valid_scene());

        let scene = PackageManifest::new_scene([0, 0, 0, 1, 1, 1], 1);
        assert!(scene.is_valid_scene());
        assert!(!scene.is_valid_cache_for("mtk_fp_0011223344556677"));
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let mut manifest = PackageManifest::new_scene([1, 2, 3, 4, 5, 6], 42);
        manifest.capabilities.has_ctm = true;
        manifest
            .extra_props
            .insert("title".to_string(), "scene".to_string());

        let json = serde_json::to_string(&manifest).expect("serializes");
        let parsed: PackageManifest = serde_json::from_str(&json).expect("parses");
        assert_eq!(parsed, manifest);
    }
}
