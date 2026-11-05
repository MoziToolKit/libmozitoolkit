//! # Unit Cube Texture Candidates Heuristics
//!
//! Generates directional candidate texture identifiers for unit cube blocks
//! (grass, logs, woods, dirt, etc.).

use mtk_core::Direction;

/// Generates directional candidate texture identifiers for unit cube blocks.
pub fn get_unit_cube_texture_candidates(clean_block: &str, dir: Direction) -> Vec<String> {
    let mut candidates = Vec::with_capacity(4);
    let is_top = dir == Direction::Up;
    let is_bottom = dir == Direction::Down;
    let is_side = !is_top && !is_bottom;

    if clean_block == "grass_block" || clean_block == "grass" {
        if is_top {
            candidates.push("block/grass_block_top".to_string());
            candidates.push("block/grass_top".to_string());
        } else if is_bottom {
            candidates.push("block/dirt".to_string());
        } else {
            candidates.push("block/grass_block_side".to_string());
            candidates.push("block/grass_side".to_string());
        }
    } else if clean_block == "podzol" {
        if is_top {
            candidates.push("block/podzol_top".to_string());
        } else if is_bottom {
            candidates.push("block/dirt".to_string());
        } else {
            candidates.push("block/podzol_side".to_string());
        }
    } else if clean_block == "mycelium" {
        if is_top {
            candidates.push("block/mycelium_top".to_string());
        } else if is_bottom {
            candidates.push("block/dirt".to_string());
        } else {
            candidates.push("block/mycelium_side".to_string());
        }
    } else if clean_block == "wall_torch" {
        candidates.push("block/torch".to_string());
    } else if clean_block == "soul_wall_torch" {
        candidates.push("block/soul_torch".to_string());
    } else if clean_block == "redstone_wall_torch" {
        candidates.push("block/redstone_torch".to_string());
    } else if clean_block == "piston_head" || clean_block == "moving_piston" {
        candidates.push("block/piston_top".to_string());
        candidates.push("block/piston_side".to_string());
    } else if clean_block.contains("log")
        || clean_block.contains("wood")
        || clean_block.contains("pillar")
        || clean_block.contains("stem")
        || clean_block.contains("hyphae")
    {
        if is_top || is_bottom {
            candidates.push(format!("block/{}_top", clean_block));
            candidates.push(format!("block/{}_end", clean_block));
        } else {
            candidates.push(format!("block/{}_side", clean_block));
            candidates.push(format!("block/{}", clean_block));
        }
    } else {
        if is_top {
            candidates.push(format!("block/{}_top", clean_block));
            candidates.push(format!("block/{}_up", clean_block));
            candidates.push(format!("block/{}_end", clean_block));
        } else if is_bottom {
            candidates.push(format!("block/{}_bottom", clean_block));
            candidates.push(format!("block/{}_down", clean_block));
            candidates.push(format!("block/{}_end", clean_block));
        } else if is_side {
            candidates.push(format!("block/{}_side", clean_block));
            candidates.push(format!("block/{}_front", clean_block));
        }
        candidates.push(format!("block/{}", clean_block));
    }

    candidates
}
