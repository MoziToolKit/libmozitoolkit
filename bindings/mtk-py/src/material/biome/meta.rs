use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

/// Retrieve canonical metadata and colors for a single biome.
#[pyfunction]
#[pyo3(signature = (biome_name))]
pub fn get_biome_meta(py: Python<'_>, biome_name: &str) -> PyResult<Py<PyAny>> {
    let pal = mtk_material::get_biome_palette(biome_name);
    let dict = PyDict::new(py);
    dict.set_item("id", pal.id)?;
    dict.set_item("name", pal.name)?;
    dict.set_item("temperature", pal.temperature)?;
    dict.set_item("humidity", pal.humidity)?;
    dict.set_item("grass_hex", pal.grass_hex)?;
    dict.set_item("foliage_hex", pal.foliage_hex)?;
    dict.set_item("dry_foliage_hex", pal.dry_foliage_hex)?;
    dict.set_item("water_hex", pal.water_hex)?;
    dict.set_item("grass_linear", pal.grass_linear())?;
    dict.set_item("foliage_linear", pal.foliage_linear())?;
    dict.set_item("dry_foliage_linear", pal.dry_foliage_linear())?;
    dict.set_item("water_linear", pal.water_linear())?;
    dict.set_item("colormap_uv", pal.colormap_uv())?;
    dict.set_item("has_custom_grass", pal.has_custom_grass)?;
    dict.set_item("has_custom_foliage", pal.has_custom_foliage)?;
    dict.set_item("has_custom_dry_foliage", pal.has_custom_dry_foliage)?;
    Ok(dict.into())
}

/// Retrieve a list of all canonical vanilla biomes.
#[pyfunction]
pub fn get_all_biomes(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let list = pyo3::types::PyList::empty(py);
    for pal in mtk_material::CANONICAL_BIOMES {
        let dict = PyDict::new(py);
        dict.set_item("id", pal.id)?;
        dict.set_item("name", pal.name)?;
        dict.set_item("temperature", pal.temperature)?;
        dict.set_item("humidity", pal.humidity)?;
        dict.set_item("grass_hex", pal.grass_hex)?;
        dict.set_item("foliage_hex", pal.foliage_hex)?;
        dict.set_item("dry_foliage_hex", pal.dry_foliage_hex)?;
        dict.set_item("water_hex", pal.water_hex)?;
        dict.set_item("grass_linear", pal.grass_linear())?;
        dict.set_item("foliage_linear", pal.foliage_linear())?;
        dict.set_item("dry_foliage_linear", pal.dry_foliage_linear())?;
        dict.set_item("water_linear", pal.water_linear())?;
        dict.set_item("colormap_uv", pal.colormap_uv())?;
        dict.set_item("has_custom_grass", pal.has_custom_grass)?;
        dict.set_item("has_custom_foliage", pal.has_custom_foliage)?;
        dict.set_item("has_custom_dry_foliage", pal.has_custom_dry_foliage)?;
        list.append(dict)?;
    }
    Ok(list.into())
}

/// Native container for resident Minecraft vanilla/resource-pack colormaps.
#[pyclass(name = "VanillaColormaps")]
#[derive(Clone)]
pub struct PyVanillaColormaps {
    pub(crate) inner: Arc<mtk_material::VanillaColormaps>,
}

#[pymethods]
impl PyVanillaColormaps {
    /// Constructs colormaps with optional PNG byte overrides for grass, foliage, and dry_foliage.
    #[staticmethod]
    #[pyo3(signature = (grass=None, foliage=None, dry_foliage=None))]
    pub fn from_bytes(
        grass: Option<&[u8]>,
        foliage: Option<&[u8]>,
        dry_foliage: Option<&[u8]>,
    ) -> PyResult<Self> {
        let cms = mtk_material::VanillaColormaps::with_overrides(grass, foliage, dry_foliage)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(Self {
            inner: Arc::new(cms),
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "<VanillaColormaps grass={}x{} foliage={}x{} dry_foliage={}x{}>",
            self.inner.grass_width,
            self.inner.grass_height,
            self.inner.foliage_width,
            self.inner.foliage_height,
            self.inner.dry_foliage_width,
            self.inner.dry_foliage_height
        )
    }
}

/// Sample resident vanilla colormap linear RGBA colors and hex strings from temperature and humidity.
#[pyfunction]
#[pyo3(signature = (temperature, humidity, colormaps=None))]
pub fn sample_colormap_colors(
    py: Python<'_>,
    temperature: f32,
    humidity: f32,
    colormaps: Option<&PyVanillaColormaps>,
) -> PyResult<Py<PyAny>> {
    let cms = colormaps
        .map(|c| c.inner.clone())
        .unwrap_or_else(mtk_material::get_default_colormaps);
    let dict = PyDict::new(py);
    dict.set_item(
        "grass_linear",
        cms.sample_grass_linear(temperature, humidity),
    )?;
    dict.set_item(
        "foliage_linear",
        cms.sample_foliage_linear(temperature, humidity),
    )?;
    dict.set_item(
        "dry_foliage_linear",
        cms.sample_dry_foliage_linear(temperature, humidity),
    )?;
    dict.set_item("grass_srgb", cms.sample_grass_srgb(temperature, humidity))?;
    dict.set_item(
        "foliage_srgb",
        cms.sample_foliage_srgb(temperature, humidity),
    )?;
    dict.set_item(
        "dry_foliage_srgb",
        cms.sample_dry_foliage_srgb(temperature, humidity),
    )?;
    dict.set_item("grass_hex", cms.sample_grass_hex(temperature, humidity))?;
    dict.set_item("foliage_hex", cms.sample_foliage_hex(temperature, humidity))?;
    dict.set_item(
        "dry_foliage_hex",
        cms.sample_dry_foliage_hex(temperature, humidity),
    )?;
    Ok(dict.into())
}
