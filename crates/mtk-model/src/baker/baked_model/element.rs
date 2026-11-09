use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;
use serde::{Deserialize, Serialize};

/// Represents a baked quad face with calculated geometry, UVs, and texture bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedFace {
    pub direction: Direction,
    pub texture: String,
    pub uv_rot: f32,
    pub uv_bounds: [f32; 4],
    pub tint_index: i16,
    pub cullface: Option<Direction>,
    pub vertices: [Vec3; 4],
    pub uvs: [Vec2; 4],
    pub normal: Vec3,
    #[serde(default)]
    pub atlas_uvs: Option<[Vec2; 4]>,
    #[serde(default)]
    pub atlas_chunk_id: Option<u16>,
    #[serde(default)]
    pub atlas_texture_id: Option<u32>,
}

impl BakedFace {
    /// Remaps this face's local UVs to absolute Atlas UV coordinates given frame 0 UV bounds.
    pub fn remap_to_atlas_bounds(
        &mut self,
        frame_0_uv_bounds: [f32; 4],
        chunk_id: u16,
        texture_id: u32,
    ) {
        let u_min = frame_0_uv_bounds[0];
        let v_min = frame_0_uv_bounds[1];
        let u_span = frame_0_uv_bounds[2] - u_min;
        let v_span = frame_0_uv_bounds[3] - v_min;
        self.atlas_uvs = Some([
            Vec2::new(
                u_min + self.uvs[0].x * u_span,
                v_min + (1.0 - self.uvs[0].y) * v_span,
            ),
            Vec2::new(
                u_min + self.uvs[1].x * u_span,
                v_min + (1.0 - self.uvs[1].y) * v_span,
            ),
            Vec2::new(
                u_min + self.uvs[2].x * u_span,
                v_min + (1.0 - self.uvs[2].y) * v_span,
            ),
            Vec2::new(
                u_min + self.uvs[3].x * u_span,
                v_min + (1.0 - self.uvs[3].y) * v_span,
            ),
        ]);
        self.atlas_chunk_id = Some(chunk_id);
        self.atlas_texture_id = Some(texture_id);
    }
}

impl Default for BakedFace {
    fn default() -> Self {
        Self {
            direction: Direction::Up,
            texture: String::new(),
            uv_rot: 0.0,
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            tint_index: -1,
            cullface: None,
            vertices: [Vec3::ZERO; 4],
            uvs: [Vec2::ZERO; 4],
            normal: Vec3::Y,
            atlas_uvs: None,
            atlas_chunk_id: None,
            atlas_texture_id: None,
        }
    }
}

/// Represents a 3D box element within a baked model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedElement {
    pub from_pos: [f32; 3],
    pub to_pos: [f32; 3],
    pub faces: HashMap<Direction, BakedFace>,
}

impl BakedElement {
    /// Returns true if this element has inverted coordinates (`from > to` in any axis),
    /// which vanilla Minecraft uses to create inward-facing interior geometry (e.g. spawners, vaults).
    pub fn is_inverted(&self) -> bool {
        self.from_pos[0] > self.to_pos[0] + 1e-4
            || self.from_pos[1] > self.to_pos[1] + 1e-4
            || self.from_pos[2] > self.to_pos[2] + 1e-4
    }
}
