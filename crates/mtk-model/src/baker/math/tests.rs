use super::*;
use mtk_core::Direction;

#[test]
fn test_unit_cube_face_normals() {
    for dir in Direction::ALL {
        let baked = bake_face_exact(
            dir,
            [0.0, 0.0, 0.0],
            [16.0, 16.0, 16.0],
            None,
            0.0,
            0.0,
            0.0,
            None,
            None,
            false,
            16.0,
        );
        assert_eq!(baked.direction, dir);
    }
}

#[test]
fn test_rotate_direction() {
    // Rotating North face around Y by 90 deg clockwise -> East
    let new_dir = rotate_direction(Direction::North, 0.0, 90.0);
    assert_eq!(new_dir, Direction::East);

    // Rotating Up face around X by 90 deg -> North
    let new_dir_x = rotate_direction(Direction::Up, 90.0, 0.0);
    assert_eq!(new_dir_x, Direction::North);
}
