use alloc::string::{String, ToString};
use alloc::sync::Arc;
use std::collections::HashMap;

use mtk_core::constants::fluid::MAX_FLUID_HEIGHT;
use mtk_core::Aabb2d;
use mtk_core::Direction;
use mtk_core::{IVec3, Vec3};

use crate::geometry::rect_ops::is_face_completely_occluded;
use crate::rules::should_skip_rendering;
use crate::types::{BlockCullMeta, CullCategory, GlassCullMode, LeavesCullMode, FULL_FACE_RECT};

pub use crate::rules::categories::*;

/// High-Performance Unified Face Culling Engine.
#[derive(Debug)]
pub struct FaceCuller {
    pub leaves_cull_mode: LeavesCullMode,
    pub glass_cull_mode: GlassCullMode,
    meta_cache: std::sync::Arc<std::sync::RwLock<HashMap<String, Arc<BlockCullMeta>>>>,
}

impl Clone for FaceCuller {
    fn clone(&self) -> Self {
        Self {
            leaves_cull_mode: self.leaves_cull_mode,
            glass_cull_mode: self.glass_cull_mode,
            meta_cache: self.meta_cache.clone(),
        }
    }
}

impl Default for FaceCuller {
    fn default() -> Self {
        Self::new(LeavesCullMode::SingleFace, GlassCullMode::Group)
    }
}

impl FaceCuller {
    /// Maximum capacity of the block cull meta cache.
    pub const CACHE_CAPACITY: usize = 32_768;
    /// Creates a new `FaceCuller` engine with custom leaves and glass modes.
    pub fn new(leaves_cull_mode: LeavesCullMode, glass_cull_mode: GlassCullMode) -> Self {
        Self {
            leaves_cull_mode,
            glass_cull_mode,
            meta_cache: std::sync::Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Clears the cached block culling metadata.
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.meta_cache.write() {
            cache.clear();
        }
    }

    /// Number of cached metadata entries.
    pub fn cache_len(&self) -> usize {
        self.meta_cache.read().map(|g| g.len()).unwrap_or(0)
    }

    /// Retrieves or computes `BlockCullMeta` for a given blockstate string.
    pub fn get_meta(
        &self,
        state_str: &str,
        element_quads: Option<&[([Vec3; 4], Direction)]>,
        is_opaque_hint: Option<bool>,
    ) -> Arc<BlockCullMeta> {
        if let Ok(cache) = self.meta_cache.read() {
            if let Some(meta) = cache.get(state_str) {
                if element_quads.is_some() && !meta.has_baked_model {
                    // Needs re-bake with detailed quads
                } else {
                    return Arc::clone(meta);
                }
            }
        }

        let meta = Arc::new(compute_block_cull_meta(
            state_str,
            element_quads,
            is_opaque_hint,
        ));

        if let Ok(mut cache) = self.meta_cache.write() {
            if cache.len() >= Self::CACHE_CAPACITY && !cache.contains_key(state_str) {
                if let Some(first_key) = cache.keys().next().cloned() {
                    cache.remove(&first_key);
                }
            }
            cache.insert(state_str.to_string(), Arc::clone(&meta));
        }
        meta
    }

    /// Evaluates canonical Minecraft fluid face visibility.
    ///
    /// Fluids are not full cubes, so their height-aware occlusion rules cannot be
    /// represented by [`BlockCullMeta`] alone. Callers that have corner heights should
    /// use this method instead of [`Self::should_render_face`].
    ///
    /// Returns `true` if this fluid face SHOULD be rendered, `false` if culled.
    pub fn should_render_fluid_face(
        &self,
        fluid_meta: &BlockCullMeta,
        self_meta: Option<&BlockCullMeta>,
        neighbor_meta: Option<&BlockCullMeta>,
        direction: Direction,
        face_height: f32,
    ) -> bool {
        crate::rules::should_render_fluid_face(
            fluid_meta,
            self_meta,
            neighbor_meta,
            direction,
            face_height,
        )
    }

    /// Evaluates Minecraft 1.21+ canonical face visibility test.
    ///
    /// Returns `true` if this face SHOULD be rendered, `false` if culled.
    pub fn should_render_face(
        &self,
        state_meta: &BlockCullMeta,
        neighbor_meta: Option<&BlockCullMeta>,
        direction: Direction,
        quad_face_shape: Option<&[Aabb2d]>,
        block_pos: Option<IVec3>,
        neighbor_pos: Option<IVec3>,
    ) -> bool {
        if state_meta.is_air {
            return false;
        }

        // Fluids have height-dependent face visibility. Without corner heights this
        // conservative fallback assumes the canonical still-fluid surface (8/9), which
        // keeps a top face under a ceiling visible while still culling full-block sides
        // and bottoms. The voxel mesher calls `should_render_fluid_face` with exact
        // per-corner heights.
        if state_meta.category == CullCategory::Fluid {
            return crate::rules::should_render_fluid_face(
                state_meta,
                None,
                neighbor_meta,
                direction,
                MAX_FLUID_HEIGHT,
            );
        }

        let neighbor_meta = match neighbor_meta {
            Some(n) if !n.is_air => n,
            _ => return true,
        };

        let opp_dir = direction.opposite();

        // 1. Solid / glass blocks must never have external faces culled by adjacent non-full / partial blocks
        let is_snow_cover = direction == Direction::Up
            && (neighbor_meta.block_name == "snow" || neighbor_meta.block_name.ends_with(":snow"))
            && neighbor_meta.has_full_face(Direction::Down);

        if (state_meta.category == CullCategory::SolidOpaque
            || state_meta.category == CullCategory::GlassTranslucent)
            && !is_snow_cover
            && (neighbor_meta.category == CullCategory::NonOccluding
                || (neighbor_meta.category == CullCategory::PartialShape
                    && !(neighbor_meta.block_name.ends_with("_slab")
                        || neighbor_meta.block_name.ends_with("_stairs"))))
        {
            return true;
        }

        // 2. Neighbor full solid face check (neighborFaceShape == Shapes.block())
        if neighbor_meta.has_full_face(opp_dir) {
            return false;
        }

        // 3. Custom skipRendering check (glass, leaves, fluid, snow, roots)
        if should_skip_rendering(
            state_meta,
            neighbor_meta,
            direction,
            self.leaves_cull_mode,
            self.glass_cull_mode,
            block_pos,
            neighbor_pos,
        ) {
            return false;
        }

        // 4. Neighbor empty face check (neighborFaceShape == Shapes.empty())
        if neighbor_meta.has_empty_face(opp_dir) {
            return true;
        }

        // 5. State empty face check (stateFaceShape == Shapes.empty())
        if quad_face_shape.is_none() && state_meta.has_empty_face(direction) {
            return true;
        }

        // 6. 2D Boolean shape occlusion check
        let default_quad_shape = [FULL_FACE_RECT];
        let target_shapes: &[Aabb2d] = if let Some(shapes) = quad_face_shape {
            shapes
        } else {
            let s = state_meta.get_face_shapes(direction);
            if s.is_empty() {
                &default_quad_shape
            } else {
                s
            }
        };
        let neighbor_shapes = neighbor_meta.get_face_shapes(opp_dir);

        !is_face_completely_occluded(target_shapes, neighbor_shapes)
    }
}
