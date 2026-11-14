#![forbid(unsafe_code)]
//! Configuration options for procedural 2D extruded item model baking.

use serde::{Deserialize, Serialize};

/// Configuration options for baking 2D extruded item models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemMeshOptions {
    /// Physical thickness in blocks (defaults to 1/16th block = 0.0625).
    pub depth: f32,
    /// Texture UV shrink factor in pixels to prevent sampling background bleeding (defaults to 0.1).
    pub uv_shrink: f32,
    /// Whether to enable 2D greedy meshing for front/back and 1D greedy span merging for sides (defaults to true).
    pub use_greedy_meshing: bool,
    /// Alpha channel threshold for determining whether a pixel is solid (defaults to 1, i.e. A > 0).
    pub alpha_threshold: u8,
}

impl Default for ItemMeshOptions {
    fn default() -> Self {
        Self {
            depth: 1.0 / 16.0,
            uv_shrink: 0.1,
            use_greedy_meshing: true,
            alpha_threshold: 1,
        }
    }
}
