use glam::{Vec2, Vec3};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::direction::Direction;

/// 2D Axis-Aligned Bounding Box (AABB) in normalized [0, 1] or pixel space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Aabb2d {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb2d {
    pub const ZERO: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ZERO,
    };

    pub const UNIT: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ONE,
    };

    #[inline]
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn from_min_max(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min: Vec2::new(min_x.min(max_x), min_y.min(max_y)),
            max: Vec2::new(min_x.max(max_x), min_y.max(max_y)),
        }
    }

    #[inline]
    pub fn from_points(points: &[[f32; 2]]) -> Self {
        if points.is_empty() {
            return Self::ZERO;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &[x, y] in points {
            if x < min_x {
                min_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
        }
        Self::from_min_max(min_x, min_y, max_x, max_y)
    }

    #[inline]
    pub fn width(&self) -> f32 {
        (self.max.x - self.min.x).max(0.0)
    }

    #[inline]
    pub fn height(&self) -> f32 {
        (self.max.y - self.min.y).max(0.0)
    }

    #[inline]
    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    #[inline]
    pub fn is_empty(&self, eps: f32) -> bool {
        self.width() <= eps || self.height() <= eps
    }

    #[inline]
    pub fn contains_rect(&self, other: &Self, eps: f32) -> bool {
        self.min.x <= other.min.x + eps
            && self.max.x >= other.max.x - eps
            && self.min.y <= other.min.y + eps
            && self.max.y >= other.max.y - eps
    }

    #[inline]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }

    #[inline]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let min_x = self.min.x.max(other.min.x);
        let min_y = self.min.y.max(other.min.y);
        let max_x = self.max.x.min(other.max.x);
        let max_y = self.max.y.min(other.max.y);

        if min_x < max_x && min_y < max_y {
            Some(Self {
                min: Vec2::new(min_x, min_y),
                max: Vec2::new(max_x, max_y),
            })
        } else {
            None
        }
    }
}

/// A 3D planar Quad composed of 4 vertices and corresponding UVs.
/// Follows Minecraft standard winding order for counter-clockwise face rendering.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Quad {
    pub vertices: [Vec3; 4],
    pub uvs: [Vec2; 4],
    pub normal: Vec3,
}

impl Quad {
    /// Constructs a quad and calculates normal using cross product.
    pub fn new(vertices: [Vec3; 4], uvs: [Vec2; 4]) -> Self {
        let edge1 = vertices[1] - vertices[0];
        let edge2 = vertices[2] - vertices[0];
        let normal = edge1.cross(edge2).normalize_or_zero();
        Self {
            vertices,
            uvs,
            normal,
        }
    }

    /// Generates standard 4 vertices for a unit cube [0, 1]^3 for the given face direction.
    /// Aligns strictly with Minecraft 1.21+ FaceInfo winding convention:
    /// - Down (-Y):  (0,0,1), (0,0,0), (1,0,0), (1,0,1)
    /// - Up (+Y):    (0,1,0), (0,1,1), (1,1,1), (1,1,0)
    /// - North (-Z): (1,1,0), (1,0,0), (0,0,0), (0,1,0)
    /// - South (+Z): (0,1,1), (0,0,1), (1,0,1), (1,1,1)
    /// - West (-X):  (0,1,0), (0,0,0), (0,0,1), (0,1,1)
    /// - East (+X):  (1,1,1), (1,0,1), (1,0,0), (1,1,0)
    pub fn unit_cube_face(dir: Direction) -> Self {
        let (vertices, normal) = match dir {
            Direction::Down => (
                [
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 1.0),
                ],
                Vec3::new(0.0, -1.0, 0.0),
            ),
            Direction::Up => (
                [
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ],
                Vec3::new(0.0, 1.0, 0.0),
            ),
            Direction::North => (
                [
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                Vec3::new(0.0, 0.0, -1.0),
            ),
            Direction::South => (
                [
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ],
                Vec3::new(0.0, 0.0, 1.0),
            ),
            Direction::West => (
                [
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(0.0, 1.0, 1.0),
                ],
                Vec3::new(-1.0, 0.0, 0.0),
            ),
            Direction::East => (
                [
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ],
                Vec3::new(1.0, 0.0, 0.0),
            ),
        };

        // Standard canonical UVs [0..1]
        let uvs = [
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
        ];

        Self {
            vertices,
            uvs,
            normal,
        }
    }
}

/// Transforms a 3D vertex from Minecraft world coordinates (+X East, +Y Up, +Z South)
/// to standard Z-up right-hand coordinates (+X East, +Y North, +Z Up).
#[inline]
pub fn mc_world_to_z_up(wx: f32, wy: f32, wz: f32) -> Vec3 {
    Vec3::new(wx, -wz, wy)
}

/// Derives the discrete 3D cardinal tangent directions `(up, left)` from a quad's 3D positions and 2D UVs.
/// Supports both standard 3D UVs (V=0 at bottom, V=1 at top) and Minecraft FaceBakery UVs (V=0 at top, V=1 at bottom) via `is_minecraft_uv`.
pub fn derive_quad_face_tangents(
    positions: &[Vec3; 4],
    uvs: &[Vec2; 4],
    fallback_dir: Direction,
    is_minecraft_uv: bool,
) -> (Direction, Direction) {
    let e1 = positions[1] - positions[0];
    let e2 = positions[2] - positions[0];

    let du1 = uvs[1].x - uvs[0].x;
    let dv1 = uvs[1].y - uvs[0].y;
    let du2 = uvs[2].x - uvs[0].x;
    let dv2 = uvs[2].y - uvs[0].y;

    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-6 {
        return fallback_dir.canonical_face_tangents();
    }

    let inv_det = 1.0 / det;
    // Tangent vector T aligns with +U (texture right)
    let t = (e1 * dv2 - e2 * dv1) * inv_det;
    // Bitangent vector B aligns with +V
    let b = (-e1 * du2 + e2 * du1) * inv_det;

    // Vector pointing towards texture 'up' (top of texture image)
    let up_vec = if is_minecraft_uv { -b } else { b };
    // Vector pointing towards texture 'left' (opposite of right)
    let left_vec = -t;

    let project_to_direction = |v: Vec3, default_dir: Direction| -> Direction {
        let v_norm = v.normalize_or_zero();
        if v_norm.length_squared() < 1e-4 {
            return default_dir;
        }
        let mut best_dir = default_dir;
        let mut max_dot = -f32::INFINITY;
        for d in Direction::ALL {
            let dot = d.normal().dot(v_norm);
            if dot > max_dot {
                max_dot = dot;
                best_dir = d;
            }
        }
        best_dir
    };

    let (canon_up, canon_left) = fallback_dir.canonical_face_tangents();
    let up_dir = project_to_direction(up_vec, canon_up);
    let left_dir = project_to_direction(left_vec, canon_left);

    (up_dir, left_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aabb2d_intersection() {
        let r1 = Aabb2d::from_min_max(0.0, 0.0, 1.0, 1.0);
        let r2 = Aabb2d::from_min_max(0.5, 0.5, 1.5, 1.5);
        let inter = r1.intersection(&r2).unwrap();
        assert_eq!(inter.min, Vec2::new(0.5, 0.5));
        assert_eq!(inter.max, Vec2::new(1.0, 1.0));
        assert_eq!(inter.area(), 0.25);
    }

    #[test]
    fn test_unit_cube_normals() {
        for dir in Direction::ALL {
            let quad = Quad::unit_cube_face(dir);
            assert!(
                quad.normal.dot(dir.normal()) > 0.99,
                "Face normal mismatch for {:?}",
                dir
            );
        }
    }

    #[test]
    fn test_mc_to_z_up_transforms() {
        let w = mc_world_to_z_up(10.0, 64.0, -20.0);
        assert_eq!(w, Vec3::new(10.0, 20.0, 64.0));
    }

    #[test]
    fn test_derive_quad_face_tangents() {
        for dir in Direction::ALL {
            let quad = Quad::unit_cube_face(dir);
            let (up, left) = derive_quad_face_tangents(&quad.vertices, &quad.uvs, dir, false);
            assert_eq!(
                (up, left),
                dir.canonical_face_tangents(),
                "Derived face tangents mismatch for {:?}",
                dir
            );
        }
    }
}
