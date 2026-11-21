use crate::ctm::tables::CTM_47_LOOKUP;
use glam::IVec3;
use mtk_core::Direction;

/// Solves full 47-tile CTM pattern using explicit 2D face tangents `(up, left)`.
pub(crate) fn solve_full_ctm_with_tangents<F>(
    _face: Direction,
    tangents: (Direction, Direction),
    check_connect: &F,
) -> usize
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

    CTM_47_LOOKUP[bits as usize] as usize
}
