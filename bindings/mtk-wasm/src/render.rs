//! Three.js / WebGPU render-ready mesh batching from `MeshData`.
//!
//! Converts `MeshData` (which uses Blender-compatible `PerLoop` UVs/colors and
//! corner AO attributes) into per-atlas-chunk `PerVertex` indexed geometry batches
//! ready for zero-copy `THREE.BufferAttribute` construction.

use std::collections::{BTreeMap, HashMap};

use libmtk::{chunks, PackageManifest};
use mtk_core::attribute_constants::{
    ATTR_ANIM_FRAME_SIZE, ATTR_ANIM_TIMING, ATTR_AO, ATTR_ATLAS_CHUNK_ID, ATTR_BIOME_TINT_COLOR,
    ATTR_BIOME_TINT_DATA,
};
use mtk_core::{quantize_normal, AttributeData, MeshData, UvLayout, WeldMode};
use mtk_texture::{
    dynamic_chunk_texture_id, AtlasAddressMap, AtlasChunkMeta, DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID,
};
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

#[inline]
fn norm_f32_bits(v: f32) -> u32 {
    if v == 0.0 {
        0
    } else {
        v.to_bits()
    }
}

#[derive(Hash, PartialEq, Eq)]
struct GpuVertexKey {
    pos: [i32; 3],
    norm: [i32; 3],
    uv: [u32; 2],
    color: [u32; 4],
    tint_weight: [u32; 2],
    anim_timing: Option<[u32; 3]>,
    anim_frame_size: Option<[u32; 3]>,
}

impl GpuVertexKey {
    #[allow(clippy::too_many_arguments)]
    fn new(
        pos: [f32; 3],
        norm: [f32; 3],
        uv: [f32; 2],
        color: [f32; 4],
        tint_weight: [f32; 2],
        anim_timing: Option<[f32; 3]>,
        anim_frame_size: Option<[f32; 3]>,
    ) -> Self {
        const INV_POS_TOL: f32 = 10_000.0;
        Self {
            pos: [
                (pos[0] * INV_POS_TOL).round() as i32,
                (pos[1] * INV_POS_TOL).round() as i32,
                (pos[2] * INV_POS_TOL).round() as i32,
            ],
            norm: quantize_normal(norm),
            uv: [norm_f32_bits(uv[0]), norm_f32_bits(uv[1])],
            color: [
                norm_f32_bits(color[0]),
                norm_f32_bits(color[1]),
                norm_f32_bits(color[2]),
                norm_f32_bits(color[3]),
            ],
            tint_weight: [norm_f32_bits(tint_weight[0]), norm_f32_bits(tint_weight[1])],
            anim_timing: anim_timing.map(|t| {
                [
                    norm_f32_bits(t[0]),
                    norm_f32_bits(t[1]),
                    norm_f32_bits(t[2]),
                ]
            }),
            anim_frame_size: anim_frame_size.map(|s| {
                [
                    norm_f32_bits(s[0]),
                    norm_f32_bits(s[1]),
                    norm_f32_bits(s[2]),
                ]
            }),
        }
    }
}

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
    weld_per_vertex: bool,
    vertex_map: HashMap<GpuVertexKey, u32>,
}

impl BatchBuilder {
    fn new(weld_per_vertex: bool) -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
            tint_weights: Vec::new(),
            indices: Vec::new(),
            anim_timings: Vec::new(),
            anim_frame_sizes: Vec::new(),
            has_animated_faces: false,
            weld_per_vertex,
            vertex_map: HashMap::new(),
        }
    }

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
    ) -> u32 {
        if self.weld_per_vertex {
            let key = GpuVertexKey::new(
                pos,
                norm,
                uv,
                color,
                tint_weight,
                anim_timing,
                anim_frame_size,
            );
            if let Some(&existing) = self.vertex_map.get(&key) {
                return existing;
            }
            let new_idx = (self.positions.len() / 3) as u32;
            self.vertex_map.insert(key, new_idx);
            self.append_vertex_attrs(
                pos,
                norm,
                uv,
                color,
                tint_weight,
                anim_timing,
                anim_frame_size,
            );
            return new_idx;
        }

        let new_idx = (self.positions.len() / 3) as u32;
        self.append_vertex_attrs(
            pos,
            norm,
            uv,
            color,
            tint_weight,
            anim_timing,
            anim_frame_size,
        );
        new_idx
    }

    #[allow(clippy::too_many_arguments)]
    fn append_vertex_attrs(
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
    if chunk_id >= DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID as i32 {
        return (
            Some(dynamic_chunk_texture_id(chunk_id as u16)),
            None,
            None,
            None,
            None,
            false,
        );
    }

    let Some(meta) = meta else {
        let fallback = (chunk_id >= 0)
            .then(|| chunks::atlas_texture(&format!("blocks_chunk_{:03}.png", chunk_id + 1)));
        return (fallback, None, None, None, None, false);
    };

    let mut albedo = None;
    let mut overlay = None;
    let mut normal = None;
    let mut orm = None;
    let mut aux = None;

    for slot in &meta.textures {
        let id = chunks::atlas_texture(&slot.file_name);
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
        albedo = Some(chunks::atlas_texture(&format!("{}.png", meta.file_stem())));
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

struct FaceAttrRefs<'a> {
    mesh: &'a MeshData,
    chunk_ids: Option<&'a [i32]>,
    tint_colors: Option<&'a [[f32; 4]]>,
    tint_data: Option<&'a [[f32; 4]]>,
    ao_values: Option<&'a [f32]>,
    anim_timings: Option<&'a [[f32; 3]]>,
    anim_frame_sizes: Option<&'a [[f32; 3]]>,
    total_corners: usize,
}

impl<'a> FaceAttrRefs<'a> {
    fn from_mesh(mesh: &'a MeshData, total_corners: usize) -> Self {
        Self {
            mesh,
            chunk_ids: mesh
                .get_custom_attribute(ATTR_ATLAS_CHUNK_ID)
                .and_then(|attr| match &attr.data {
                    AttributeData::Int32(v) => Some(v.as_slice()),
                    _ => None,
                }),
            tint_colors: mesh
                .get_custom_attribute(ATTR_BIOME_TINT_COLOR)
                .and_then(|attr| match &attr.data {
                    AttributeData::Float4(v) => Some(v.as_slice()),
                    _ => None,
                }),
            tint_data: mesh
                .get_custom_attribute(ATTR_BIOME_TINT_DATA)
                .and_then(|attr| match &attr.data {
                    AttributeData::Float4(v) => Some(v.as_slice()),
                    _ => None,
                }),
            ao_values: mesh
                .get_custom_attribute(ATTR_AO)
                .and_then(|attr| match &attr.data {
                    AttributeData::Float(v) => Some(v.as_slice()),
                    _ => None,
                }),
            anim_timings: mesh
                .get_custom_attribute(ATTR_ANIM_TIMING)
                .and_then(|attr| match &attr.data {
                    AttributeData::Float3(v) => Some(v.as_slice()),
                    _ => None,
                }),
            anim_frame_sizes: mesh
                .get_custom_attribute(ATTR_ANIM_FRAME_SIZE)
                .and_then(|attr| match &attr.data {
                    AttributeData::Float3(v) => Some(v.as_slice()),
                    _ => None,
                }),
            total_corners,
        }
    }

    #[inline]
    fn chunk_id(&self, face_idx: usize) -> i32 {
        self.chunk_ids
            .and_then(|ids| ids.get(face_idx).copied())
            .or_else(|| self.mesh.face_materials.get(face_idx).map(|&id| id as i32))
            .unwrap_or(0)
    }

    #[inline]
    fn tint_weights(&self, face_idx: usize) -> [f32; 2] {
        self.tint_data
            .and_then(|arr| arr.get(face_idx).copied())
            .map(|td| [td[0] * td[2], td[1] * td[2]])
            .unwrap_or([0.0, 0.0])
    }

    #[inline]
    fn push_corner(
        &self,
        builder: &mut BatchBuilder,
        v_idx: usize,
        corner_idx: usize,
        tint_w: [f32; 2],
        anim_t: Option<[f32; 3]>,
        anim_s: Option<[f32; 3]>,
    ) -> u32 {
        let pos = self.mesh.positions.get(v_idx).copied().unwrap_or([0.0; 3]);
        let norm = self
            .mesh
            .normals
            .get(v_idx)
            .copied()
            .unwrap_or([0.0, 1.0, 0.0]);
        let uv = sample_uv(self.mesh, v_idx, corner_idx);
        let color = sample_color_and_ao(
            self.mesh,
            self.tint_colors,
            self.ao_values,
            self.total_corners,
            v_idx,
            corner_idx,
        );
        builder.push_vertex(pos, norm, uv, color, tint_w, anim_t, anim_s)
    }
}

pub(crate) fn build_render_scene(
    mesh: &MeshData,
    atlas_mapping: Option<&AtlasAddressMap>,
    weld_mode: WeldMode,
) -> WasmRenderScene {
    if mesh.is_empty() || mesh.indices.is_empty() {
        return WasmRenderScene {
            batches: Vec::new(),
            total_vertices: 0,
            total_triangles: 0,
        };
    }

    let weld_per_vertex = weld_mode == WeldMode::PerVertex;
    let mut builders: BTreeMap<i32, BatchBuilder> = BTreeMap::new();

    if mesh.is_quad_topology() {
        let quad_count = mesh.quad_count().min(mesh.indices.len() / 6);
        let quads = mesh.reconstruct_quad_indices();
        let attrs = FaceAttrRefs::from_mesh(mesh, quad_count * 4);

        for f in 0..quad_count {
            let chunk_id = attrs.chunk_id(f);
            let tint_w = attrs.tint_weights(f);
            let anim_t = attrs.anim_timings.and_then(|arr| arr.get(f).copied());
            let anim_s = attrs.anim_frame_sizes.and_then(|arr| arr.get(f).copied());

            let builder = builders
                .entry(chunk_id)
                .or_insert_with(|| BatchBuilder::new(weld_per_vertex));
            let q_base = f * 4;
            let t_base = f * 6;
            let quad: [u32; 4] = quads[q_base..q_base + 4].try_into().unwrap_or([0; 4]);
            let tris: [u32; 6] = mesh.indices[t_base..t_base + 6]
                .try_into()
                .unwrap_or([0; 6]);

            let mut corner_indices = [0u32; 4];
            for (c, &v_idx_u32) in quad.iter().enumerate() {
                corner_indices[c] = attrs.push_corner(
                    builder,
                    v_idx_u32 as usize,
                    q_base + c,
                    tint_w,
                    anim_t,
                    anim_s,
                );
            }
            for c in quad_tri_corner_map(&quad, &tris) {
                builder.indices.push(corner_indices[c]);
            }
        }
    } else {
        let tri_count =
            (mesh.indices.len() / 3).min(mesh.face_materials.len().max(mesh.indices.len() / 3));
        let attrs = FaceAttrRefs::from_mesh(mesh, tri_count * 3);

        for f in 0..tri_count {
            let chunk_id = attrs.chunk_id(f);
            let tint_w = attrs.tint_weights(f);
            let anim_t = attrs.anim_timings.and_then(|arr| arr.get(f).copied());
            let anim_s = attrs.anim_frame_sizes.and_then(|arr| arr.get(f).copied());

            let builder = builders
                .entry(chunk_id)
                .or_insert_with(|| BatchBuilder::new(weld_per_vertex));
            let t_base = f * 3;

            for c in 0..3 {
                let corner_idx = t_base + c;
                let v_idx = mesh.indices[corner_idx] as usize;
                let out_idx = attrs.push_corner(builder, v_idx, corner_idx, tint_w, anim_t, anim_s);
                builder.indices.push(out_idx);
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
