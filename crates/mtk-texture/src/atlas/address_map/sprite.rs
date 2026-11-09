use mtk_resource::AnimationMetadata;
use serde::{Deserialize, Serialize};

/// Source priority level for atlas address map keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtlasKeySource {
    /// Authoritative prebaked static asset from resource packs (higher priority).
    Static = 0,
    /// Procedurally registered dynamic entity sprite (e.g. skin, banner).
    Dynamic = 1,
}

/// Classification of sprite placement and usage mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SpriteKind {
    /// Placed on a static chunk (either a truly static sprite or Frame 0 of an animated sprite).
    #[default]
    StaticAtlas,
    /// Placed on a dedicated animated chunk (full vertical strip, frame_count > 1).
    AnimatedAtlas,
    /// Standalone-only texture.
    StandaloneOnly,
}

/// Address and metrics for a single sprite located within a stitched Atlas Chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasSpriteLocation {
    pub chunk_id: u16,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub is_animated: bool,
    #[serde(default)]
    pub sprite_kind: SpriteKind,
    #[serde(default)]
    pub texture_id: u32,
    /// Normalized UV bounds: `[u_min, v_min, u_max, v_max]` in [0.0..1.0] atlas space (Frame 0 or static frame).
    pub uv_bounds: [f32; 4],
    /// Normalized UV bounds of Frame 0 specifically: `[u_min, v_min, u_max, v_max]`.
    #[serde(default)]
    pub frame_0_uv_bounds: [f32; 4],
    /// Normalized local UV bounds in single-block [0.0..1.0] space (default [0.0, 0.0, 1.0, 1.0]).
    #[serde(default = "default_local_uv_bounds")]
    pub local_uv_bounds: [f32; 4],
    /// Step size in UV space per animation frame: `[u_step, v_step]`.
    #[serde(default)]
    pub frame_uv_step: [f32; 2],
    /// Physical pixel rectangle on the atlas chunk for Frame 0: `[x, y, width, height]`.
    #[serde(default)]
    pub pixel_rect: [u32; 4],
    /// Physical pixel rectangle on the atlas chunk for the entire animation strip: `[x, y, width, height]`.
    #[serde(default)]
    pub strip_pixel_rect: [u32; 4],
    /// Single-frame physical resolution: `[frame_width, frame_height]`.
    #[serde(default)]
    pub frame_size: [u32; 2],
    /// Total animation frames.
    pub frame_count: u32,
    /// Optional animation timing metadata.
    pub animation: Option<AnimationMetadata>,
    /// Whether this slot contains a non-default Normal companion map.
    pub has_normal: bool,
    /// Whether this slot contains a non-default Specular companion map.
    pub has_specular: bool,
    /// Whether this slot contains a non-default Overlay companion map.
    #[serde(default)]
    pub has_overlay: bool,
    /// Average solid color of the texture as a hex string `#RRGGBB` (excluding alpha).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_color: Option<String>,
    /// Whether this texture is 100% opaque without any transparent or cutout pixels.
    #[serde(default = "default_true")]
    pub is_opaque: bool,
    /// Optional slot index for dynamic entity atlases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_index: Option<u16>,
}

impl Default for AtlasSpriteLocation {
    fn default() -> Self {
        Self {
            chunk_id: 0,
            category: default_category(),
            is_animated: false,
            sprite_kind: SpriteKind::default(),
            texture_id: 0,
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            frame_0_uv_bounds: [0.0, 0.0, 1.0, 1.0],
            local_uv_bounds: default_local_uv_bounds(),
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
            is_opaque: true,
            slot_index: None,
        }
    }
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_category() -> String {
    "blocks".to_string()
}

pub(crate) fn default_local_uv_bounds() -> [f32; 4] {
    [0.0, 0.0, 1.0, 1.0]
}
