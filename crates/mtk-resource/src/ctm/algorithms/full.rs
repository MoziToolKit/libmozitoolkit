use crate::ctm::tables::CTM_47_LOOKUP;
use glam::IVec3;
use mtk_core::Direction;

/// Solves full 47-tile CTM pattern using explicit 2D face tangents `(up, left)`.
pub fn solve_full_ctm_with_tangents<F>(
    face: Direction,
    tangents: (Direction, Direction),
    inner_seams: bool,
    check_connect: &F,
) -> usize
where
    F: Fn(IVec3) -> bool,
{
    let (up, left) = tangents;
    let down = up.opposite();
    let right = left.opposite();
    let forward = face;

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

    if !inner_seams {
        if (bits & (1 << 7)) == 0 && check_connect(up.offset() + left.offset() + forward.offset()) {
            bits |= 1 << 7;
        }
        if (bits & (1 << 6)) == 0 && check_connect(up.offset() + forward.offset()) {
            bits |= 1 << 6;
        }
        if (bits & (1 << 5)) == 0 && check_connect(up.offset() + right.offset() + forward.offset())
        {
            bits |= 1 << 5;
        }
        if (bits & (1 << 4)) == 0 && check_connect(right.offset() + forward.offset()) {
            bits |= 1 << 4;
        }
        if (bits & (1 << 3)) == 0
            && check_connect(down.offset() + right.offset() + forward.offset())
        {
            bits |= 1 << 3;
        }
        if (bits & (1 << 2)) == 0 && check_connect(down.offset() + forward.offset()) {
            bits |= 1 << 2;
        }
        if (bits & (1 << 1)) == 0 && check_connect(down.offset() + left.offset() + forward.offset())
        {
            bits |= 1 << 1;
        }
        if (bits & 1) == 0 && check_connect(left.offset() + forward.offset()) {
            bits |= 1;
        }
    }

    CTM_47_LOOKUP[bits as usize] as usize
}
