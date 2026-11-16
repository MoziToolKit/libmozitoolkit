//! # Biome Tint Resolver for Meshing
//!
//! Computes per-face Biome Tint data and resolved linear colors.

/// Computes biome tint data and linear color for a face, delegating to `mtk_material::compute_face_tint`.
pub(crate) fn compute_face_tint(
    texture_key: &str,
    block_name: &str,
    tint_index: i16,
    biome_resolver: Option<&mtk_material::BiomeResolver>,
    colormaps: Option<&mtk_material::VanillaColormaps>,
) -> ([f32; 4], [f32; 4]) {
    mtk_material::compute_face_tint(
        texture_key,
        Some(block_name),
        tint_index,
        biome_resolver,
        colormaps,
    )
}
