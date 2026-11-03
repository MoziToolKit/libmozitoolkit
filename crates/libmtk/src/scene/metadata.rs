//! Global scene metadata definitions for `.mtkscene` interchange packages.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Current format version for `.mtkscene` scene interchange packages.
pub const SCENE_PACKAGE_FORMAT_VERSION: u32 = 1;

/// Global metadata manifest for `.mtkscene` scene interchange packages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMetadata {
    /// Format version number (currently 1).
    pub format_version: u32,
    /// Generator identifier (e.g. "MoziToolKit 2.1.0 / libmozitoolkit").
    pub generator: String,
    /// Unix Epoch timestamp in seconds when the package was created.
    pub created_at_epoch_secs: u64,
    /// Spatial bounding box [min_x, min_y, min_z, size_x, size_y, size_z].
    pub bounds: [i32; 6],
    /// Total count of non-air sparse voxels in the scene.
    pub voxel_count: usize,
    /// Number of distinct blockstates present in the palette.
    pub unique_states_count: usize,
    /// Number of distinct biomes present in the palette.
    pub unique_biomes_count: usize,
    /// Number of block entities / hanging entities present in the palette.
    pub unique_entities_count: usize,
    /// Resolution of the accompanying scene mini-atlas [width, height] (or [0, 0] if none).
    pub atlas_resolution: [u32; 2],
    /// Whether the package contains dedicated animated atlas chunks for fluid/fire playback.
    #[serde(default)]
    pub has_animated_atlas: bool,
    /// Custom key-value properties (scene title, author, description, tags, etc.).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_props: HashMap<String, String>,
}

impl SceneMetadata {
    /// Creates a default `SceneMetadata` for a given bounding box and voxel count.
    pub fn new(bounds: [i32; 6], voxel_count: usize) -> Self {
        let epoch_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            format_version: SCENE_PACKAGE_FORMAT_VERSION,
            generator: format!("libmozitoolkit {}", env!("CARGO_PKG_VERSION")),
            created_at_epoch_secs: epoch_now,
            bounds,
            voxel_count,
            unique_states_count: 0,
            unique_biomes_count: 0,
            unique_entities_count: 0,
            atlas_resolution: [0, 0],
            has_animated_atlas: false,
            extra_props: HashMap::new(),
        }
    }
}
