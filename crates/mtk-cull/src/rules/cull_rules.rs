use glam::IVec3;
use mtk_core::Direction;

use crate::types::{BlockCullMeta, CullCategory, GlassCullMode, LeavesCullMode};

/// Custom face skipping rules for specialized Minecraft blocks.
/// Replicates `BlockBehaviour.skipRendering` and its overrides in vanilla Minecraft.
///
/// Fluid **rendering** visibility is height-aware and lives in
/// [`crate::rules::fluid_cull`]; this function retains vanilla `skipRendering` semantics
/// for direct callers and non-fluid block paths.
///
/// Returns `true` if this face should be skipped (culled), `false` otherwise.
pub(crate) fn should_skip_rendering(
    state_meta: &BlockCullMeta,
    neighbor_meta: &BlockCullMeta,
    direction: Direction,
    leaves_mode: LeavesCullMode,
    glass_mode: GlassCullMode,
    block_pos: Option<IVec3>,
    neighbor_pos: Option<IVec3>,
) -> bool {
    if state_meta.is_air || neighbor_meta.is_air {
        return false;
    }

    let cat_a = state_meta.category;
    let cat_b = neighbor_meta.category;

    // 1. Glass & Translucent blocks (HalfTransparentBlock / TintedGlassBlock / Ice / Slime / Honey)
    if cat_a == CullCategory::GlassTranslucent && cat_b == CullCategory::GlassTranslucent {
        match glass_mode {
            GlassCullMode::Group => {
                if state_meta.cull_group == neighbor_meta.cull_group {
                    return true;
                }
                if state_meta.cull_group.starts_with("glass")
                    && neighbor_meta.cull_group.starts_with("glass")
                {
                    return true;
                }
            }
            GlassCullMode::SameBlock => {
                if state_meta.block_name == neighbor_meta.block_name {
                    return true;
                }
            }
            GlassCullMode::None => {}
        }
        return false;
    }

    // 2. Cutout Leaves & Foliage (LeavesBlock)
    if cat_a == CullCategory::CutoutLeaves && cat_b == CullCategory::CutoutLeaves {
        match leaves_mode {
            LeavesCullMode::Fast => {
                return true;
            }
            LeavesCullMode::SingleFace => {
                if let (Some(pos_a), Some(pos_b)) = (block_pos, neighbor_pos) {
                    // Deterministic canonical ordering (render face only from smaller pos coordinate)
                    return (pos_a.x, pos_a.y, pos_a.z) > (pos_b.x, pos_b.y, pos_b.z);
                } else {
                    // Directional tie-breaker
                    return matches!(
                        direction,
                        Direction::East | Direction::Up | Direction::South
                    );
                }
            }
            LeavesCullMode::Fancy | LeavesCullMode::None => {
                return false;
            }
        }
    }

    // 3. Fluids (LiquidBlock: Water / Lava)
    if cat_a == CullCategory::Fluid {
        if cat_b == CullCategory::Fluid {
            // Same fluid type (water vs water, lava vs lava) culls mutual boundary
            if state_meta.cull_group == neighbor_meta.cull_group {
                return true;
            }
        } else if state_meta.cull_group == "water" && neighbor_meta.is_waterlogged {
            // Water against waterlogged block culls face
            return true;
        }
        return false;
    }

    // 4. Mangrove Roots (MangroveRootsBlock)
    if state_meta.block_name == "mangrove_roots" && neighbor_meta.block_name == "mangrove_roots" {
        // Mangrove roots only cull in vertical Y axis
        if matches!(direction, Direction::Up | Direction::Down) {
            return true;
        }
    }

    // 5. Powder Snow (PowderSnowBlock)
    state_meta.block_name == "powder_snow" && neighbor_meta.block_name == "powder_snow"
}
