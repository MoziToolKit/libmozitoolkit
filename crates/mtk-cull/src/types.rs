use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use mtk_core::Aabb2d;
use mtk_core::{DirMask, Direction};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// High-level categorization of blocks for Minecraft face culling behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CullCategory {
    /// Standard full solid cubes (stone, dirt, planks, cobble, etc.).
    #[default]
    SolidOpaque,
    /// Glass, Stained Glass, Ice, Slime, Honey, Tinted Glass, Powder Snow.
    GlassTranslucent,
    /// Cutout Leaves (Oak, Birch, etc.), Mangrove Roots.
    CutoutLeaves,
    /// Non-full blocks (Slabs, Stairs, Trapdoors, Snow, Farmland, Path).
    PartialShape,
    /// Fluid blocks (Water, Lava, Flowing fluids).
    Fluid,
    /// Non-occluding decoration/vegetation (Flowers, Torches, Saplings, Rails, Web, Signs).
    NonOccluding,
    /// Air blocks (minecraft:air, cave_air, void_air, structure_void).
    Air,
}

/// Rendering and culling modes for cutout foliage / leaves blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LeavesCullMode {
    /// Vanilla Fancy: both adjacent leaves faces rendered (internal volume visible).
    Fancy,
    /// Optimized Single-Face: exactly one face rendered between touching leaves (no z-fighting).
    #[default]
    SingleFace,
    /// Vanilla Fast: mutual culling between touching leaves (opaque outer shell).
    Fast,
    /// No leaves culling at all (renders all faces).
    None,
}

/// Culling modes for glass and stained glass blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GlassCullMode {
    /// Strictly same block type culls (e.g. red glass against red glass only).
    SameBlock,
    /// Any glass culls against any glass (plain + all 16 stained colors).
    #[default]
    Group,
    /// No glass culling (renders internal partition faces).
    None,
}

pub use mtk_core::constants::geometry::{EMPTY_FACE_RECT, FULL_FACE_RECT};

/// Check if a rectangle covers the full 1.0 x 1.0 face within epsilon.
#[inline]
pub fn is_full_rect(rect: &Aabb2d, eps: f32) -> bool {
    rect.min.x <= eps && rect.min.y <= eps && rect.max.x >= 1.0 - eps && rect.max.y >= 1.0 - eps
}

/// Check if a rectangle has zero area within epsilon.
#[inline]
pub fn is_empty_rect(rect: &Aabb2d, eps: f32) -> bool {
    (rect.max.x - rect.min.x) <= eps || (rect.max.y - rect.min.y) <= eps
}

/// Precomputed and cached culling metadata for a single unique BlockState string.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BlockCullMeta {
    /// Full block state string identifier (e.g. "minecraft:oak_slab[type=bottom]").
    pub state_str: String,
    /// Stripped block identifier without namespace/props (e.g. "oak_slab").
    pub block_name: String,
    /// Category classification for culling rules.
    pub category: CullCategory,
    /// Whether this block represents a geometric 1x1x1 full cube.
    pub is_full_cube: bool,
    /// Whether this block is fully opaque.
    pub is_opaque: bool,
    /// Whether this block is air or void.
    pub is_air: bool,
    /// Whether this block is a fluid.
    pub is_fluid: bool,
    /// Culling group identifier for group-level mutual culling (e.g. "glass", "water", "solid").
    pub cull_group: String,
    /// 2D boundary face occlusion shapes for each of the 6 cardinal directions.
    pub face_shapes: [Vec<Aabb2d>; 6],
    /// Bitmask indicating which directions have at least one full solid face.
    pub full_face_mask: DirMask,
    /// Bitmask indicating which directions have an empty occlusion shape.
    pub empty_face_mask: DirMask,
    /// Parsed blockstate properties map.
    pub props: BTreeMap<String, String>,
    /// Whether this block is submerged/waterlogged.
    pub is_waterlogged: bool,
    /// Whether this metadata was computed from detailed baked model geometry.
    pub has_baked_model: bool,
}

impl BlockCullMeta {
    /// Checks if this block state has a full occluding face in the given direction.
    #[inline]
    pub fn has_full_face(&self, dir: Direction) -> bool {
        self.full_face_mask.contains_dir(dir)
    }

    /// Checks if this block state has an empty occlusion shape in the given direction.
    #[inline]
    pub fn has_empty_face(&self, dir: Direction) -> bool {
        self.empty_face_mask.contains_dir(dir)
    }

    /// Returns the slice of 2D occlusion rectangles for a specific direction.
    #[inline]
    pub fn get_face_shapes(&self, dir: Direction) -> &[Aabb2d] {
        &self.face_shapes[dir.to_index()]
    }
}
