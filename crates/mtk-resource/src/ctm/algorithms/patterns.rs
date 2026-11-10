use crate::ctm::tables::OVERLAY_17_LOOKUP;
use crate::ctm::types::{coordinate_random, extract_block_name, CtmSymmetry};
use glam::IVec3;
use mtk_core::Direction;

/// Solves an NxM `repeat` CTM pattern using explicit 2D face tangents `(up, left)`.
///
/// The tile grid is anchored to the face's visual orientation: the column index `u`
/// increases along the tangent pointing *right* (opposite of `left`), and the row index
/// `v` increases *downward* (opposite of `up`), matching standard top-left image origin
/// and OptiFine/Continuity repeat semantics. Returns `v * width + u`.
pub(crate) fn solve_repeat_with_tangents(
    _face: Direction,
    tangents: (Direction, Direction),
    world_pos: IVec3,
    width: u32,
    height: u32,
) -> Option<usize> {
    if width == 0 || height == 0 {
        return None;
    }
    let (up, left) = tangents;
    let right = left.opposite();
    let down = up.opposite();

    // Column u increases from left to right along the 'right' tangent vector
    let u_coord = match right {
        Direction::East => world_pos.x,
        Direction::West => -world_pos.x,
        Direction::South => world_pos.z,
        Direction::North => -world_pos.z,
        Direction::Up => world_pos.y,
        Direction::Down => -world_pos.y,
    };
    let u = u_coord.rem_euclid(width as i32) as u32;

    // Row v increases from top to bottom along the 'down' tangent vector
    let v_coord = match down {
        Direction::Down => -world_pos.y,
        Direction::Up => world_pos.y,
        Direction::South => world_pos.z,
        Direction::North => -world_pos.z,
        Direction::East => world_pos.x,
        Direction::West => -world_pos.x,
    };
    let v = v_coord.rem_euclid(height as i32) as u32;

    Some((v * width + u) as usize)
}

pub(crate) fn solve_random<F, S>(
    state: &str,
    face: Direction,
    world_pos: IVec3,
    weights: &[f32],
    total_weight: f32,
    symmetry: CtmSymmetry,
    linked: bool,
    num_tiles: usize,
    get_block: &F,
) -> usize
where
    F: Fn(IVec3) -> Option<S>,
    S: AsRef<str>,
{
    if num_tiles == 0 {
        return 0;
    }
    let (rx, ry, rz) = match symmetry {
        CtmSymmetry::None => (
            face.offset().x * 26,
            face.offset().y * 26,
            face.offset().z * 26,
        ),
        CtmSymmetry::Opposite => {
            let rand_dir = match face {
                Direction::South => Direction::North,
                Direction::West => Direction::East,
                Direction::Down => Direction::Up,
                other => other,
            };
            (
                rand_dir.offset().x * 26,
                rand_dir.offset().y * 26,
                rand_dir.offset().z * 26,
            )
        }
        CtmSymmetry::All => (0, 0, 0),
    };
    let mut qy = world_pos.y + ry;
    if linked {
        if let Some(below_state) = get_block(world_pos + IVec3::new(0, -1, 0)) {
            if extract_block_name(state) == extract_block_name(below_state.as_ref()) {
                qy -= 1;
            }
        }
    }
    let rand_val = coordinate_random(world_pos.x + rx, qy, world_pos.z + rz);
    if weights.is_empty() || total_weight <= 0.0 {
        return ((rand_val * (num_tiles as f32)) as usize).min(num_tiles - 1);
    }
    let target = rand_val * total_weight;
    let mut accum = 0.0f32;
    let mut chosen_idx = 0;
    for (i, &w) in weights.iter().enumerate().take(num_tiles) {
        accum += w;
        if target < accum {
            chosen_idx = i;
            break;
        }
    }
    chosen_idx
}

pub(crate) fn solve_overlay_with_tangents<F>(
    _face: Direction,
    tangents: (Direction, Direction),
    check_connect: &F,
) -> Option<usize>
where
    F: Fn(IVec3) -> bool,
{
    let (up, left) = tangents;
    let down = up.opposite();
    let right = left.opposite();

    let mut bits = 0u8;
    if check_connect(up.offset() + left.offset()) {
        bits |= 1 << 7;
    }
    if check_connect(up.offset()) {
        bits |= 1 << 6;
    }
    if check_connect(up.offset() + right.offset()) {
        bits |= 1 << 5;
    }
    if check_connect(right.offset()) {
        bits |= 1 << 4;
    }
    if check_connect(down.offset() + right.offset()) {
        bits |= 1 << 3;
    }
    if check_connect(down.offset()) {
        bits |= 1 << 2;
    }
    if check_connect(down.offset() + left.offset()) {
        bits |= 1 << 1;
    }
    if check_connect(left.offset()) {
        bits |= 1;
    }

    let tile_idx = OVERLAY_17_LOOKUP[bits as usize];
    if tile_idx >= 0 {
        Some(tile_idx as usize)
    } else {
        None
    }
}
