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

    /// Returns true if the database contains no baked models.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Models discovered in the resource stack that produced no baked geometry.
    ///
    /// Empty for databases loaded from a cache: failures describe one bake run.
    #[getter]
    pub fn bake_failures<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyList>> {
        let list = pyo3::types::PyList::empty(py);
        for failure in self.inner.bake_failures() {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("key", &failure.key)?;
            dict.set_item("reason", &failure.reason)?;
            list.append(dict)?;
        }
        Ok(list)
    }

    /// Bakes a model into a local-space mesh plus the texture keys its faces reference.
    ///
    /// `state_or_item` accepts a canonical blockstate string or an item identifier. Item
    /// models are tried first so a block item resolves to its item display variant rather
    /// than the block-context model.
    #[pyo3(signature = (state_or_item, clip_hidden=true, weld=false))]
    pub fn get_mesh(
        &self,
        state_or_item: &str,
        clip_hidden: bool,
        weld: bool,
    ) -> Option<(crate::mesh::PyMeshData, Vec<String>)> {
        self.inner
            .get_item(state_or_item)
            .or_else(|| self.inner.get(state_or_item))
            .map(|baked| {
                let (mut mesh, textures) = baked.to_mesh_with_textures(clip_hidden);
                if weld {
                    mesh.weld_vertices(1e-4);
                }
                (crate::mesh::PyMeshData::from_mesh(mesh), textures)
            })
    }

    /// Item display transforms (`fixed`, `gui`, `ground`, ...) plus `gui_light`.
    ///
    /// Returns `None` when the item has no baked model, so callers never have to guess a
    /// scale or orientation.
    pub fn get_item_transforms<'py>(
        &self,
        py: Python<'py>,
        item_id: &str,
    ) -> PyResult<Option<Bound<'py, pyo3::types::PyDict>>> {
        let Some(baked) = self.inner.get_item(item_id) else {
            return Ok(None);
        };
        let dict = pyo3::types::PyDict::new(py);
        let make_transform_dict =
            |t: &mtk_model::ModelTransform| -> PyResult<Bound<'py, pyo3::types::PyDict>> {
                let d = pyo3::types::PyDict::new(py);
                d.set_item("rotation", t.rotation.to_vec())?;
                d.set_item("translation", t.translation.to_vec())?;
                d.set_item("scale", t.scale.to_vec())?;
                Ok(d)
            };

        if let Some(ref t) = baked.transforms.fixed {
            dict.set_item("fixed", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.gui {
            dict.set_item("gui", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.ground {
            dict.set_item("ground", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.head {
            dict.set_item("head", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.thirdperson_righthand {
            dict.set_item("thirdperson_righthand", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.thirdperson_lefthand {
            dict.set_item("thirdperson_lefthand", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.firstperson_righthand {
            dict.set_item("firstperson_righthand", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.firstperson_lefthand {
            dict.set_item("firstperson_lefthand", make_transform_dict(t)?)?;
        }
        if let Some(ref t) = baked.transforms.on_shelf {
            dict.set_item("on_shelf", make_transform_dict(t)?)?;
        }
        if let Some(ref gl) = baked.gui_light {
            let gl_str = match gl {
                mtk_model::GuiLight::Front => "front",
                mtk_model::GuiLight::Side => "side",
            };
            dict.set_item("gui_light", gl_str)?;
        }
        Ok(Some(dict))
    }

    /// Bakes a tiled showcase gallery of every item model into one combined mesh.
    #[pyo3(signature = (columns=36, spacing=1.5, vertical=false, filter_mode="items_only", weld=true))]
    pub fn build_items_gallery(
        &self,
        columns: usize,
        spacing: f32,
        vertical: bool,
        filter_mode: &str,
        weld: bool,
    ) -> (crate::mesh::PyMeshData, Vec<String>, usize) {
        let mut item_keys: Vec<String> = self
            .inner
            .models
            .keys()
            .filter(|k| k.starts_with("minecraft:item/") || k.contains(":item/"))
            .cloned()
            .collect();
        item_keys.sort();

        let cols = columns.max(1);
        let mut gallery_mesh = mtk_core::MeshData::new();
        gallery_mesh.uv_layout = mtk_core::UvLayout::PerLoop;
        let mut all_textures: Vec<String> = Vec::new();
        let mut total_items = 0usize;

        let mode = filter_mode.to_lowercase();

        for key in item_keys {
            if let Some(baked) = self.inner.get(&key) {
                let (mut item_mesh, textures) = baked.to_mesh_with_textures(false);
                if item_mesh.is_empty() {
                    continue;
                }

                let has_item_tex = textures
                    .iter()
                    .any(|t| t.contains(":item/") || t.starts_with("item/"));
                let has_block_tex = textures
                    .iter()
                    .any(|t| t.contains(":block/") || t.starts_with("block/"));

                let should_skip = (mode == "items_only" && (!has_item_tex || has_block_tex))
                    || (mode == "blocks_only" && (!has_block_tex || has_item_tex));
                if should_skip {
                    continue;
                }

                if weld {
                    item_mesh.weld_vertices(1e-4);
                }

                let col = total_items % cols;
                let row = total_items / cols;
                let offset_x = (col as f32) * spacing;
                let offset_y = -(row as f32) * spacing;

                let base_v_idx = gallery_mesh.positions.len() as u32;

                for p in item_mesh.positions {
                    if vertical {
                        gallery_mesh
                            .positions
                            .push([p[0] + offset_x, p[2], p[1] + offset_y]);
                    } else {
                        gallery_mesh
                            .positions
                            .push([p[0] + offset_x, p[1] + offset_y, p[2]]);
                    }
                }

                for n in item_mesh.normals {
                    if vertical {
                        gallery_mesh.normals.push([n[0], n[2], n[1]]);
                    } else {
                        gallery_mesh.normals.push(n);
                    }
                }

                gallery_mesh.uvs.extend(item_mesh.uvs);

                for idx in item_mesh.indices {
                    gallery_mesh.indices.push(idx + base_v_idx);
                }

                if let Some(quads) = item_mesh.quad_indices {
                    let g_quads = gallery_mesh.quad_indices.get_or_insert_with(Vec::new);
                    for q_idx in quads {
                        g_quads.push(q_idx + base_v_idx);
                    }
                }

                gallery_mesh.face_materials.extend(item_mesh.face_materials);
                gallery_mesh
                    .face_tint_indices
                    .extend(item_mesh.face_tint_indices);

                for t in textures {
                    if !all_textures.contains(&t) {
                        all_textures.push(t);
                    }
                }

                total_items += 1;
            }
        }

        (
            crate::mesh::PyMeshData::from_mesh(gallery_mesh),
            all_textures,
            total_items,
        )
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
                    stack
                        .inner
                        .open_asset_raw(&bs_path)
                        .map_err(|e| e.to_string())?
                        .and_then(|bytes| {
                            serde_json::from_slice::<BlockStateDefinition>(&bytes).ok()
                        })
                };

                // The parent-lookup callback cannot return an error, so the first read failure is
                // parked and surfaced instead of silently baking without that model.
                let read_failure: std::cell::RefCell<Option<String>> =
                    std::cell::RefCell::new(None);
                let baked = self
                    .inner
                    .bake_blockstate(state_str, bs_def.as_ref(), |model_id| {
                        match stack.inner.open_model_raw(model_id) {
                            Ok(Some(bytes)) => {
                                serde_json::from_slice::<BlockModelJson>(&bytes).ok()
                            }
                            Ok(None) => None,
                            Err(err) => {
                                read_failure
                                    .borrow_mut()
                                    .get_or_insert_with(|| err.to_string());
                                None
                            }
                        }
                    })
                    .map_err(|e| e.to_string())?;
                if let Some(err) = read_failure.into_inner() {
                    return Err(err);
                }

                let (mesh, textures) = baked.to_mesh_with_textures(clip_hidden);
                Ok((mesh, textures))
            })
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;

        Ok((PyMeshData::from_mesh(mesh), textures))
    }
}
