//! Vanilla model-space cube primitives.
//!
//! Transcribed from the client's `ModelPart.Cube` / `CubeDefinition` / `CubeListBuilder` so a
//! procedural generator can describe geometry the way the vanilla renderer does — boxes with a
//! texture offset, optional mirroring and a visible-face mask — instead of a bespoke JSON dialect.
//!
//! Conventions:
//! - Coordinates are in `[0, 16]` block units, **y up**, +z south — the block-model space our
//!   baker uses. The client's `ModelPart` space is y *down* (the renderer applies
//!   `scale(-1, -1, 1)`), so a box transcribed from a `*Model` class must be flipped on y and its
//!   face UVs permuted; [`CubeSpec::model_face_uvs`] does the UV part, [`flip_y`] the geometry.
//! - `origin` is the box's minimum corner; `dimensions` are its extents.
//! - UVs are returned in texture *pixels*; the caller divides by `texture_size / 16` to reach the
//!   `[0, 16]` model space used by `BlockModelJson`.
//! - Only the faces in `visible` are emitted, matching `CubeListBuilder.addBox(..., visibleFaces)`.

use mtk_core::Direction;

/// A single textured box in model space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubeSpec {
    /// Minimum corner in `[0, 16]` model units.
    pub origin: [f32; 3],
    /// Full extents `(width, height, depth)` in model units.
    pub dimensions: [f32; 3],
    /// Texture offset in texels (`texOffs(u, v)`).
    pub tex_offs: [f32; 2],
    /// Mirrors the box across its own x axis, as `CubeListBuilder.mirror()` does.
    pub mirror: bool,
}

impl CubeSpec {
    /// Builds a cube spec from `addBox(x, y, z, w, h, d)` plus a `texOffs`.
    pub fn new(tex_offs: [f32; 2], origin: [f32; 3], dimensions: [f32; 3]) -> Self {
        Self {
            origin,
            dimensions,
            tex_offs,
            mirror: false,
        }
    }

    /// UV rectangle for `face` in the block-model space, in texels.
    ///
    /// Three effects of the `ModelPart` -> block-model conversion are encoded here:
    ///
    /// 1. **Panel choice.** [`model_corners`] emits a y-inverted corner pair, so the baker places
    ///    the `down` key on the *upper* plane and the `up` key on the *lower* one. The client's
    ///    panels therefore swap between the keys, and north/south swap as well because the box is
    ///    mirrored in depth.
    /// 2. **V orientation.** The client's polygon assigns UVs as `v0←(u_end, v_start)`,
    ///    `v1←(u_start, v_start)`, `v2←(u_start, v_end)`, `v3←(u_end, v_end)`, while the baker
    ///    assigns `v0←(min_u, min_v)`, `v1←(min_u, max_v)`, `v2←(max_u, max_v)`,
    ///    `v3←(max_u, min_v)`. For the plane that receives the baker's `down` mapping the orders
    ///    differ by a `v0 <-> v2` swap, so that rectangle is handed over v-descending; the plane
    ///    that receives the baker's `up` mapping (its mirror) keeps v ascending.
    /// 3. East/west are unaffected and stay ascending.
    pub fn model_face_uvs(&self, face: Direction) -> [f32; 4] {
        let rect = match face {
            Direction::Up => self.face_uvs(Direction::Down),
            Direction::Down => self.face_uvs(Direction::Up),
            Direction::North => self.face_uvs(Direction::South),
            Direction::South => self.face_uvs(Direction::North),
            Direction::East | Direction::West => self.face_uvs(face),
        };
        let (u0, u1) = (rect[0].min(rect[2]), rect[0].max(rect[2]));
        let (v0, v1) = (rect[1].min(rect[3]), rect[1].max(rect[3]));
        match face {
            // Rendered on the upper plane (see `model_corners`): needs the reversed v.
            Direction::Down => [u0, v1, u1, v0],
            _ => [u0, v0, u1, v1],
        }
    }

    /// UV rectangle for `face`, in texels, as `ModelPart.Cube` computes it (y-down space).
    ///
    /// Returns `None` for faces hidden by the caller's mask (handled outside this type) — the
    /// rectangle itself is always well defined.
    pub fn face_uvs(&self, face: Direction) -> [f32; 4] {
        let [u, v] = self.tex_offs;
        let [w, h, d] = self.dimensions;

        let u0 = u;
        let u1 = u + d;
        let u2 = u + d + w;
        let u22 = u + d + w + w;
        let u3 = u + d + w + d;
        let u4 = u + d + w + d + w;
        let v0 = v;
        let v1 = v + d;
        let v2 = v + d + h;

        match face {
            Direction::Down => [u1, v0, u2, v1],
            Direction::Up => [u2, v1, u22, v0],
            Direction::West => [u0, v1, u1, v2],
            Direction::North => [u1, v1, u2, v2],
            Direction::East => [u2, v1, u3, v2],
            Direction::South => [u3, v1, u4, v2],
        }
    }
}

/// Builds the `(from, to)` pair the block-model baker expects for a box.
///
/// The baker derives face winding from the corner order, so the emitted `from`/`to` are not simply
/// min/max: x and y come from the maximum corner while z comes from the minimum, which is the
/// ordering that matches the client's rendering once the box is expressed on a y-up, +z-south grid.
pub fn model_corners(min: [f32; 3], max: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    ([max[0], max[1], min[2]], [min[0], min[1], max[2]])
}

/// Faces of a box that are visible from outside, i.e. `allOfEnumExcept(hidden)`.
///
/// Vanilla uses this to drop the shared face of paired models; the corresponding texture region is
/// blank (verified for `entity/chest/normal_left.png`: the west panel rect is fully transparent),
/// so keeping the face would only add a transparent quad.
pub fn visible_faces_except(hidden: &[Direction]) -> Vec<Direction> {
    CUBE_FACES
        .into_iter()
        .filter(|face| !hidden.contains(face))
        .collect()
}

/// All six faces in the order the client emits them (`Cube` constructor order).
pub const CUBE_FACES: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::West,
    Direction::North,
    Direction::East,
    Direction::South,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Chest body from `ChestModel.createSingleBodyLayer`:
    /// `texOffs(0, 19).addBox(1, 0, 1, 14, 10, 14)` on a 64x64 texture.
    #[test]
    fn chest_body_uvs_match_the_vanilla_unwrap() {
        let cube = CubeSpec::new([0.0, 19.0], [1.0, 0.0, 1.0], [14.0, 10.0, 14.0]);
        // Values below are `ModelPart.Cube`'s rectangles divided by 4 (64px texture -> 16 units).
        assert_eq!(cube.face_uvs(Direction::Down), [14.0, 19.0, 28.0, 33.0]);
        assert_eq!(cube.face_uvs(Direction::Up), [28.0, 33.0, 42.0, 19.0]);
        assert_eq!(cube.face_uvs(Direction::West), [0.0, 33.0, 14.0, 43.0]);
        assert_eq!(cube.face_uvs(Direction::North), [14.0, 33.0, 28.0, 43.0]);
        assert_eq!(cube.face_uvs(Direction::East), [28.0, 33.0, 42.0, 43.0]);
        assert_eq!(cube.face_uvs(Direction::South), [42.0, 33.0, 56.0, 43.0]);

        // Div by 4 gives the model-space rects the block model format uses.
        let up = cube.face_uvs(Direction::Up);
        assert_eq!(
            [up[0] / 4.0, up[1] / 4.0, up[2] / 4.0, up[3] / 4.0],
            [7.0, 8.25, 10.5, 4.75]
        );
    }

    #[test]
    fn visible_face_mask_drops_only_the_requested_faces() {
        let faces = visible_faces_except(&[Direction::East]);
        assert_eq!(faces.len(), 5);
        assert!(!faces.contains(&Direction::East));
        assert!(faces.contains(&Direction::West));
    }

    /// The corner order the baker expects: x/y from the maximum, z from the minimum.
    #[test]
    fn model_corners_put_x_and_y_at_the_maximum() {
        let (from, to) = model_corners([1.0, 9.0, 1.0], [15.0, 14.0, 15.0]);
        assert_eq!(from, [15.0, 14.0, 1.0]);
        assert_eq!(to, [1.0, 9.0, 15.0]);
    }
}
