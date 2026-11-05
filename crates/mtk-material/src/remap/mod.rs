mod batch;
mod routes;

pub use batch::remap_mesh_multi_uvs_paletted_with_options;
pub use routes::{reroute_mesh_animation, resolve_mesh_material_routes};

use mtk_texture::AtlasSpriteLocation;

/// Project a local [0.0..1.0] (or continuous tiled) UV coordinate into a target AtlasSprite's Frame 0 bounds.
#[inline]
pub fn remap_local_to_atlas(u: f32, v: f32, sprite: &AtlasSpriteLocation) -> [f32; 2] {
    let [u_min, v_min, u_max, v_max] = sprite.frame_0_uv_bounds;
    let width = u_max - u_min;
    let height = v_max - v_min;

    let target_u = u_min + u * width;
    let target_v = v_min + v * height;

    [target_u, target_v]
}

/// Invert an incoming Atlas UV coordinate back to local [0.0..1.0] space.
#[inline]
pub(crate) fn remap_atlas_to_local(u: f32, v: f32, sprite: &AtlasSpriteLocation) -> [f32; 2] {
    let [u_min, v_min, u_max, v_max] = sprite.frame_0_uv_bounds;
    let width = (u_max - u_min).max(1e-7);
    let height = (v_max - v_min).max(1e-7);

    let local_u = (u - u_min) / width;
    let local_v = (v - v_min) / height;

    [local_u, local_v]
}

/// Invert a UV coordinate from a source sprite and project it into a target sprite.
#[inline]
pub(crate) fn remap_sprite_to_sprite(
    u: f32,
    v: f32,
    source_sprite: &AtlasSpriteLocation,
    target_sprite: &AtlasSpriteLocation,
) -> [f32; 2] {
    let local = remap_atlas_to_local(u, v, source_sprite);
    remap_local_to_atlas(local[0], local[1], target_sprite)
}

/// Calculate the Euler Z rotation angle (in radians) of a face's loop UVs.
/// Returns 0.0 for unrotated or standard axis-aligned faces.
/// Returns non-zero angle theta in radians only when ALL valid edges of the face are non-orthogonal.
///
/// Delegates to [`mtk_core::detect_uv_rotation`] so the UV angle math has a single source of truth.
pub(crate) fn detect_face_uv_rotation(uvs: &[[f32; 2]], tolerance: f32) -> f32 {
    let vecs: Vec<glam::Vec2> = uvs.iter().map(|p| glam::Vec2::new(p[0], p[1])).collect();
    mtk_core::detect_uv_rotation(&vecs, tolerance)
}

/// Straighten a rotated polygon's loop UVs back to standard axis-aligned coordinates.
pub(crate) fn straighten_face_uv(uvs: &mut [[f32; 2]], angle: f32) -> bool {
    if angle.abs() < 1e-4 || uvs.len() < 3 {
        return false;
    }

    let n = uvs.len() as f32;
    let center_u: f32 = uvs.iter().map(|p| p[0]).sum::<f32>() / n;
    let center_v: f32 = uvs.iter().map(|p| p[1]).sum::<f32>() / n;

    let cos_t = (-angle).cos();
    let sin_t = (-angle).sin();

    for p in uvs.iter_mut() {
        let dx = p[0] - center_u;
        let dy = p[1] - center_v;
        p[0] = center_u + (dx * cos_t - dy * sin_t);
        p[1] = center_v + (dx * sin_t + dy * cos_t);
    }

    true
}

/// Check if a polygon's UV coordinates escape the canonical [0, 1] unit square.
#[inline]
pub(crate) fn face_uv_requires_atlas_tiling(uvs: &[[f32; 2]], epsilon: f32) -> bool {
    if uvs.is_empty() {
        return false;
    }
    for p in uvs.iter() {
        if p[0] < -epsilon || p[0] > 1.0 + epsilon || p[1] < -epsilon || p[1] > 1.0 + epsilon {
            return true;
        }
    }
    false
}

/// Normalize one face's local UV coordinates to [0, 1] and calculate Mapping inputs for MC_Atlas_UV_Tiling.
/// Returns ([scale_u, scale_v, location_u, location_v], is_tiled).
pub(crate) fn normalize_face_uv_for_atlas_tiling(
    uvs: &mut [[f32; 2]],
    epsilon: f32,
) -> ([f32; 4], bool) {
    if uvs.is_empty() || !face_uv_requires_atlas_tiling(uvs, 1e-4) {
        return ([1.0, 1.0, 0.0, 0.0], false);
    }

    let mut min_u = f32::INFINITY;
    let mut max_u = f32::NEG_INFINITY;
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;

    for p in uvs.iter() {
        if p[0] < min_u {
            min_u = p[0];
        }
        if p[0] > max_u {
            max_u = p[0];
        }
        if p[1] < min_v {
            min_v = p[1];
        }
        if p[1] > max_v {
            max_v = p[1];
        }
    }

    let span_u = max_u - min_u;
    let span_v = max_v - min_v;

    let safe_span_u = if span_u > epsilon { span_u } else { 1.0 };
    let safe_span_v = if span_v > epsilon { span_v } else { 1.0 };

    for p in uvs.iter_mut() {
        p[0] = if span_u > epsilon {
            (p[0] - min_u) / safe_span_u
        } else {
            0.0
        };
        p[1] = if span_v > epsilon {
            (p[1] - min_v) / safe_span_v
        } else {
            0.0
        };
    }

    // MC_Atlas_UV_Tiling subtracts/adds 0.5 around the Mapping node, hence
    // location produces min + span * normalized_uv exactly.
    let scale_u = safe_span_u;
    let scale_v = safe_span_v;
    let loc_u = min_u + (safe_span_u - 1.0) * 0.5;
    let loc_v = min_v + (safe_span_v - 1.0) * 0.5;

    ([scale_u, scale_v, loc_u, loc_v], true)
}
