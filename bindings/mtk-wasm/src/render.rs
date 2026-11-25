//! Three.js / WebGPU render-ready mesh batching from `MeshData`.
//!
//! Converts `MeshData` (which uses Blender-compatible `PerLoop` UVs/colors and
//! corner AO attributes) into per-atlas-chunk `PerVertex` indexed geometry batches
//! ready for zero-copy `THREE.BufferAttribute` construction.

use std::collections::BTreeMap;
use std::sync::Arc;

use libmtk::PackageManifest;
use mtk_core::attribute_constants::{
    ATTR_ANIM_FRAME_SIZE, ATTR_ANIM_TIMING, ATTR_AO, ATTR_ATLAS_CHUNK_ID, ATTR_BIOME_TINT_COLOR,
    ATTR_BIOME_TINT_DATA,
};
use mtk_core::{AttributeData, MeshData, UvLayout};
use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_resource::CtmSolver;
use mtk_texture::{AtlasAddressMap, AtlasChunkMeta};
use mtk_voxel::{CoordinateSystem, MesherConfig};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct WasmMetadataPayload<'a> {
    manifest: &'a PackageManifest,
    texture_names: &'a [String],
    atlas_chunks: &'a [AtlasChunkMeta],
}

pub(crate) fn serialize_metadata_json(
    manifest: &PackageManifest,
    texture_names: &[String],
    atlas_mapping: Option<&AtlasAddressMap>,
) -> Result<String, JsValue> {
    let atlas_chunks: &[AtlasChunkMeta] = atlas_mapping.map(|m| m.chunks.as_slice()).unwrap_or(&[]);
    let payload = WasmMetadataPayload {
        manifest,
        texture_names,
        atlas_chunks,
    };
    serde_json::to_string(&payload).map_err(|e| JsValue::from_str(&e.to_string()))
}

pub(crate) fn build_mesher_config(
    enable_ao: Option<bool>,
    mesh_fluids: Option<bool>,
    enable_ctm: Option<bool>,
    enable_animated_textures: Option<bool>,
    atlas_address_map: Option<Arc<AtlasAddressMap>>,
    biome_resolver: Option<Arc<BiomeResolver>>,
    colormaps: Option<Arc<VanillaColormaps>>,
    ctm_solver: Option<Arc<CtmSolver>>,
) -> MesherConfig {
    let use_ctm = enable_ctm.unwrap_or(true);
    MesherConfig {
        coordinate_system: CoordinateSystem::YUpRightHanded,
        origin_centered: true,
        weld_vertices: false,
        enable_ao: enable_ao.unwrap_or(true),
        mesh_fluids: mesh_fluids.unwrap_or(true),
        enable_animated_textures: enable_animated_textures.unwrap_or(true),
        atlas_address_map,
        biome_resolver,
        colormaps,
        ctm_solver: if use_ctm { ctm_solver } else { None },
        ..Default::default()
    }
}

/// Single atlas-chunk mesh batch with aligned `PerVertex` buffers for Three.js / WebGPU.
#[wasm_bindgen]
pub struct WasmRenderBatch {
    chunk_id: i32,
    is_animated: bool,
    albedo_chunk_id: Option<String>,
    overlay_chunk_id: Option<String>,
    normal_chunk_id: Option<String>,
    orm_chunk_id: Option<String>,
    aux_chunk_id: Option<String>,
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    /// Linear `[tint_r, tint_g, tint_b, ao]` per vertex.
    colors: Vec<f32>,
    /// `[base_tint_factor, overlay_tint_factor]` per vertex.
    tint_weights: Vec<f32>,
    indices: Vec<u32>,
    anim_timings: Vec<f32>,
    anim_frame_sizes: Vec<f32>,
}

#[wasm_bindgen]
impl WasmRenderBatch {
    #[wasm_bindgen(getter)]
    pub fn chunk_id(&self) -> i32 {
        self.chunk_id
    }

    #[wasm_bindgen(getter)]
    pub fn is_animated(&self) -> bool {
        self.is_animated
    }

    #[wasm_bindgen(getter)]
    pub fn has_animation(&self) -> bool {
        !self.anim_timings.is_empty()
    }

    #[wasm_bindgen(getter)]
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    #[wasm_bindgen(getter)]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    #[wasm_bindgen(getter)]
    pub fn albedo_chunk_id(&self) -> Option<String> {
        self.albedo_chunk_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn overlay_chunk_id(&self) -> Option<String> {
        self.overlay_chunk_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn normal_chunk_id(&self) -> Option<String> {
        self.normal_chunk_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn orm_chunk_id(&self) -> Option<String> {
        self.orm_chunk_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn aux_chunk_id(&self) -> Option<String> {
        self.aux_chunk_id.clone()
    }

    pub fn positions_ptr(&self) -> *const f32 {
        self.positions.as_ptr()
    }

    pub fn positions_len(&self) -> usize {
        self.positions.len()
    }

    pub fn normals_ptr(&self) -> *const f32 {
        self.normals.as_ptr()
    }

    pub fn normals_len(&self) -> usize {
        self.normals.len()
    }

    pub fn uvs_ptr(&self) -> *const f32 {
        self.uvs.as_ptr()
    }

    pub fn uvs_len(&self) -> usize {
        self.uvs.len()
    }

    /// Linear `[tint_r, tint_g, tint_b, ao]` attributes, 4 floats per vertex.
    pub fn colors_ptr(&self) -> *const f32 {
        self.colors.as_ptr()
    }

    pub fn colors_len(&self) -> usize {
        self.colors.len()
    }

    /// `[base_tint_factor, overlay_tint_factor]` weights, 2 floats per vertex.
    pub fn tint_weights_ptr(&self) -> *const f32 {
        self.tint_weights.as_ptr()
    }

    pub fn tint_weights_len(&self) -> usize {
        self.tint_weights.len()
    }

    pub fn indices_ptr(&self) -> *const u32 {
        self.indices.as_ptr()
    }

    pub fn indices_len(&self) -> usize {
        self.indices.len()
    }

    /// Optional per-vertex `[total_frames, frametime, interpolate]` (3 floats per vertex).
    pub fn anim_timings_ptr(&self) -> *const f32 {
        self.anim_timings.as_ptr()
    }

    pub fn anim_timings_len(&self) -> usize {
        self.anim_timings.len()
    }

    /// Optional per-vertex `[frame_width, frame_height, 0]` (3 floats per vertex).
    pub fn anim_frame_sizes_ptr(&self) -> *const f32 {
        self.anim_frame_sizes.as_ptr()
    }

    pub fn anim_frame_sizes_len(&self) -> usize {
        self.anim_frame_sizes.len()
    }
}

/// Collection of per-chunk render batches ready for Three.js consumption.
#[wasm_bindgen]
pub struct WasmRenderScene {
    batches: Vec<Option<WasmRenderBatch>>,
    total_vertices: usize,
    total_triangles: usize,
}

#[wasm_bindgen]
impl WasmRenderScene {
    #[wasm_bindgen(getter)]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    #[wasm_bindgen(getter)]
    pub fn total_vertices(&self) -> usize {
        self.total_vertices
    }

    #[wasm_bindgen(getter)]
    pub fn total_triangles(&self) -> usize {
        self.total_triangles
    }

    /// Moves ownership of batch `index` out to JS without copying geometry buffers.
    pub fn take_batch(&mut self, index: usize) -> Result<WasmRenderBatch, JsValue> {
        let slot = self
            .batches
            .get_mut(index)
            .ok_or_else(|| JsValue::from_str("Render batch index out of bounds"))?;
        slot.take()
            .ok_or_else(|| JsValue::from_str("Render batch already taken"))
    }
}

#[derive(Default)]
struct BatchBuilder {
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    colors: Vec<f32>,
    tint_weights: Vec<f32>,
    indices: Vec<u32>,
    anim_timings: Vec<f32>,
    anim_frame_sizes: Vec<f32>,
    has_animated_faces: bool,
}

impl BatchBuilder {
    #[allow(clippy::too_many_arguments)]
    fn push_vertex(
        &mut self,
        pos: [f32; 3],
        norm: [f32; 3],
        uv: [f32; 2],
        color: [f32; 4],
        tint_weight: [f32; 2],
        anim_timing: Option<[f32; 3]>,
        anim_frame_size: Option<[f32; 3]>,
    ) {
        self.positions.extend_from_slice(&pos);
        self.normals.extend_from_slice(&norm);
        self.uvs.extend_from_slice(&uv);
        self.colors.extend_from_slice(&color);
        self.tint_weights.extend_from_slice(&tint_weight);
        if let Some(timing) = anim_timing {
            if timing[0] > 1.0 {
                self.has_animated_faces = true;
            }
            self.anim_timings.extend_from_slice(&timing);
            self.anim_frame_sizes
                .extend_from_slice(&anim_frame_size.unwrap_or([16.0, 16.0, 0.0]));
        }
    }

    fn finish(self, chunk_id: i32, meta: Option<&AtlasChunkMeta>) -> WasmRenderBatch {
        let (
            albedo_chunk_id,
            overlay_chunk_id,
            normal_chunk_id,
            orm_chunk_id,
            aux_chunk_id,
            meta_animated,
        ) = resolve_chunk_textures(chunk_id, meta);

        let keep_anim = self.has_animated_faces;
        WasmRenderBatch {
            chunk_id,
            is_animated: meta_animated || keep_anim,
            albedo_chunk_id,
            overlay_chunk_id,
            normal_chunk_id,
            orm_chunk_id,
            aux_chunk_id,
            positions: self.positions,
            normals: self.normals,
            uvs: self.uvs,
            colors: self.colors,
            tint_weights: self.tint_weights,
            indices: self.indices,
            anim_timings: if keep_anim {
                self.anim_timings
            } else {
                Vec::new()
            },
            anim_frame_sizes: if keep_anim {
                self.anim_frame_sizes
            } else {
                Vec::new()
            },
        }
    }
}

fn resolve_chunk_textures(
    chunk_id: i32,
    meta: Option<&AtlasChunkMeta>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
) {
    if chunk_id >= mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID as i32 {
        return (
            Some(format!("dynamic/chunk_{chunk_id}.png")),
            None,
            None,
            None,
            None,
            false,
        );
    }

    let Some(meta) = meta else {
        let fallback = if chunk_id >= 0 {
            Some(format!(
                "atlas/textures/blocks_chunk_{:03}.png",
                chunk_id + 1
            ))
        } else {
            None
        };
        return (fallback, None, None, None, None, false);
    };

    let prefix = "atlas/textures/";
    let mut albedo = None;
    let mut overlay = None;
    let mut normal = None;
    let mut orm = None;
    let mut aux = None;

    for slot in &meta.textures {
        let id = if slot.file_name.starts_with(prefix) {
            slot.file_name.clone()
        } else {
            format!("{prefix}{}", slot.file_name)
        };
        match slot.role.as_str() {
            "albedo" => albedo = Some(id),
            "overlay" => overlay = Some(id),
            "normal" => normal = Some(id),
            "orm" => orm = Some(id),
            "aux" => aux = Some(id),
            _ => {}
        }
    }

    if albedo.is_none() {
        albedo = Some(format!("{prefix}{}.png", meta.file_stem()));
    }

    (albedo, overlay, normal, orm, aux, meta.is_animated)
}

#[inline]
fn quad_tri_corner_map(quad: &[u32; 4], tris: &[u32; 6]) -> [usize; 6] {
    if tris[0] == quad[0] && tris[1] == quad[1] && tris[2] == quad[2] {
        return [0, 1, 2, 0, 2, 3];
    }
    if tris[0] == quad[1] && tris[1] == quad[2] && tris[2] == quad[3] {
        return [1, 2, 3, 0, 1, 3];
    }
    let mut map = [0usize; 6];
    for k in 0..6 {
        let target = tris[k];
        map[k] = quad.iter().position(|&v| v == target).unwrap_or(k % 4);
    }
    map
}

#[inline]
fn sample_uv(mesh: &MeshData, vert_idx: usize, corner_idx: usize) -> [f32; 2] {
    match mesh.uv_layout {
        UvLayout::PerLoop => mesh.uvs.get(corner_idx).copied().unwrap_or([0.0, 0.0]),
        UvLayout::PerVertex => mesh.uvs.get(vert_idx).copied().unwrap_or([0.0, 0.0]),
        UvLayout::None => [0.0, 0.0],
    }
}

#[inline]
fn sample_color_and_ao(
    mesh: &MeshData,
    tint_colors: Option<&[[f32; 4]]>,
    ao_values: Option<&[f32]>,
    total_corners: usize,
    vert_idx: usize,
    corner_idx: usize,
) -> [f32; 4] {
    let rgb = if let Some(tc) = tint_colors {
        if tc.len() == total_corners {
            tc.get(corner_idx).copied().unwrap_or([1.0; 4])
        } else {
            tc.get(vert_idx).copied().unwrap_or([1.0; 4])
        }
    } else if let Some(colors) = mesh.colors.as_deref() {
        if colors.len() == total_corners {
            colors.get(corner_idx).copied().unwrap_or([1.0; 4])
        } else {
            colors.get(vert_idx).copied().unwrap_or([1.0; 4])
        }
    } else {
        [1.0, 1.0, 1.0, 1.0]
    };

    let ao = ao_values
        .and_then(|arr| arr.get(corner_idx).copied())
        .unwrap_or(1.0);

    [rgb[0], rgb[1], rgb[2], ao]
}

pub(crate) fn build_render_scene(
    mesh: &MeshData,
    atlas_mapping: Option<&AtlasAddressMap>,
) -> WasmRenderScene {
    if mesh.is_empty() || mesh.indices.is_empty() {
        return WasmRenderScene {
            batches: Vec::new(),
            total_vertices: 0,
            total_triangles: 0,
        };
    }

    let chunk_ids = mesh
        .get_custom_attribute(ATTR_ATLAS_CHUNK_ID)
        .and_then(|attr| match &attr.data {
            AttributeData::Int32(v) => Some(v.as_slice()),
            _ => None,
        });
    let tint_colors = mesh
        .get_custom_attribute(ATTR_BIOME_TINT_COLOR)
        .and_then(|attr| match &attr.data {
            AttributeData::Float4(v) => Some(v.as_slice()),
            _ => None,
        });
    let tint_data = mesh
        .get_custom_attribute(ATTR_BIOME_TINT_DATA)
        .and_then(|attr| match &attr.data {
            AttributeData::Float4(v) => Some(v.as_slice()),
            _ => None,
        });
    let ao_values = mesh
        .get_custom_attribute(ATTR_AO)
        .and_then(|attr| match &attr.data {
            AttributeData::Float(v) => Some(v.as_slice()),
            _ => None,
        });
    let anim_timings = mesh
        .get_custom_attribute(ATTR_ANIM_TIMING)
        .and_then(|attr| match &attr.data {
            AttributeData::Float3(v) => Some(v.as_slice()),
            _ => None,
        });
    let anim_frame_sizes = mesh
        .get_custom_attribute(ATTR_ANIM_FRAME_SIZE)
        .and_then(|attr| match &attr.data {
            AttributeData::Float3(v) => Some(v.as_slice()),
            _ => None,
        });

    let mut builders: BTreeMap<i32, BatchBuilder> = BTreeMap::new();

    if mesh.is_quad_topology() {
        let quad_count = mesh.quad_count().min(mesh.indices.len() / 6);
        let quads = mesh.reconstruct_quad_indices();
        let total_corners = quad_count * 4;

        for f in 0..quad_count {
            let chunk_id = chunk_ids
                .and_then(|ids| ids.get(f).copied())
                .or_else(|| mesh.face_materials.get(f).map(|&id| id as i32))
                .unwrap_or(0);
            let tint_w = tint_data
                .and_then(|arr| arr.get(f).copied())
                .map(|td| [td[0] * td[2], td[1] * td[2]])
                .unwrap_or([0.0, 0.0]);
            let anim_t = anim_timings.and_then(|arr| arr.get(f).copied());
            let anim_s = anim_frame_sizes.and_then(|arr| arr.get(f).copied());

            let builder = builders.entry(chunk_id).or_default();
            let base = (builder.positions.len() / 3) as u32;

            let q_base = f * 4;
            let quad = [
                quads[q_base],
                quads[q_base + 1],
                quads[q_base + 2],
                quads[q_base + 3],
            ];
            let t_base = f * 6;
            let tris = [
                mesh.indices[t_base],
                mesh.indices[t_base + 1],
                mesh.indices[t_base + 2],
                mesh.indices[t_base + 3],
                mesh.indices[t_base + 4],
                mesh.indices[t_base + 5],
            ];
            let corner_map = quad_tri_corner_map(&quad, &tris);

            for (c, &v_idx_u32) in quad.iter().enumerate() {
                let v_idx = v_idx_u32 as usize;
                let corner_idx = q_base + c;
                let pos = mesh.positions.get(v_idx).copied().unwrap_or([0.0; 3]);
                let norm = mesh.normals.get(v_idx).copied().unwrap_or([0.0, 1.0, 0.0]);
                let uv = sample_uv(mesh, v_idx, corner_idx);
                let color = sample_color_and_ao(
                    mesh,
                    tint_colors,
                    ao_values,
                    total_corners,
                    v_idx,
                    corner_idx,
                );
                builder.push_vertex(pos, norm, uv, color, tint_w, anim_t, anim_s);
            }

            for &c in &corner_map {
                builder.indices.push(base + c as u32);
            }
        }
    } else {
        let tri_count =
            (mesh.indices.len() / 3).min(mesh.face_materials.len().max(mesh.indices.len() / 3));
        let total_corners = tri_count * 3;

        for f in 0..tri_count {
            let chunk_id = chunk_ids
                .and_then(|ids| ids.get(f).copied())
                .or_else(|| mesh.face_materials.get(f).map(|&id| id as i32))
                .unwrap_or(0);
            let tint_w = tint_data
                .and_then(|arr| arr.get(f).copied())
                .map(|td| [td[0] * td[2], td[1] * td[2]])
                .unwrap_or([0.0, 0.0]);
            let anim_t = anim_timings.and_then(|arr| arr.get(f).copied());
            let anim_s = anim_frame_sizes.and_then(|arr| arr.get(f).copied());

            let builder = builders.entry(chunk_id).or_default();
            let base = (builder.positions.len() / 3) as u32;
            let t_base = f * 3;

            for c in 0..3 {
                let corner_idx = t_base + c;
                let v_idx = mesh.indices[corner_idx] as usize;
                let pos = mesh.positions.get(v_idx).copied().unwrap_or([0.0; 3]);
                let norm = mesh.normals.get(v_idx).copied().unwrap_or([0.0, 1.0, 0.0]);
                let uv = sample_uv(mesh, v_idx, corner_idx);
                let color = sample_color_and_ao(
                    mesh,
                    tint_colors,
                    ao_values,
                    total_corners,
                    v_idx,
                    corner_idx,
                );
                builder.push_vertex(pos, norm, uv, color, tint_w, anim_t, anim_s);
                builder.indices.push(base + c as u32);
            }
        }
    }

    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    let mut batches = Vec::with_capacity(builders.len());

    for (chunk_id, builder) in builders {
        let meta =
            atlas_mapping.and_then(|map| map.chunks.iter().find(|c| c.chunk_id as i32 == chunk_id));
        let batch = builder.finish(chunk_id, meta);
        total_vertices += batch.vertex_count();
        total_triangles += batch.triangle_count();
        batches.push(Some(batch));
    }

    WasmRenderScene {
        batches,
        total_vertices,
        total_triangles,
    }
}
