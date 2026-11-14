//! Procedural geometric model generator and spatial layout for Minecraft Item Frames.
//!
//! Mirrors `ItemFrameRenderer`, `ItemFrame`, and `template_item_frame.json` from vanilla Minecraft.
//! Generates exact 0.75x0.75x0.0625 block local-space frame geometry with Birch wood borders
//! and Item Frame / Glow Item Frame backplate, supporting 6-direction spatial placement
//! (North, South, East, West, Up/Floor, Down/Ceiling).

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;

use crate::baker::baked_model::model::{BakedElement, BakedFace, BakedModel};
use crate::baker::math::face::{cuboid_face_get_u, cuboid_face_get_v, get_face_canonical_vertex};

/// Total physical width of an item frame in blocks (12/16th block, or 12 pixels).
pub const ITEM_FRAME_WIDTH: f32 = 0.75;

/// Total physical height of an item frame in blocks (12/16th block, or 12 pixels).
pub const ITEM_FRAME_HEIGHT: f32 = 0.75;

/// Total physical thickness of an item frame in blocks (1/16th block, or 1 pixel).
pub const ITEM_FRAME_DEPTH: f32 = 0.0625;

/// Half-depth offset from the origin (1/32th block).
pub const ITEM_FRAME_HALF_DEPTH: f32 = 0.03125;

/// Texture identifier for regular item frame backplate.
pub const ITEM_FRAME_BACK_TEXTURE: &str = "minecraft:block/item_frame";

/// Texture identifier for glowing item frame backplate.
pub const GLOW_ITEM_FRAME_BACK_TEXTURE: &str = "minecraft:block/glow_item_frame";

/// Texture identifier for the wooden border rim.
pub const ITEM_FRAME_WOOD_TEXTURE: &str = "minecraft:block/birch_planks";

/// Computes the canonical Minecraft local-model Euler rotation `(pitch, yaw, roll)` in radians
/// for an item frame given its facing direction (6 degrees of freedom).
///
/// In Minecraft:
/// - Horizontal walls:
///   - North: pitch `0.0`, yaw `0.0`
///   - South: pitch `0.0`, yaw `PI`
///   - West: pitch `0.0`, yaw `FRAC_PI_2`
///   - East: pitch `0.0`, yaw `-FRAC_PI_2`
/// - Vertical placement:
///   - Up (Floor / ground placement): pitch `-FRAC_PI_2`, yaw `PI`
///   - Down (Ceiling placement): pitch `FRAC_PI_2`, yaw `PI`
#[inline]
pub fn facing_to_rotation(facing: Direction) -> Vec3 {
    use std::f32::consts::{FRAC_PI_2, PI};
    match facing {
        Direction::North => Vec3::ZERO,
        Direction::South => Vec3::new(0.0, PI, 0.0),
        Direction::West => Vec3::new(0.0, FRAC_PI_2, 0.0),
        Direction::East => Vec3::new(0.0, -FRAC_PI_2, 0.0),
        Direction::Up => Vec3::new(-FRAC_PI_2, PI, 0.0),
        Direction::Down => Vec3::new(FRAC_PI_2, PI, 0.0),
    }
}

/// Calculates the exact continuous world center position for an item frame,
/// mirroring `ItemFrame.calculateBoundingBox` in vanilla Minecraft.
///
/// * `pos`: Either the block coordinates of the support/anchor block (if `is_support_block == true`)
///   or the air block space occupied by the entity (`TileX, TileY, TileZ`, if `is_support_block == false`).
/// * `facing`: Direction pointing outwards from the attached surface into open air.
/// * `is_support_block`: Whether `pos` is the support block.
pub fn calculate_item_frame_world_position(
    pos: [i32; 3],
    facing: Direction,
    is_support_block: bool,
) -> glam::DVec3 {
    let center = glam::DVec3::new(
        pos[0] as f64 + 0.5,
        pos[1] as f64 + 0.5,
        pos[2] as f64 + 0.5,
    );

    let normal = facing.normal().as_dvec3();
    if is_support_block {
        // Support block: center + normal * (1.0 - 0.46875) = center + normal * 0.53125
        center + normal * 0.53125
    } else {
        // Air block space: center - normal * 0.46875
        center - normal * 0.46875
    }
}

/// Helper to build a `BakedElement` from vanilla raw block-coordinates `[0..16]` shifted to local centered space.
fn build_frame_element(
    from_pos: [f32; 3],
    to_pos: [f32; 3],
    face_defs: &[(Direction, [f32; 4], &str)],
) -> BakedElement {
    // Model center in block coordinates: X=8, Y=8, Z=15.5
    // After dividing by 16 and subtracting center, origin (0,0,0) is at frame center.
    let center_offset = Vec3::new(8.0 / 16.0, 8.0 / 16.0, 15.5 / 16.0);

    let mut faces = HashMap::with_capacity(face_defs.len());
    let mut min_pos = Vec3::splat(f32::INFINITY);
    let mut max_pos = Vec3::splat(f32::NEG_INFINITY);

    for &(dir, uv_bounds, texture) in face_defs {
        let mut verts = [Vec3::ZERO; 4];
        let mut uvs = [Vec2::ZERO; 4];

        for i in 0..4 {
            let raw_p = get_face_canonical_vertex(dir, from_pos, to_pos, i);
            let centered_p = raw_p - center_offset;
            verts[i] = centered_p;
            min_pos = min_pos.min(centered_p);
            max_pos = max_pos.max(centered_p);

            uvs[i] = Vec2::new(
                cuboid_face_get_u(uv_bounds, 0, i, 16.0),
                cuboid_face_get_v(uv_bounds, 0, i, 16.0),
            );
        }

        let v1 = verts[1] - verts[0];
        let v2 = verts[2] - verts[0];
        let normal = v1.cross(v2).normalize_or_zero();

        faces.insert(
            dir,
            BakedFace {
                direction: dir,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds,
                tint_index: -1,
                cullface: None,
                vertices: verts,
                uvs,
                normal,
                atlas_uvs: None,
                atlas_chunk_id: None,
                atlas_texture_id: None,
            },
        );
    }

    BakedElement {
        from_pos: [min_pos.x, min_pos.y, min_pos.z],
        to_pos: [max_pos.x, max_pos.y, max_pos.z],
        faces,
    }
}

/// Creates a procedural `BakedModel` for an item frame in local model space (centered at (0,0,0)).
///
/// Follows Minecraft 1.21 `template_item_frame.json` geometry:
/// - Element 0: Backplate (`#back`, 10x10x0.5 pixels)
/// - Element 1: Bottom wood rim (`#wood`, 12x1x1 pixels)
/// - Element 2: Top wood rim (`#wood`, 12x1x1 pixels)
/// - Element 3: Left (West) wood rim (`#wood`, 1x10x1 pixels)
/// - Element 4: Right (East) wood rim (`#wood`, 1x10x1 pixels)
///
/// When `is_invisible == true`, returns an empty `BakedModel` matching vanilla behavior (`frameModel.clear()`).
pub fn create_item_frame_baked_model(is_glowing: bool, is_invisible: bool) -> BakedModel {
    let block_state = if is_glowing {
        "minecraft:glow_item_frame"
    } else {
        "minecraft:item_frame"
    }
    .to_string();

    if is_invisible {
        return BakedModel {
            block_state,
            ..Default::default()
        };
    }

    let back_texture = if is_glowing {
        GLOW_ITEM_FRAME_BACK_TEXTURE
    } else {
        ITEM_FRAME_BACK_TEXTURE
    };
    let wood_texture = ITEM_FRAME_WOOD_TEXTURE;

    let elements = vec![
        // 0. Backplate: from [3, 3, 15.5] to [13, 13, 16]
        build_frame_element(
            [3.0, 3.0, 15.5],
            [13.0, 13.0, 16.0],
            &[
                (Direction::North, [3.0, 3.0, 13.0, 13.0], back_texture),
                (Direction::South, [3.0, 3.0, 13.0, 13.0], back_texture),
            ],
        ),
        // 1. Bottom wood rim: from [2, 2, 15] to [14, 3, 16]
        build_frame_element(
            [2.0, 2.0, 15.0],
            [14.0, 3.0, 16.0],
            &[
                (Direction::Down, [2.0, 0.0, 14.0, 1.0], wood_texture),
                (Direction::Up, [2.0, 15.0, 14.0, 16.0], wood_texture),
                (Direction::North, [2.0, 13.0, 14.0, 14.0], wood_texture),
                (Direction::South, [2.0, 13.0, 14.0, 14.0], wood_texture),
                (Direction::West, [15.0, 13.0, 16.0, 14.0], wood_texture),
                (Direction::East, [0.0, 13.0, 1.0, 14.0], wood_texture),
            ],
        ),
        // 2. Top wood rim: from [2, 13, 15] to [14, 14, 16]
        build_frame_element(
            [2.0, 13.0, 15.0],
            [14.0, 14.0, 16.0],
            &[
                (Direction::Down, [2.0, 0.0, 14.0, 1.0], wood_texture),
                (Direction::Up, [2.0, 15.0, 14.0, 16.0], wood_texture),
                (Direction::North, [2.0, 2.0, 14.0, 3.0], wood_texture),
                (Direction::South, [2.0, 2.0, 14.0, 3.0], wood_texture),
                (Direction::West, [15.0, 2.0, 16.0, 3.0], wood_texture),
                (Direction::East, [0.0, 2.0, 1.0, 3.0], wood_texture),
            ],
        ),
        // 3. Left (West) wood rim: from [2, 3, 15] to [3, 13, 16]
        build_frame_element(
            [2.0, 3.0, 15.0],
            [3.0, 13.0, 16.0],
            &[
                (Direction::North, [13.0, 3.0, 14.0, 13.0], wood_texture),
                (Direction::South, [2.0, 3.0, 3.0, 13.0], wood_texture),
                (Direction::West, [15.0, 3.0, 16.0, 13.0], wood_texture),
                (Direction::East, [0.0, 3.0, 1.0, 13.0], wood_texture),
            ],
        ),
        // 4. Right (East) wood rim: from [13, 3, 15] to [14, 13, 16]
        build_frame_element(
            [13.0, 3.0, 15.0],
            [14.0, 13.0, 16.0],
            &[
                (Direction::North, [2.0, 3.0, 3.0, 13.0], wood_texture),
                (Direction::South, [13.0, 3.0, 14.0, 13.0], wood_texture),
                (Direction::West, [15.0, 3.0, 16.0, 13.0], wood_texture),
                (Direction::East, [0.0, 3.0, 1.0, 13.0], wood_texture),
            ],
        ),
    ];

    let mut found_faces: [Option<BakedFace>; 6] = [None, None, None, None, None, None];
    for elem in &elements {
        for face in elem.faces.values() {
            let idx = face.direction.to_index();
            if found_faces[idx].is_none() {
                found_faces[idx] = Some(face.clone());
            }
        }
    }

    let default_face = BakedFace {
        texture: wood_texture.to_string(),
        ..Default::default()
    };
    let faces = [
        found_faces[0]
            .clone()
            .unwrap_or_else(|| default_face.clone()),
        found_faces[1]
            .clone()
            .unwrap_or_else(|| default_face.clone()),
        found_faces[2]
            .clone()
            .unwrap_or_else(|| default_face.clone()),
        found_faces[3]
            .clone()
            .unwrap_or_else(|| default_face.clone()),
        found_faces[4]
            .clone()
            .unwrap_or_else(|| default_face.clone()),
        found_faces[5]
            .clone()
            .unwrap_or_else(|| default_face.clone()),
    ];

    BakedModel {
        block_state,
        elements,
        obj_faces: Vec::new(),
        faces,
        is_cube: false,
        is_opaque: false,
        is_emissive: is_glowing,
        cull_meta: None,
        culled_faces: Default::default(),
        unculled_faces: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_frame_dimensions_and_elements() {
        let model = create_item_frame_baked_model(false, false);
        assert_eq!(model.elements.len(), 5);

        // Check overall bounds
        let mut min_bound = Vec3::splat(f32::INFINITY);
        let mut max_bound = Vec3::splat(f32::NEG_INFINITY);
        for el in &model.elements {
            min_bound = min_bound.min(Vec3::from_array(el.from_pos));
            max_bound = max_bound.max(Vec3::from_array(el.to_pos));
        }

        // Width & Height must be 0.75 (from -0.375 to +0.375)
        assert!((min_bound.x - (-0.375)).abs() < 1e-4);
        assert!((max_bound.x - 0.375).abs() < 1e-4);
        assert!((min_bound.y - (-0.375)).abs() < 1e-4);
        assert!((max_bound.y - 0.375).abs() < 1e-4);

        // Depth must be 0.0625 (from -0.03125 to +0.03125)
        assert!((min_bound.z - (-0.03125)).abs() < 1e-4);
        assert!((max_bound.z - 0.03125).abs() < 1e-4);
    }

    #[test]
    fn test_glow_item_frame_texture_and_emissive() {
        let model = create_item_frame_baked_model(true, false);
        assert!(model.is_emissive);
        let back_el = &model.elements[0];
        let north_face = back_el.faces.get(&Direction::North).unwrap();
        assert_eq!(north_face.texture, GLOW_ITEM_FRAME_BACK_TEXTURE);
    }

    #[test]
    fn test_invisible_item_frame_has_no_elements() {
        let model = create_item_frame_baked_model(false, true);
        assert!(model.elements.is_empty());
    }

    #[test]
    fn test_item_frame_facing_rotation() {
        use std::f32::consts::{FRAC_PI_2, PI};
        assert_eq!(facing_to_rotation(Direction::North), Vec3::ZERO);
        assert_eq!(
            facing_to_rotation(Direction::South),
            Vec3::new(0.0, PI, 0.0)
        );
        assert_eq!(
            facing_to_rotation(Direction::West),
            Vec3::new(0.0, FRAC_PI_2, 0.0)
        );
        assert_eq!(
            facing_to_rotation(Direction::East),
            Vec3::new(0.0, -FRAC_PI_2, 0.0)
        );
        assert_eq!(
            facing_to_rotation(Direction::Up),
            Vec3::new(-FRAC_PI_2, PI, 0.0)
        );
        assert_eq!(
            facing_to_rotation(Direction::Down),
            Vec3::new(FRAC_PI_2, PI, 0.0)
        );
    }

    #[test]
    fn test_item_frame_world_position_calculation() {
        let pos = [10, 64, 20];
        // Facing North (-Z), air block calculation:
        // center is (10.5, 64.5, 20.5) - (0, 0, -1) * 0.46875 = (10.5, 64.5, 20.96875)
        let world_pos = calculate_item_frame_world_position(pos, Direction::North, false);
        assert!((world_pos.x - 10.5).abs() < 1e-4);
        assert!((world_pos.y - 64.5).abs() < 1e-4);
        assert!((world_pos.z - 20.96875).abs() < 1e-4);

        // Support block at (10, 64, 21), facing North:
        // center is (10.5, 64.5, 21.5) + (0, 0, -1) * 0.53125 = (10.5, 64.5, 20.96875)
        let world_pos_support =
            calculate_item_frame_world_position([10, 64, 21], Direction::North, true);
        assert!((world_pos_support.z - world_pos.z).abs() < 1e-4);

        // Floor placement (Up, +Y):
        let world_pos_up = calculate_item_frame_world_position(pos, Direction::Up, false);
        assert!((world_pos_up.y - (64.5 - 0.46875)).abs() < 1e-4);
    }
}
