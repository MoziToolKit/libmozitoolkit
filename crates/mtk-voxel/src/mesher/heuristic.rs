//! # Unit Cube Texture Candidates Heuristics
//!
//! Generates directional candidate texture identifiers for unit cube blocks
//! (grass, logs, woods, dirt, etc.).

use mtk_core::Direction;

/// Generates directional candidate texture identifiers for unit cube blocks.
pub(crate) fn get_unit_cube_texture_candidates(clean_block: &str, dir: Direction) -> Vec<String> {
    let mut candidates = Vec::with_capacity(6);
    let is_top = dir == Direction::Up;
    let is_bottom = dir == Direction::Down;
    let is_side = !is_top && !is_bottom;

    let (ns, path) = match clean_block.split_once(':') {
        Some((n, p)) => (Some(n), p),
        None => (None, clean_block),
    };

    let mut push_cand = |stem: &str| {
        if let Some(namespace) = ns {
            candidates.push(format!("{}:block/{}", namespace, stem));
            candidates.push(format!("block/{}", stem));
        } else {
            candidates.push(format!("block/{}", stem));
            candidates.push(format!("minecraft:block/{}", stem));
        }
    };

    if path == "grass_block" || path == "grass" {
        if is_top {
            push_cand("grass_block_top");
            push_cand("grass_top");
        } else if is_bottom {
            push_cand("dirt");
        } else {
            push_cand("grass_block_side");
            push_cand("grass_side");
        }
    } else if path == "podzol" {
        if is_top {
            push_cand("podzol_top");
        } else if is_bottom {
            push_cand("dirt");
        } else {
            push_cand("podzol_side");
        }
    } else if path == "mycelium" {
        if is_top {
            push_cand("mycelium_top");
        } else if is_bottom {
            push_cand("dirt");
        } else {
            push_cand("mycelium_side");
        }
    } else if path == "wall_torch" {
        push_cand("torch");
    } else if path == "soul_wall_torch" {
        push_cand("soul_torch");
    } else if path == "redstone_wall_torch" {
        push_cand("redstone_torch");
    } else if path == "piston_head" || path == "moving_piston" {
        push_cand("piston_top");
        push_cand("piston_side");
    } else if path.contains("log")
        || path.contains("wood")
        || path.contains("pillar")
        || path.contains("stem")
        || path.contains("hyphae")
    {
        if is_top || is_bottom {
            push_cand(&format!("{}_top", path));
            push_cand(&format!("{}_end", path));
        } else {
            push_cand(&format!("{}_side", path));
            push_cand(path);
        }
    } else {
        if is_top {
            push_cand(&format!("{}_top", path));
            push_cand(&format!("{}_up", path));
            push_cand(&format!("{}_end", path));
        } else if is_bottom {
            push_cand(&format!("{}_bottom", path));
            push_cand(&format!("{}_down", path));
            push_cand(&format!("{}_end", path));
        } else if is_side {
            push_cand(&format!("{}_side", path));
            push_cand(&format!("{}_front", path));
        }
        push_cand(path);
    }

    candidates
}
