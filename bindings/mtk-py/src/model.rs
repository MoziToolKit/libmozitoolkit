//! # `mtk-py` Model Baker Binding
//!
//! Exposes universal headless BlockState and Minecraft model baking to Python.

use std::sync::Arc;

use crate::mesh::PyMeshData;
use crate::resource::PyResourcePackStack;
use mtk_core::Direction;
use mtk_model::{BakedModelDatabase, BlockModelJson, BlockState, BlockStateDefinition, ModelBaker};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Prebaked Model Database container.
#[pyclass(name = "BakedModelDatabase")]
#[derive(Default, Clone)]
pub struct PyBakedModelDatabase {
    pub(crate) inner: Arc<BakedModelDatabase>,
}

#[pymethods]
impl PyBakedModelDatabase {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BakedModelDatabase::new()),
        }
    }

    /// Number of prebaked models in the database.
    pub fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Retrieves baked mesh geometry and texture list for a given canonical blockstate string.
    #[pyo3(signature = (state_str, clip_hidden=true))]
    pub fn get_mesh(
        &self,
        state_str: &str,
        clip_hidden: bool,
    ) -> Option<(PyMeshData, Vec<String>)> {
        self.inner.get(state_str).map(|baked| {
            let (mesh, textures) = baked.to_mesh_with_textures(clip_hidden);
            (PyMeshData::from_mesh(mesh), textures)
        })
    }
}

/// Headless Minecraft Model Baker for BlockStates and custom models.
#[pyclass(name = "ModelBaker")]
#[derive(Default)]
pub struct PyModelBaker {
    pub(crate) inner: ModelBaker,
}

#[pymethods]
impl PyModelBaker {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: ModelBaker::new(),
        }
    }

    /// Clears internal model bake cache.
    pub fn clear_cache(&mut self) {
        self.inner.clear_cache();
    }

    /// Prebakes ALL blockstates discovered across the entire resource pack stack in parallel.
    ///
    /// Automatically releases Python GIL during multi-threaded baking.
    #[pyo3(signature = (stack, atlas=None))]
    pub fn bake_all(
        &self,
        py: Python<'_>,
        stack: &PyResourcePackStack,
        atlas: Option<&crate::texture::PyBakedAtlas>,
    ) -> PyResult<PyBakedModelDatabase> {
        let atlas_map = atlas.map(|a| &a.inner.address_map);
        let db = py
            .detach(|| libmtk::prebake_all_models(&stack.inner, atlas_map))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(PyBakedModelDatabase {
            inner: Arc::new(db),
        })
    }

    /// Bakes a single blockstate string into a `PyMeshData` and texture list.
    ///
    /// # Arguments
    /// - `stack`: Resource pack stack to resolve blockstates and JSON models.
    /// - `state_str`: E.g. `"minecraft:oak_stairs[facing=east,half=bottom,shape=straight]"`
    /// - `clip_hidden`: Whether to clip faces occluded by interior element geometry.
    #[pyo3(signature = (stack, state_str, clip_hidden=true))]
    pub fn bake_blockstate(
        &mut self,
        py: Python<'_>,
        stack: &PyResourcePackStack,
        state_str: &str,
        clip_hidden: bool,
    ) -> PyResult<(PyMeshData, Vec<String>)> {
        let bs = BlockState::parse(state_str)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        let (mesh, textures) = py
            .detach(|| -> Result<(mtk_core::MeshData, Vec<String>), String> {
                let bs_def = {
                    let bs_path = format!("assets/{}/blockstates/{}.json", bs.namespace, bs.name);
                    stack.inner.open_asset_raw(&bs_path).and_then(|bytes| {
                        serde_json::from_slice::<BlockStateDefinition>(&bytes).ok()
                    })
                };

                let baked = self
                    .inner
                    .bake_blockstate(state_str, bs_def.as_ref(), |model_id| {
                        if let Some(bytes) = stack.inner.open_model_raw(model_id) {
                            serde_json::from_slice::<BlockModelJson>(&bytes).ok()
                        } else {
                            None
                        }
                    })
                    .map_err(|e| e.to_string())?;

                let (mesh, textures) = baked.to_mesh_with_textures(clip_hidden);
                Ok((mesh, textures))
            })
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;

        Ok((PyMeshData::from_mesh(mesh), textures))
    }
}

/// Builds contiguous MeshData and texture list for a Minecraft Painting.
#[pyfunction]
#[pyo3(signature = (variant, facing="north", offset=None))]
pub fn build_painting_mesh(
    variant: &str,
    facing: &str,
    offset: Option<[f32; 3]>,
) -> PyResult<(PyMeshData, Vec<String>)> {
    let dir = Direction::parse_loose(facing).unwrap_or(Direction::North);
    let off = offset.map(|[x, y, z]| glam::Vec3::new(x, y, z));
    let (mesh, textures) = mtk_model::painting::create_painting_mesh(variant, dir, off);
    Ok((PyMeshData::from_mesh(mesh), textures))
}

/// Retrieves the (width, height) in integer blocks for a painting variant.
#[pyfunction]
pub fn get_painting_dimension(variant: &str) -> (u8, u8) {
    mtk_model::painting::painting_dimension(variant)
}

/// Retrieves metadata dictionary for a specific painting variant.
#[pyfunction]
pub fn get_painting_variant<'py>(
    py: Python<'py>,
    variant: &str,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    if let Some(v) = mtk_model::painting::get_painting_variant(variant) {
        let dict = PyDict::new(py);
        dict.set_item("name", v.name)?;
        dict.set_item("width", v.width)?;
        dict.set_item("height", v.height)?;
        dict.set_item("asset_id", v.asset_id)?;
        dict.set_item("title", v.title)?;
        dict.set_item("author", v.author)?;
        Ok(Some(dict))
    } else {
        Ok(None)
    }
}

/// Retrieves a list of all 51 official Minecraft 1.21+ Painting Variant metadata dictionaries.
#[pyfunction]
pub fn get_all_painting_variants<'py>(py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
    let mut list = Vec::with_capacity(mtk_model::painting::PAINTING_VARIANTS.len());
    for v in mtk_model::painting::all_painting_variants() {
        let dict = PyDict::new(py);
        dict.set_item("name", v.name)?;
        dict.set_item("width", v.width)?;
        dict.set_item("height", v.height)?;
        dict.set_item("asset_id", v.asset_id)?;
        dict.set_item("title", v.title)?;
        dict.set_item("author", v.author)?;
        list.push(dict);
    }
    Ok(list)
}

/// Calculates the exact 3D world center position for a painting attached to a support block.
#[pyfunction]
#[pyo3(signature = (support_pos, facing="north", width=None, height=None, variant=None))]
pub fn calculate_painting_position(
    support_pos: [i32; 3],
    facing: &str,
    width: Option<u8>,
    height: Option<u8>,
    variant: Option<&str>,
) -> [f32; 3] {
    let dir = Direction::parse_loose(facing).unwrap_or(Direction::North);
    let (w, h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        _ => {
            if let Some(v) = variant {
                mtk_model::painting::painting_dimension(v)
            } else {
                (1, 1)
            }
        }
    };
    let pos = mtk_model::painting::calculate_painting_world_position(support_pos, dir, w, h);
    [pos.x, pos.y, pos.z]
}
