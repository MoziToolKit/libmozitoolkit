//! Shared helpers for `mtk-voxel` integration tests.
//!
//! Source-texture provenance is emitted strictly as the canonical
//! `mtk_source_textures` (Mesh string palette) + `mtk_source_texture_idx`
//! (Face Int32) pair. These helpers expand that pair back into per-face keys so
//! tests can assert on face order without depending on a retired attribute.

#![allow(dead_code)]

use mtk_core::AttributeData;
use mtk_core::MeshData;

/// Palette of the canonical source-texture provenance, or `None` when absent.
pub fn face_texture_palette(mesh: &MeshData) -> Option<&[String]> {
    match &mesh.get_custom_attribute("mtk_source_textures")?.data {
        AttributeData::String(palette) => Some(palette.as_slice()),
        _ => None,
    }
}

/// Per-face palette indices of the canonical source-texture provenance.
pub fn face_texture_indices(mesh: &MeshData) -> Option<&[i32]> {
    match &mesh.get_custom_attribute("mtk_source_texture_idx")?.data {
        AttributeData::Int32(indices) => Some(indices.as_slice()),
        _ => None,
    }
}

/// Expands the canonical provenance pair into one texture key per face.
///
/// Panics when the provenance pair is missing or malformed: every mesher output
/// that carries faces must expose both halves of the contract.
pub fn face_texture_keys(mesh: &MeshData) -> Vec<String> {
    let palette = face_texture_palette(mesh).expect("mesh must expose mtk_source_textures palette");
    let indices = face_texture_indices(mesh).expect("mesh must expose mtk_source_texture_idx");
    indices
        .iter()
        .map(|&idx| {
            palette
                .get(idx as usize)
                .unwrap_or_else(|| {
                    panic!(
                        "palette index {idx} out of range ({} entries)",
                        palette.len()
                    )
                })
                .clone()
        })
        .collect()
}
