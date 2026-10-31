//! MaterialResolver and BiomeResolver Python bindings.

use std::collections::HashMap;

use crate::buffer::{
    create_owned_f32_memoryview, create_owned_i32_memoryview, create_owned_u32_memoryview,
};
use mtk_material::{
    clean_identifier, remap_mesh_multi_uvs_paletted_with_options, reroute_mesh_animation,
    resolve_mesh_material_routes, BiomeResolver,
};
use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyMemoryView};

use crate::resource::PyResourcePackStack;

use crate::texture::PyBakedAtlas;

use super::atlas::PyGridAtlasSpec;

enum BorrowedU32Buffer {
    Py(PyBuffer<u32>),
    Owned(Vec<u32>),
}

impl BorrowedU32Buffer {
    fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<u32>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<u32>>()?))
    }

    fn as_slice(&self) -> &[u32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const u32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

enum BorrowedI32Buffer {
    Py(PyBuffer<i32>),
    Owned(Vec<i32>),
}

impl BorrowedI32Buffer {
    fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<i32>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<i32>>()?))
    }

    fn as_slice(&self) -> &[i32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const i32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

/// Builds a compact face-material palette + u32 indices without per-face Python strings.
#[pyfunction]
#[pyo3(signature = (slot_names, slot_is_mtk, slot_source_ids, poly_mats, existing_palette=None, existing_indices=None))]
pub fn build_material_palette<'py>(
    py: Python<'py>,
    slot_names: Vec<String>,
    slot_is_mtk: Vec<bool>,
    slot_source_ids: Vec<String>,
    poly_mats: &Bound<'_, PyAny>,
    existing_palette: Option<Vec<String>>,
    existing_indices: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Vec<String>, Bound<'py, PyMemoryView>)> {
    let mats = BorrowedI32Buffer::extract(poly_mats)?;
    let existing_arr = existing_indices
        .map(BorrowedU32Buffer::extract)
        .transpose()?;
    let existing_indices = existing_arr.as_ref().map(|b| b.as_slice()).unwrap_or(&[]);
    let existing_palette = existing_palette.unwrap_or_default();

    let mut palette: Vec<String> = Vec::new();
    let mut lookup: HashMap<String, u32> = HashMap::new();
    let mut indices: Vec<u32> = Vec::with_capacity(mats.as_slice().len());

    for &mat in mats.as_slice() {
        let slot = if mat >= 0 && (mat as usize) < slot_names.len() {
            mat as usize
        } else {
            0
        };
        let is_mtk = slot_is_mtk.get(slot).copied().unwrap_or(true);
        let source_id = slot_source_ids.get(slot).map(String::as_str).unwrap_or("");
        let existing = existing_indices
            .get(indices.len())
            .and_then(|idx| existing_palette.get(*idx as usize))
            .map(String::as_str)
            .unwrap_or("");
        let name = if is_mtk && !existing.is_empty() && existing != "mozi:fallback" {
            existing
        } else if is_mtk && !source_id.is_empty() {
            source_id
        } else {
            slot_names
                .get(slot)
                .map(String::as_str)
                .unwrap_or("default")
        };
        let idx = *lookup.entry(name.to_string()).or_insert_with(|| {
            let id = palette.len() as u32;
            palette.push(name.to_string());
            id
        });
        indices.push(idx);
    }

    let memoryview = create_owned_u32_memoryview(py, indices)?;
    Ok((palette, memoryview))
}

#[pyclass(name = "MaterialResolver")]
pub struct PyMaterialResolver;

#[pymethods]
impl PyMaterialResolver {
    /// Clean a raw material or texture name into a normalized identifier stem.
    #[staticmethod]
    pub fn clean_name(name: &str) -> String {
        clean_identifier(name)
    }

    /// Resolve atlas routing (chunk/texture/animation metadata) for face texture keys.
    #[staticmethod]
    #[pyo3(signature = (face_texture_keys, atlas, aliases=None, enable_animated_textures=false))]
    pub fn resolve_mesh_routes(
        py: Python<'_>,
        face_texture_keys: Vec<String>,
        atlas: &PyBakedAtlas,
        aliases: Option<HashMap<String, Vec<String>>>,
        enable_animated_textures: bool,
    ) -> PyResult<Py<PyAny>> {
        let routes = py.detach(|| {
            resolve_mesh_material_routes(
                &face_texture_keys,
                aliases.as_ref(),
                &atlas.inner.address_map,
                enable_animated_textures,
            )
        });

        let mut chunk_ids = Vec::with_capacity(routes.len());
        let mut texture_ids = Vec::with_capacity(routes.len());
        let mut is_animated = Vec::with_capacity(routes.len());
        let mut anim_timing = Vec::with_capacity(routes.len() * 3);
        let mut anim_frame_size = Vec::with_capacity(routes.len() * 3);
        let mut mapped = 0usize;
        for route in &routes {
            chunk_ids.push(route.chunk_id);
            texture_ids.push(route.texture_id);
            is_animated.push(route.is_animated);
            anim_timing.extend_from_slice(&route.anim_timing);
            anim_frame_size.extend_from_slice(&route.anim_frame_size);
            if route.mapped {
                mapped += 1;
            }
        }

        let dict = PyDict::new(py);
        dict.set_item("chunk_ids", chunk_ids)?;
        dict.set_item("texture_ids", texture_ids)?;
        dict.set_item("is_animated", is_animated)?;
        dict.set_item("anim_timing", anim_timing)?;
        dict.set_item("anim_frame_size", anim_frame_size)?;
        dict.set_item("mapped_faces", mapped)?;
        dict.set_item("unmapped_faces", routes.len().saturating_sub(mapped))?;
        Ok(dict.into())
    }

    /// Re-project existing atlas UVs from the previous chunk routing to the current animation policy.
    #[staticmethod]
    #[pyo3(signature = (uvs, face_loop_ranges, face_texture_keys, face_old_chunk_ids, atlas, aliases=None, enable_animated_textures=false))]
    pub fn reroute_mesh_animation(
        py: Python<'_>,
        uvs: Vec<f32>,
        face_loop_ranges: Vec<(u32, u32)>,
        face_texture_keys: Vec<String>,
        face_old_chunk_ids: Vec<u16>,
        atlas: &PyBakedAtlas,
        aliases: Option<HashMap<String, Vec<String>>>,
        enable_animated_textures: bool,
    ) -> PyResult<Py<PyAny>> {
        let loop_count = uvs.len() / 2;
        let mut uv_pairs: Vec<[f32; 2]> = Vec::with_capacity(loop_count);
        for i in 0..loop_count {
            uv_pairs.push([uvs[i * 2], uvs[i * 2 + 1]]);
        }

        let result = py.detach(|| {
            reroute_mesh_animation(
                &uv_pairs,
                &face_loop_ranges,
                &face_texture_keys,
                &face_old_chunk_ids,
                &atlas.inner.address_map,
                aliases.as_ref(),
                enable_animated_textures,
            )
        });

        let mut flat_atlas_uvs = Vec::with_capacity(result.atlas_uvs.len() * 2);
        for p in &result.atlas_uvs {
            flat_atlas_uvs.push(p[0]);
            flat_atlas_uvs.push(p[1]);
        }

        let dict = PyDict::new(py);
        dict.set_item("atlas_uvs", flat_atlas_uvs)?;
        dict.set_item("face_chunk_ids", result.face_chunk_ids)?;
        dict.set_item("face_texture_ids", result.face_texture_ids)?;
        dict.set_item("face_is_animated", result.face_is_animated)?;
        dict.set_item("face_anim_timing", result.face_anim_timing)?;
        dict.set_item("face_anim_frame_size", result.face_anim_frame_size)?;
        dict.set_item("unmapped_faces", result.unmapped_faces)?;
        dict.set_item("uv_unmapped_faces", result.uv_unmapped_faces)?;
        Ok(dict.into())
    }

    /// Palette-native multi-UV remap. Returns zero-copy buffers for per-face ids and metrics.
    #[staticmethod]
    #[pyo3(signature = (
        uvs,
        face_material_palette,
        face_material_indices,
        face_loop_ranges,
        atlas,
        aliases=None,
        grid_atlas_spec=None,
        existing_key_palette=None,
        existing_key_indices=None,
        enable_animated_textures=false
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn remap_mesh_multi_uvs_paletted(
        py: Python<'_>,
        uvs: Vec<f32>,
        face_material_palette: Vec<String>,
        face_material_indices: &Bound<'_, PyAny>,
        face_loop_ranges: Vec<(u32, u32)>,
        atlas: &PyBakedAtlas,
        aliases: Option<HashMap<String, Vec<String>>>,
        grid_atlas_spec: Option<&PyGridAtlasSpec>,
        existing_key_palette: Option<Vec<String>>,
        existing_key_indices: Option<&Bound<'_, PyAny>>,
        enable_animated_textures: bool,
    ) -> PyResult<Py<PyAny>> {
        let loop_count = uvs.len() / 2;
        let mut uv_pairs: Vec<[f32; 2]> = Vec::with_capacity(loop_count);
        for i in 0..loop_count {
            uv_pairs.push([uvs[i * 2], uvs[i * 2 + 1]]);
        }

        let material_indices = BorrowedU32Buffer::extract(face_material_indices)?;
        let existing_indices = existing_key_indices
            .map(BorrowedU32Buffer::extract)
            .transpose()?;
        let result = remap_mesh_multi_uvs_paletted_with_options(
            &uv_pairs,
            &face_material_palette,
            material_indices.as_slice(),
            &face_loop_ranges,
            &atlas.inner.address_map,
            aliases.as_ref(),
            grid_atlas_spec.map(|s| &s.inner),
            existing_key_palette.as_deref(),
            existing_indices.as_ref().map(|b| b.as_slice()),
            enable_animated_textures,
        );

        let mesh = result.mesh;
        let mut flat_atlas_uvs = Vec::with_capacity(mesh.atlas_uvs.len() * 2);
        for p in &mesh.atlas_uvs {
            flat_atlas_uvs.push(p[0]);
            flat_atlas_uvs.push(p[1]);
        }
        let mut flat_local_uvs = Vec::with_capacity(mesh.local_uvs.len() * 2);
        for p in &mesh.local_uvs {
            flat_local_uvs.push(p[0]);
            flat_local_uvs.push(p[1]);
        }
        let mut flat_transforms = Vec::with_capacity(mesh.face_uv_transforms.len() * 4);
        for t in &mesh.face_uv_transforms {
            flat_transforms.extend_from_slice(t);
        }
        let mut flat_anim_timing = Vec::with_capacity(mesh.face_anim_timing.len() * 3);
        for t in &mesh.face_anim_timing {
            flat_anim_timing.extend_from_slice(t);
        }
        let mut flat_anim_size = Vec::with_capacity(mesh.face_anim_frame_size.len() * 3);
        for s in &mesh.face_anim_frame_size {
            flat_anim_size.extend_from_slice(s);
        }

        let dict = PyDict::new(py);
        dict.set_item(
            "atlas_uvs",
            create_owned_f32_memoryview(py, flat_atlas_uvs)?,
        )?;
        dict.set_item(
            "local_uvs",
            create_owned_f32_memoryview(py, flat_local_uvs)?,
        )?;
        dict.set_item(
            "face_chunk_ids",
            create_owned_i32_memoryview(
                py,
                mesh.face_chunk_ids.iter().map(|v| *v as i32).collect(),
            )?,
        )?;
        dict.set_item(
            "face_texture_ids",
            create_owned_i32_memoryview(
                py,
                mesh.face_texture_ids.iter().map(|v| *v as i32).collect(),
            )?,
        )?;
        dict.set_item(
            "face_uv_transforms",
            create_owned_f32_memoryview(py, flat_transforms)?,
        )?;
        dict.set_item(
            "face_uv_rotations",
            create_owned_f32_memoryview(py, mesh.face_uv_rotations.clone())?,
        )?;
        dict.set_item("face_is_animated", mesh.face_is_animated)?;
        dict.set_item(
            "face_anim_timing",
            create_owned_f32_memoryview(py, flat_anim_timing)?,
        )?;
        dict.set_item(
            "face_anim_frame_size",
            create_owned_f32_memoryview(py, flat_anim_size)?,
        )?;
        dict.set_item("source_key_palette", result.source_key_palette)?;
        dict.set_item(
            "face_source_key_indices",
            create_owned_u32_memoryview(py, result.face_source_key_indices.clone())?,
        )?;
        dict.set_item("unmapped_faces", mesh.unmapped_faces)?;
        dict.set_item("face_count", mesh.face_count)?;
        dict.set_item("loop_count", mesh.loop_count)?;
        Ok(dict.into())
    }
}

/// Python wrapper for Rust BiomeResolver.
#[pyclass(name = "BiomeResolver")]
#[derive(Clone)]
pub struct PyBiomeResolver {
    pub(crate) inner: BiomeResolver,
}

fn py_dict_to_json_hashmap(
    py: Python<'_>,
    dict: &Bound<'_, pyo3::types::PyDict>,
) -> PyResult<HashMap<String, serde_json::Value>> {
    let json_module = py.import("json")?;
    let json_str: String = json_module.call_method1("dumps", (dict,))?.extract()?;
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(map)
}

#[pymethods]
impl PyBiomeResolver {
    #[new]
    #[pyo3(signature = (models=None))]
    pub fn new(py: Python<'_>, models: Option<Bound<'_, pyo3::types::PyDict>>) -> PyResult<Self> {
        let mut inner = mtk_material::BiomeResolver::new();
        if let Some(m) = models {
            let map = py_dict_to_json_hashmap(py, &m)?;
            inner.set_models(map);
        }
        Ok(Self { inner })
    }

    /// Load BiomeResolver from a JSON string.
    #[staticmethod]
    pub fn from_json(json_str: &str) -> PyResult<Self> {
        let inner = mtk_material::BiomeResolver::from_json(json_str)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Load BiomeResolver from a JSON file path.
    #[staticmethod]
    pub fn from_file(file_path: &str) -> PyResult<Self> {
        let inner = mtk_material::BiomeResolver::from_file(file_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Export mapping table to JSON string.
    pub fn to_json(&self) -> PyResult<String> {
        self.inner
            .to_json()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Load block models from a ResourcePackStack.
    pub fn load_from_pack_stack(&mut self, stack: &PyResourcePackStack) -> PyResult<()> {
        self.inner.load_from_pack_stack(&stack.inner);
        Ok(())
    }

    /// Resolve tint metadata for a single texture.
    #[pyo3(signature = (texture_name, block_name=None, tint_index=None))]
    pub fn get_tint_info(
        &self,
        py: Python<'_>,
        texture_name: &str,
        block_name: Option<&str>,
        tint_index: Option<i32>,
    ) -> PyResult<Py<PyAny>> {
        let info = self
            .inner
            .get_tint_info(texture_name, block_name, tint_index);
        let dict = PyDict::new(py);
        dict.set_item("tint_type", info.tint_type)?;
        dict.set_item("tint_category", info.tint_category)?;
        dict.set_item("tint_weight", info.tint_weight)?;
        dict.set_item("base_tint_weight", info.base_tint_weight)?;
        dict.set_item("overlay_tint_weight", info.overlay_tint_weight)?;
        dict.set_item("has_overlay", info.has_overlay)?;
        dict.set_item("overlay_texture", info.overlay_texture)?;
        dict.set_item("is_hardcoded", info.is_hardcoded)?;
        dict.set_item("hardcoded_color", info.hardcoded_color)?;
        dict.set_item("hardcoded_hex", info.hardcoded_hex)?;
        dict.set_item("default_tint_weight", info.tint_weight)?;
        dict.set_item("default_base_tint_weight", info.base_tint_weight)?;
        dict.set_item("default_overlay_tint_weight", info.overlay_tint_weight)?;
        Ok(dict.into())
    }
}
