//! # Canonical Minecraft Fluid Face Culling
//!
//! Fluid geometry is not a full cube: a still source surface sits at `8/9`, and flowing
//! fluids are lower still. Reusing the generic full-cube face culling state machine for
//! fluids therefore culls top faces that are physically separated from the block above.
//!
//! This module mirrors vanilla `FluidRenderer` (26.2) face visibility:
//!
//! - `renderUp` only requires that the fluid above is a different fluid.
//! - A full block above the fluid occludes the top face **only when the fluid face is at
//!   `y = 1.0`** (`isFaceOccludedByState`).
//! - Horizontal faces use the maximum corner height of that side; the bottom face uses
//!   the canonical `8/9` height, exactly like vanilla.
//! - `skipRendering` is not consulted by the fluid renderer. Only same-fluid neighbors
//!   suppress fluid-fluid boundaries.
//!
//! Callers should pass the actual face height (corner average) for `UP` and horizontal
//! directions. The bottom face height is fixed by vanilla and ignored internally.

use mtk_core::constants::fluid::MAX_FLUID_HEIGHT;
use mtk_core::constants::geometry::EPS;
use mtk_core::Aabb2d;
use mtk_core::Direction;

use crate::geometry::rect_ops::is_face_completely_occluded;
use crate::types::{BlockCullMeta, CullCategory, FULL_FACE_RECT};

/// Returns the fluid group (`"water"` / `"lava"`) represented by `meta`.
///
/// Waterlogged non-fluid blocks also represent water fluid for the purpose of
/// vanilla `FluidState.getType().isSame(...)` checks.
#[inline]
pub fn fluid_group(meta: &BlockCullMeta) -> Option<&str> {
    if meta.category == CullCategory::Fluid {
        Some(meta.cull_group.as_str())
    } else if meta.is_waterlogged {
        Some("water")
    } else {
        None
    }
}

/// Returns `true` when both metas resolve to the same vanilla fluid type.
#[inline]
pub fn is_same_fluid(a: &BlockCullMeta, b: &BlockCullMeta) -> bool {
    match (fluid_group(a), fluid_group(b)) {
        (Some(group_a), Some(group_b)) => group_a == group_b,
        _ => false,
    }
}

/// Vanilla `FluidRenderer.isFaceOccludedByState`.
///
/// `height` is the fluid surface height used for this face. The occlusion shape is the
/// neighbor's face pointing back at the fluid. A full face shape occludes every side and
/// the bottom; the top is occluded only when the fluid surface reaches `y = 1.0`.
pub fn is_face_occluded_by_state(
    direction: Direction,
    height: f32,
    neighbor: &BlockCullMeta,
) -> bool {
    if neighbor.is_air {
        return false;
    }

    // A fluid surface below the block boundary can never touch the block above.
    if direction == Direction::Up && height < 1.0 - EPS {
        return false;
    }

    let opposite = direction.opposite();
    if neighbor.has_full_face(opposite) {
        return direction != Direction::Up || height >= 1.0 - EPS;
    }

    let shapes = neighbor.get_face_shapes(opposite);
    if shapes.is_empty() || neighbor.has_empty_face(opposite) {
        return false;
    }

    // The fluid face on the shared plane. `UP`/`DOWN` faces span the full horizontal
    // footprint; horizontal faces span full width but only `[0, height]` vertically.
    let fluid_face = match direction {
        Direction::Up | Direction::Down => FULL_FACE_RECT,
        Direction::East | Direction::West | Direction::North | Direction::South => {
            let clamped = height.clamp(0.0, 1.0);
            Aabb2d::from_min_max(0.0, 0.0, 1.0, clamped)
        }
    };

    is_face_completely_occluded(&[fluid_face], shapes)
}

/// Vanilla `FluidRenderer.isFaceOccludedBySelf`.
///
/// Fluid blocks themselves have an empty occlusion shape. Waterlogged blocks use the
/// host block's shape, so this only matters for the solid half of a waterlogged state.
fn is_face_occluded_by_self(self_meta: Option<&BlockCullMeta>, direction: Direction) -> bool {
    let Some(meta) = self_meta else {
        return false;
    };
    if meta.is_air || meta.is_fluid {
        return false;
    }
    is_face_occluded_by_state(direction.opposite(), 1.0, meta)
}

/// Canonical vanilla fluid face visibility.
///
/// - `fluid_meta` describes the fluid being meshed (water/lava/waterlogged fluid state).
/// - `self_meta` is the host block state at the same position (relevant for waterlogged
///   blocks); `None` for plain water/lava.
/// - `neighbor_meta` is the block state in `direction`.
/// - `face_height` is the corner height used by vanilla: minimum of the four corners for
///   `UP`, maximum of the two side corners for horizontal faces. It is ignored for `DOWN`.
///
/// Returns `true` when the face should be emitted.
pub fn should_render_fluid_face(
    fluid_meta: &BlockCullMeta,
    self_meta: Option<&BlockCullMeta>,
    neighbor_meta: Option<&BlockCullMeta>,
    direction: Direction,
    face_height: f32,
) -> bool {
    if fluid_meta.is_air {
        return false;
    }

    let neighbor = neighbor_meta.filter(|meta| !meta.is_air);

    // 1. Vanilla `isNeighborSameFluid`: fluid-fluid boundaries never render.
    if let Some(neighbor) = neighbor {
        if is_same_fluid(fluid_meta, neighbor) {
            return false;
        }
    }

    // 2. Vanilla `shouldRenderFace` excludes self-occluded faces. The top face path in
    //    vanilla does not consult self occlusion.
    if direction != Direction::Up && is_face_occluded_by_self(self_meta, direction) {
        return false;
    }

    // 3. Vanilla `isFaceOccludedByNeighbor`: full blocks occlude horizontal/bottom faces,
    //    but only occlude the fluid top when the surface is full-height.
    let Some(neighbor) = neighbor else {
        return true;
    };

    let height = match direction {
        Direction::Down => MAX_FLUID_HEIGHT,
        _ => face_height,
    };

    !is_face_occluded_by_state(direction, height, neighbor)
}
