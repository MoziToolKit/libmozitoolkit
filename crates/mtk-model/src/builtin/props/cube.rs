//! Vanilla model-space cube primitives.
//!
//! Transcribed from the client's `ModelPart.Cube` / `CubeDefinition` / `CubeListBuilder` so a
//! procedural generator can describe geometry the way the vanilla renderer does — boxes with a
//! texture offset, optional mirroring and a visible-face mask — instead of a bespoke JSON dialect.
//!
//! Conventions (identical to the client):
//! - Coordinates are in `[0, 16]` block units, y up, +z south.
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

    /// Axis-aligned bounds `(min, max)` in model units.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = self.origin;
        let mut max = [
            self.origin[0] + self.dimensions[0],
            self.origin[1] + self.dimensions[1],
            self.origin[2] + self.dimensions[2],
        ];
        if self.mirror {
            std::mem::swap(&mut min[0], &mut max[0]);
        }
        (min, max)
    }

    /// UV rectangle for `face`, in texels, as `ModelPart.Cube` computes it.
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

    #[test]
    fn bounds_use_minimum_corner_and_extents() {
        let cube = CubeSpec::new([0.0, 0.0], [2.0, 3.0, 4.0], [5.0, 6.0, 7.0]);
        let (min, max) = cube.bounds();
        assert_eq!(min, [2.0, 3.0, 4.0]);
        assert_eq!(max, [7.0, 9.0, 11.0]);
    }
}
