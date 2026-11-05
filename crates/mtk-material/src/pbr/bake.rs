//! # Baked Atlas PBR Decoding
//!
//! Applies a source-format decoder to every stitched `BakedAtlasChunk` and updates the
//! address-map metadata (`has_orm`, `has_aux`, `pbr`). This is the orchestration seam used
//! by `libmtk::prebake`; `mtk-texture` itself stays free of material semantics.

use mtk_texture::BakedAtlas;

use crate::pbr::labpbr::decode_labpbr13;

/// Summary of a [`decode_baked_atlas`] pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PbrDecodeSummary {
    pub chunks: usize,
    pub with_normal: usize,
    pub with_orm: usize,
    pub with_aux: usize,
}

/// Decode raw LabPBR companions on every chunk into `MtkPackedV1` buffers.
///
/// Raw `normal` / `specular` buffers are dropped after decoding to keep peak memory
/// bounded. The function is idempotent: chunks that already have decoded layers are skipped.
pub fn decode_baked_atlas(
    atlas: &mut BakedAtlas,
) -> Result<PbrDecodeSummary, crate::MaterialError> {
    let mut summary = PbrDecodeSummary::default();

    // Index chunk metadata by chunk ID to avoid O(n^2) linear scans during iteration
    let mut meta_by_id: std::collections::HashMap<u16, &mut mtk_texture::AtlasChunkMeta> = atlas
        .address_map
        .chunks
        .iter_mut()
        .map(|m| (m.chunk_id, m))
        .collect();

    for chunk in &mut atlas.chunks {
        // Legacy decoder behavior: transparent albedo pixels use white RGB so
        // bilinear filtering at cutout edges does not produce dark fringes.
        clean_transparent_albedo(&mut chunk.albedo);

        // If legacy layers exist, decode them to extract any missing canonical layers.
        // Canonical layers take strict precedence; legacy complements missing channels.
        if chunk.normal.is_some() || chunk.specular.is_some() {
            let decoded = decode_labpbr13(chunk.normal.as_ref(), chunk.specular.as_ref())?;
            if chunk.normal_gl.is_none() {
                chunk.normal_gl = decoded.normal_gl;
            }
            if chunk.orm.is_none() {
                chunk.orm = decoded.orm;
            }
            if chunk.aux.is_none() {
                chunk.aux = decoded.aux;
            }
            chunk.normal = None;
            chunk.specular = None;
        }

        let has_normal = chunk.normal_gl.is_some();
        let has_orm = chunk.orm.is_some();
        let has_aux = chunk.aux.is_some();
        let is_empty = !has_normal && !has_orm && !has_aux;

        if let Some(chunk_meta) = meta_by_id.get_mut(&chunk.chunk_id) {
            sync_chunk_meta(
                chunk_meta,
                has_normal,
                has_orm,
                has_aux,
                if is_empty {
                    None
                } else {
                    Some(mtk_texture::PbrMeta::labpbr13())
                },
            );
        }

        summary.chunks += 1;
        summary.with_normal += usize::from(has_normal);
        summary.with_orm += usize::from(has_orm);
        summary.with_aux += usize::from(has_aux);
    }

    Ok(summary)
}

fn sync_chunk_meta(
    chunk_meta: &mut mtk_texture::AtlasChunkMeta,
    has_normal: bool,
    has_orm: bool,
    has_aux: bool,
    pbr_meta: Option<mtk_texture::PbrMeta>,
) {
    chunk_meta.has_normal = has_normal;
    chunk_meta.has_specular = false;
    chunk_meta.has_orm = has_orm;
    chunk_meta.has_aux = has_aux;
    chunk_meta.pbr = pbr_meta;
    chunk_meta.refresh_textures();
}

fn clean_transparent_albedo(buffer: &mut mtk_texture::RgbaBuffer) {
    for pixel in buffer.pixels.as_chunks_mut::<4>().0 {
        if pixel[3] == 0 {
            pixel[0] = 255;
            pixel[1] = 255;
            pixel[2] = 255;
        }
    }
}
