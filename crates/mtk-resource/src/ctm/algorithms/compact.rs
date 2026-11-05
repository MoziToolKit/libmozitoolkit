use glam::IVec3;
use mtk_core::Direction;

/// Solves 5-tile compact CTM pattern using explicit 2D face tangents `(up, left)`.
pub fn solve_compact_ctm_with_tangents<F>(
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

    let up_c = check_connect(up.offset());
    let down_c = check_connect(down.offset());
    let left_c = check_connect(left.offset());
    let right_c = check_connect(right.offset());

    let v = up_c && down_c;
    let h = left_c && right_c;
    let any_v = up_c || down_c;
    let any_h = left_c || right_c;

    match (any_v, any_h) {
        (true, true) => {
            let ul = check_connect(up.offset() + left.offset());
            let ur = check_connect(up.offset() + right.offset());
            let dl = check_connect(down.offset() + left.offset());
            let dr = check_connect(down.offset() + right.offset());
            if v && h && ul && ur && dl && dr {
                1
            } else {
                4
            }
        }
        (true, false) => 2,
        (false, true) => 3,
        (false, false) => 0,
    }
}
