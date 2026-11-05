//! Zero-thickness planar element face deduplication and single-sided baking.
//!
//! Minecraft elements with zero thickness (plants like cross.json, crops, ladders, rails, decals)
//! define duplicate back-to-back faces with opposite normals purely to counteract OpenGL backface culling.
//! In modern DCC pipelines (Blender), double-sided rendering is handled via shaders, and overlapping
//! coplanar geometry causes Z-fighting and gets mistakenly culled as occluded interior contact faces.

use std::collections::HashMap;

use mtk_core::Direction;

use crate::parser::model_json::ResolvedFace;

/// Check if a face should be skipped due to planar zero-thickness deduplication.
#[inline]
pub fn should_skip_zero_thickness_face(
    from_pos: [f32; 3],
    to_pos: [f32; 3],
    orig_dir: Direction,
    faces: &HashMap<String, ResolvedFace>,
) -> bool {
    let is_zero_x = (from_pos[0] - to_pos[0]).abs() < 1e-4;
    let is_zero_y = (from_pos[1] - to_pos[1]).abs() < 1e-4;
    let is_zero_z = (from_pos[2] - to_pos[2]).abs() < 1e-4;

    if is_zero_z {
        // Skip degenerate edge faces parallel to normal
        if orig_dir != Direction::North && orig_dir != Direction::South {
            return true;
        }
        if let (Some(f_north), Some(f_south)) = (faces.get("north"), faces.get("south")) {
            if f_north.texture == f_south.texture && f_north.tintindex == f_south.tintindex {
                let keep_dir = if from_pos[2] > 8.0 + 1e-4 {
                    Direction::North
                } else if from_pos[2] < 8.0 - 1e-4 {
                    Direction::South
                } else {
                    Direction::North
                };
                if orig_dir != keep_dir {
                    return true;
                }
            }
        }
    } else if is_zero_x {
        // Skip degenerate edge faces parallel to normal
        if orig_dir != Direction::West && orig_dir != Direction::East {
            return true;
        }
        if let (Some(f_west), Some(f_east)) = (faces.get("west"), faces.get("east")) {
            if f_west.texture == f_east.texture && f_west.tintindex == f_east.tintindex {
                let keep_dir = if from_pos[0] > 8.0 + 1e-4 {
                    Direction::West
                } else if from_pos[0] < 8.0 - 1e-4 {
                    Direction::East
                } else {
                    Direction::West
                };
                if orig_dir != keep_dir {
                    return true;
                }
            }
        }
    } else if is_zero_y {
        // Skip degenerate edge faces parallel to normal
        if orig_dir != Direction::Down && orig_dir != Direction::Up {
            return true;
        }
        if let (Some(f_down), Some(f_up)) = (faces.get("down"), faces.get("up")) {
            if f_down.texture == f_up.texture && f_down.tintindex == f_up.tintindex {
                let keep_dir = if from_pos[1] >= 15.0 - 1e-4 {
                    Direction::Down
                } else {
                    Direction::Up
                };
                if orig_dir != keep_dir {
                    return true;
                }
            }
        }
    }

    false
}
