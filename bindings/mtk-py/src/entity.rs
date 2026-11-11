//! Visual Entity and Entity Render Plan Bindings for mtk-py.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::mesh::PyMeshData;
use crate::texture::PyBakedAtlas;
use crate::voxel::PyMesherConfig;

/// Host-space rigid transform for an entity node.
#[pyclass(name = "EntityRenderTransform", module = "libmtk_py")]
#[derive(Clone)]
pub struct PyEntityRenderTransform {
    pub(crate) inner: mtk_model::EntityRenderTransform,
}

#[pymethods]
impl PyEntityRenderTransform {
    #[getter]
    pub fn location(&self) -> (f32, f32, f32) {
        let l = self.inner.location;
        (l[0], l[1], l[2])
    }

    #[getter]
    pub fn rotation_euler(&self) -> (f32, f32, f32) {
        let r = self.inner.rotation_euler;
        (r[0], r[1], r[2])
    }

    #[getter]
    pub fn scale(&self) -> (f32, f32, f32) {
        let s = self.inner.scale;
        (s[0], s[1], s[2])
    }

    #[getter]
    pub fn matrix<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.inner.matrix)
    }

    fn __repr__(&self) -> String {
        format!(
            "EntityRenderTransform(loc={:?}, rot={:?}, scale={:?})",
            self.inner.location, self.inner.rotation_euler, self.inner.scale
        )
    }
}

/// One entity object that the host should create or update.
#[pyclass(name = "EntityRenderNode", module = "libmtk_py")]
#[derive(Clone)]
pub struct PyEntityRenderNode {
    pub(crate) inner: mtk_model::EntityRenderNode,
}

#[pymethods]
impl PyEntityRenderNode {
    #[getter]
    pub fn key(&self) -> &str {
        &self.inner.key
    }

    #[getter]
    pub fn display_name(&self) -> &str {
        &self.inner.display_name
    }

    #[getter]
    pub fn transform(&self) -> PyEntityRenderTransform {
        PyEntityRenderTransform {
            inner: self.inner.transform,
        }
    }

    #[getter]
    pub fn location(&self) -> (f32, f32, f32) {
        let l = self.inner.transform.location;
        (l[0], l[1], l[2])
    }

    #[getter]
    pub fn rotation_euler(&self) -> (f32, f32, f32) {
        let r = self.inner.transform.rotation_euler;
        (r[0], r[1], r[2])
    }

    #[getter]
    pub fn scale(&self) -> (f32, f32, f32) {
        let s = self.inner.transform.scale;
        (s[0], s[1], s[2])
    }

    #[getter]
    pub fn matrix<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.inner.transform.matrix)
    }

    #[getter]
    pub fn mesh(&self) -> PyMeshData {
        PyMeshData::from_mesh(self.inner.local_mesh.clone())
    }

    #[getter]
    pub fn material_routes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for route in &self.inner.material_routes {
            let dict = PyDict::new(py);
            dict.set_item("texture_key", &route.texture_key)?;
            dict.set_item("material_slot", route.material_slot)?;
            dict.set_item("atlas_chunk_id", route.atlas_chunk_id)?;
            dict.set_item("atlas_texture_id", route.atlas_texture_id)?;
            list.append(dict)?;
        }
        Ok(list)
    }

    #[getter]
    pub fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.metadata {
            dict.set_item(k, v)?;
        }
        Ok(dict)
    }

    #[getter]
    pub fn entity_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.entity).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Failed to serialize entity: {e}"))
        })
    }

    #[getter]
    pub fn entity_id(&self) -> &str {
        &self.inner.entity.id
    }

    #[getter]
    pub fn uuid(&self) -> Option<&str> {
        self.inner.entity.uuid.as_deref()
    }

    #[getter]
    pub fn custom_name(&self) -> Option<&str> {
        self.inner.entity.custom_name.as_deref()
    }

    #[getter]
    pub fn facing(&self) -> Option<&str> {
        self.inner.entity.facing.map(|d| d.as_str())
    }

    #[getter]
    pub fn variant(&self) -> Option<&str> {
        match &self.inner.entity.kind {
            mtk_model::EntityKindData::Painting(p) => Some(p.variant.as_str()),
            _ => None,
        }
    }

    #[getter]
    pub fn dimensions(&self) -> Option<(u8, u8)> {
        match &self.inner.entity.kind {
            mtk_model::EntityKindData::Painting(p) => Some((p.width, p.height)),
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "EntityRenderNode(key='{}', name='{}', loc={:?}, rot={:?})",
            self.inner.key,
            self.inner.display_name,
            self.inner.transform.location,
            self.inner.transform.rotation_euler,
        )
    }
}

/// Desired set of entity render nodes for a scene revision.
#[pyclass(name = "EntityRenderPlan", module = "libmtk_py")]
#[derive(Clone)]
pub struct PyEntityRenderPlan {
    pub(crate) inner: mtk_model::EntityRenderPlan,
}

#[pymethods]
impl PyEntityRenderPlan {
    #[getter]
    pub fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }

    #[getter]
    pub fn revision(&self) -> u64 {
        self.inner.revision
    }

    #[getter]
    pub fn authoritative(&self) -> bool {
        self.inner.authoritative
    }

    #[getter]
    pub fn bounds(&self) -> Option<[i32; 6]> {
        self.inner.bounds
    }

    #[getter]
    pub fn node_count(&self) -> usize {
        self.inner.nodes.len()
    }

    #[getter]
    pub fn nodes(&self) -> Vec<PyEntityRenderNode> {
        self.inner
            .nodes
            .iter()
            .cloned()
            .map(|node| PyEntityRenderNode { inner: node })
            .collect()
    }

    #[getter]
    pub fn warnings<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for warn in &self.inner.warnings {
            let dict = PyDict::new(py);
            dict.set_item("key", &warn.key)?;
            dict.set_item("code", &warn.code)?;
            dict.set_item("message", &warn.message)?;
            list.append(dict)?;
        }
        Ok(list)
    }

    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Failed to serialize render plan: {e}"))
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "EntityRenderPlan(rev={}, nodes={}, warnings={})",
            self.inner.revision,
            self.inner.nodes.len(),
            self.inner.warnings.len(),
        )
    }
}

pub(crate) fn build_plan_from_entities_with_map(
    entities: &[mtk_model::EntityInstance],
    map: Option<&mtk_texture::AtlasAddressMap>,
    config: Option<&PyMesherConfig>,
    explicit_bounds: Option<[i32; 6]>,
    revision: u64,
    authoritative: bool,
) -> PyResult<PyEntityRenderPlan> {
    let active_config = config.map(|c| c.inner.clone()).unwrap_or_default();
    let custom_aliases = active_config.custom_aliases.as_deref();

    let eff_bounds = explicit_bounds.or_else(|| {
        active_config
            .selection_bounds
            .map(|(min, size)| [min[0], min[1], min[2], size[0], size[1], size[2]])
    });

    let render_config = mtk_model::EntityRenderConfig {
        revision,
        authoritative,
        bounds: eff_bounds,
        origin_centered: active_config.origin_centered,
        coordinate_system: active_config.coordinate_system,
    };

    let plan = mtk_model::build_entity_render_plan(entities, render_config, |tex| {
        if let Some(m) = map {
            let loc = mtk_material::MaterialResolver::resolve(tex, custom_aliases, m)
                .map(|(_, s)| s)
                .or_else(|| m.lookup_fallback())?;
            Some((loc.uv_bounds, loc.chunk_id, loc.texture_id))
        } else {
            None
        }
    });

    Ok(PyEntityRenderPlan { inner: plan })
}

pub(crate) fn build_plan_from_entities(
    entities: &[mtk_model::EntityInstance],
    atlas: Option<&PyBakedAtlas>,
    config: Option<&PyMesherConfig>,
    atlas_mapping_json: Option<&str>,
    explicit_bounds: Option<[i32; 6]>,
    revision: u64,
    authoritative: bool,
) -> PyResult<PyEntityRenderPlan> {
    let active_config = config.map(|c| c.inner.clone()).unwrap_or_default();

    let owned_map: Option<mtk_texture::AtlasAddressMap> = if let Some(json) = atlas_mapping_json {
        Some(mtk_texture::AtlasAddressMap::from_json(json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid atlas mapping JSON: {e}"))
        })?)
    } else {
        None
    };

    let map_ref = if let Some(ref m) = owned_map {
        Some(m)
    } else if let Some(a) = atlas {
        Some(&a.inner.address_map)
    } else {
        active_config.atlas_address_map.as_deref()
    };

    build_plan_from_entities_with_map(
        entities,
        map_ref,
        config,
        explicit_bounds,
        revision,
        authoritative,
    )
}

/// Bakes and compiles a batch of visual entities into a unified `EntityRenderPlan`.
#[pyfunction]
#[pyo3(signature = (
    entities,
    atlas=None,
    config=None,
    atlas_mapping_json=None,
    bounds=None,
    origin_centered=None,
    coordinate_system=None,
    revision=0,
    authoritative=true
))]
pub fn build_entity_render_plan(
    entities: &Bound<'_, PyAny>,
    atlas: Option<&PyBakedAtlas>,
    config: Option<&PyMesherConfig>,
    atlas_mapping_json: Option<&str>,
    bounds: Option<[i32; 6]>,
    origin_centered: Option<bool>,
    coordinate_system: Option<&str>,
    revision: u64,
    authoritative: bool,
) -> PyResult<PyEntityRenderPlan> {
    let mut parsed_entities: Vec<mtk_model::EntityInstance> = Vec::new();
    let mut parse_warnings: Vec<mtk_model::EntityRenderWarning> = Vec::new();
    let py = entities.py();

    if let Ok(list) = entities.cast::<PyList>() {
        for (idx, item) in list.iter().enumerate() {
            if let Ok(s) = item.extract::<String>() {
                match serde_json::from_str::<mtk_model::EntityInstance>(&s) {
                    Ok(ent) => parsed_entities.push(ent),
                    Err(e) => {
                        parse_warnings.push(mtk_model::EntityRenderWarning {
                            key: format!("invalid_entity_{idx}"),
                            code: "invalid_entity".to_string(),
                            message: format!("Invalid entity JSON: {e}"),
                        });
                    }
                }
            } else if let Ok(dict) = item.cast::<PyDict>() {
                let json_mod = py.import("json")?;
                let json_str: String = json_mod.call_method1("dumps", (dict,))?.extract()?;
                match serde_json::from_str::<mtk_model::EntityInstance>(&json_str) {
                    Ok(ent) => parsed_entities.push(ent),
                    Err(e) => {
                        parse_warnings.push(mtk_model::EntityRenderWarning {
                            key: format!("invalid_entity_{idx}"),
                            code: "invalid_entity".to_string(),
                            message: format!("Invalid entity dict: {e}"),
                        });
                    }
                }
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "entities list items must be JSON strings or dicts",
                ));
            }
        }
    } else if let Ok(s) = entities.extract::<String>() {
        parsed_entities = serde_json::from_str(&s).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid entity JSON array: {e}"))
        })?;
    } else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "entities must be a list of JSON strings/dicts or a JSON array string",
        ));
    }

    let mut active_config = config.map(|c| c.inner.clone()).unwrap_or_default();
    if let Some(oc) = origin_centered {
        active_config.origin_centered = oc;
    }
    if let Some(cs) = coordinate_system {
        active_config.coordinate_system = match cs.to_ascii_lowercase().as_str() {
            "minecraft" => mtk_core::CoordinateSystem::Minecraft,
            "y_up" => mtk_core::CoordinateSystem::YUpRightHanded,
            "z_up" => mtk_core::CoordinateSystem::ZUpRightHanded,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid coordinate_system '{}', expected 'z_up', 'minecraft', or 'y_up'",
                    other
                )));
            }
        };
    }

    let active_cfg_py = PyMesherConfig {
        inner: active_config,
        num_threads: None,
    };
    let mut plan = build_plan_from_entities(
        &parsed_entities,
        atlas,
        Some(&active_cfg_py),
        atlas_mapping_json,
        bounds,
        revision,
        authoritative,
    )?;
    plan.inner.warnings.extend(parse_warnings);
    Ok(plan)
}

/// Inverts host transform (location, rotation_euler, scale) back into canonical Minecraft world coordinates.
/// Returns `((pos_x, pos_y, pos_z), (rot_pitch, rot_yaw, rot_roll), (scale_x, scale_y, scale_z))`.
#[pyfunction]
#[pyo3(signature = (
    location,
    rotation_euler,
    scale,
    bounds=None,
    origin_centered=true,
    coordinate_system="z_up"
))]
pub fn host_transform_to_mc(
    location: (f32, f32, f32),
    rotation_euler: (f32, f32, f32),
    scale: (f32, f32, f32),
    bounds: Option<[i32; 6]>,
    origin_centered: bool,
    coordinate_system: Option<&str>,
) -> PyResult<((f64, f64, f64), (f32, f32, f32), (f32, f32, f32))> {
    let cs = match coordinate_system
        .unwrap_or("z_up")
        .to_ascii_lowercase()
        .as_str()
    {
        "minecraft" => mtk_core::CoordinateSystem::Minecraft,
        "y_up" => mtk_core::CoordinateSystem::YUpRightHanded,
        "z_up" => mtk_core::CoordinateSystem::ZUpRightHanded,
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid coordinate_system '{}', expected 'z_up', 'minecraft', or 'y_up'",
                other
            )));
        }
    };

    let config = mtk_model::EntityRenderConfig {
        revision: 0,
        authoritative: true,
        bounds,
        origin_centered,
        coordinate_system: cs,
    };

    let (pos, rot, s) = mtk_model::host_transform_to_mc(
        [location.0, location.1, location.2],
        [rotation_euler.0, rotation_euler.1, rotation_euler.2],
        [scale.0, scale.1, scale.2],
        &config,
    );

    Ok((
        (pos.x, pos.y, pos.z),
        (rot.x, rot.y, rot.z),
        (s.x, s.y, s.z),
    ))
}
