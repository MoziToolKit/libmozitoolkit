//! # `mtk-py` Model Baker Binding
//!
//! Exposes universal headless BlockState and Minecraft model baking to Python.

use std::sync::Arc;

use crate::mesh::PyMeshData;
use crate::resource::PyResourcePackStack;
use mtk_model::{BakedModelDatabase, BlockModelJson, BlockState, BlockStateDefinition, ModelBaker};
use pyo3::prelude::*;

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
            .detach(
                || -> Result<(mtk_core::mesh::MeshData, Vec<String>), String> {
                    let bs_def = {
                        let bs_path =
                            format!("assets/{}/blockstates/{}.json", bs.namespace, bs.name);
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
                },
            )
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;

        Ok((PyMeshData::from_mesh(mesh), textures))
    }
}
