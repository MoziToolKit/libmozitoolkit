//! Standalone Biome, Colormap, and Material attribute functions.

use std::sync::Arc;

use super::resolver::PyBiomeResolver;
use crate::buffer::create_owned_f32_memoryview;
use crate::voxel::PyVoxelStorage;
use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyMemoryView};

enum BorrowedU8Buffer {
    Py(PyBuffer<u8>),
    Owned(Vec<u8>),
}

impl BorrowedU8Buffer {
    fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<u8>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<u8>>()?))
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const u8, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

enum BorrowedF32Buffer {
    Py(PyBuffer<f32>),
    Owned(Vec<f32>),
}

impl BorrowedF32Buffer {
    fn extract(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(buf) = PyBuffer::<f32>::get(obj) {
            if buf.is_c_contiguous() {
                return Ok(Self::Py(buf));
            }
        }
        Ok(Self::Owned(obj.extract::<Vec<f32>>()?))
    }

    fn as_slice(&self) -> &[f32] {
        match self {
            Self::Py(buf) => unsafe {
                std::slice::from_raw_parts(buf.buf_ptr() as *const f32, buf.item_count())
            },
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

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
    /// Loads the embedded vanilla default colormaps.
    #[staticmethod]
    pub fn load_embedded() -> Self {
        Self {
            inner: Arc::new(mtk_material::VanillaColormaps::load_embedded()),
        }
    }

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

    /// Sample linear RGBA grass color at temperature and humidity.
    pub fn sample_grass_linear(&self, temperature: f32, humidity: f32) -> [f32; 4] {
        self.inner.sample_grass_linear(temperature, humidity)
    }

    /// Sample linear RGBA foliage color at temperature and humidity.
    pub fn sample_foliage_linear(&self, temperature: f32, humidity: f32) -> [f32; 4] {
        self.inner.sample_foliage_linear(temperature, humidity)
    }

    /// Sample linear RGBA dry foliage color at temperature and humidity.
    pub fn sample_dry_foliage_linear(&self, temperature: f32, humidity: f32) -> [f32; 4] {
        self.inner.sample_dry_foliage_linear(temperature, humidity)
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

/// Standalone batch function to compute mesh biome attributes in Rust parallel Rayon.
#[pyfunction]
#[pyo3(signature = (
    face_texture_keys,
    biome_name="PLAINS",
    multi_biomes=None,
    resolver=None,
    custom_temp=None,
    custom_humidity=None,
    custom_grass=None,
    custom_foliage=None,
    custom_dry_foliage=None,
    custom_water=None,
    has_custom_grass=false,
    has_custom_foliage=false,
    has_custom_dry_foliage=false,
    face_biomes=None,
    colormaps=None,
))]
pub fn compute_biome_tint_attributes(
    py: Python<'_>,
    face_texture_keys: Vec<String>,
    biome_name: &str,
    multi_biomes: Option<Vec<(String, f32)>>,
    resolver: Option<&PyBiomeResolver>,
    custom_temp: Option<f32>,
    custom_humidity: Option<f32>,
    custom_grass: Option<[f32; 4]>,
    custom_foliage: Option<[f32; 4]>,
    custom_dry_foliage: Option<[f32; 4]>,
    custom_water: Option<[f32; 4]>,
    has_custom_grass: bool,
    has_custom_foliage: bool,
    has_custom_dry_foliage: bool,
    face_biomes: Option<Vec<String>>,
    colormaps: Option<&PyVanillaColormaps>,
) -> PyResult<Py<PyAny>> {
    let default_resolver = mtk_material::BiomeResolver::new();
    let res_ref = resolver.map(|r| &r.inner).unwrap_or(&default_resolver);
    let cms_ref = colormaps.map(|c| c.inner.as_ref());

    let res = py.detach(|| {
        if let Some(ref biomes) = face_biomes {
            mtk_material::compute_mesh_biome_attributes_per_face(
                &face_texture_keys,
                biomes,
                res_ref,
            )
        } else if biome_name.eq_ignore_ascii_case("custom")
            || custom_temp.is_some()
            || custom_humidity.is_some()
            || custom_grass.is_some()
            || custom_foliage.is_some()
            || custom_water.is_some()
        {
            let settings = mtk_material::CustomBiomeSettings {
                temperature: custom_temp.unwrap_or(0.8),
                humidity: custom_humidity.unwrap_or(0.4),
                grass_color: custom_grass,
                foliage_color: custom_foliage,
                dry_foliage_color: custom_dry_foliage,
                water_color: custom_water,
                has_custom_grass,
                has_custom_foliage,
                has_custom_dry_foliage,
            };
            mtk_material::compute_mesh_biome_attributes_custom_with_colormaps(
                &face_texture_keys,
                &settings,
                res_ref,
                cms_ref,
            )
        } else {
            mtk_material::compute_mesh_biome_attributes_with_colormaps(
                &face_texture_keys,
                biome_name,
                multi_biomes.as_deref(),
                res_ref,
                cms_ref,
            )
        }
    });

    let dict = PyDict::new(py);
    dict.set_item("packed_tint_data", res.packed_tint_data)?;
    dict.set_item("tint_colors", res.tint_colors)?;
    Ok(dict.into())
}

/// Palette-native biome tint computation returning flat float memoryviews.
#[pyfunction]
#[pyo3(signature = (
    palette,
    face_indices,
    biome_name="PLAINS",
    multi_biomes=None,
    resolver=None,
    custom_temp=None,
    custom_humidity=None,
    custom_grass=None,
    custom_foliage=None,
    custom_dry_foliage=None,
    custom_water=None,
    has_custom_grass=false,
    has_custom_foliage=false,
    has_custom_dry_foliage=false,
    colormaps=None,
))]
pub fn compute_biome_tint_attributes_paletted<'py>(
    py: Python<'py>,
    palette: Vec<String>,
    face_indices: &Bound<'_, PyAny>,
    biome_name: &str,
    multi_biomes: Option<Vec<(String, f32)>>,
    resolver: Option<&PyBiomeResolver>,
    custom_temp: Option<f32>,
    custom_humidity: Option<f32>,
    custom_grass: Option<[f32; 4]>,
    custom_foliage: Option<[f32; 4]>,
    custom_dry_foliage: Option<[f32; 4]>,
    custom_water: Option<[f32; 4]>,
    has_custom_grass: bool,
    has_custom_foliage: bool,
    has_custom_dry_foliage: bool,
    colormaps: Option<&PyVanillaColormaps>,
) -> PyResult<(Bound<'py, PyMemoryView>, Bound<'py, PyMemoryView>)> {
    let indices = BorrowedU32Buffer::extract(face_indices)?;
    let default_resolver = mtk_material::BiomeResolver::new();
    let res_ref = resolver.map(|r| &r.inner).unwrap_or(&default_resolver);
    let cms_ref = colormaps.map(|c| c.inner.as_ref());

    let custom = if biome_name.eq_ignore_ascii_case("custom")
        || custom_temp.is_some()
        || custom_humidity.is_some()
        || custom_grass.is_some()
        || custom_foliage.is_some()
        || custom_dry_foliage.is_some()
        || custom_water.is_some()
    {
        Some(mtk_material::CustomBiomeSettings {
            temperature: custom_temp.unwrap_or(0.8),
            humidity: custom_humidity.unwrap_or(0.4),
            grass_color: custom_grass,
            foliage_color: custom_foliage,
            dry_foliage_color: custom_dry_foliage,
            water_color: custom_water,
            has_custom_grass,
            has_custom_foliage,
            has_custom_dry_foliage,
        })
    } else {
        None
    };

    let (colors, cg, cf, cdf) = mtk_material::resolve_biome_color_set_with_colormaps(
        biome_name,
        multi_biomes.as_deref(),
        custom.as_ref(),
        cms_ref,
    );
    let result = py.detach(|| {
        mtk_material::compute_mesh_biome_attributes_paletted_flat(
            &palette,
            indices.as_slice(),
            colors,
            cg,
            cf,
            cdf,
            res_ref,
        )
    });

    Ok((
        create_owned_f32_memoryview(py, result.packed_tint_data)?,
        create_owned_f32_memoryview(py, result.tint_colors)?,
    ))
}

/// Re-apply a uniform biome palette in-place-compatible flat float buffers.
///
/// Inputs must be C-contiguous buffers:
/// - `base_tint_types`: `uint8` per face (0=None, 1=Grass, 2=Foliage, 3=Water, 4=Hardcoded, 5=DryFoliage)
/// - `packed_tint_data`: `float32` x 4 per face
/// - `existing_tint_colors`: `float32` x 4 per face (empty buffer allowed; hardcoded faces then use white)
///
/// Returns two zero-copy memoryviews over Rust-owned buffers:
/// `(packed_tint_data, tint_colors)`.
#[pyfunction]
#[pyo3(signature = (
    base_tint_types,
    packed_tint_data,
    existing_tint_colors,
    grass_color,
    foliage_color,
    dry_foliage_color,
    water_color,
    has_custom_grass=false,
    has_custom_foliage=false,
    has_custom_dry_foliage=false,
))]
pub fn reapply_uniform_biome_tint<'py>(
    py: Python<'py>,
    base_tint_types: &Bound<'_, PyAny>,
    packed_tint_data: &Bound<'_, PyAny>,
    existing_tint_colors: &Bound<'_, PyAny>,
    grass_color: [f32; 4],
    foliage_color: [f32; 4],
    dry_foliage_color: [f32; 4],
    water_color: [f32; 4],
    has_custom_grass: bool,
    has_custom_foliage: bool,
    has_custom_dry_foliage: bool,
) -> PyResult<(Bound<'py, PyMemoryView>, Bound<'py, PyMemoryView>)> {
    let base = BorrowedU8Buffer::extract(base_tint_types)?;
    let packed = BorrowedF32Buffer::extract(packed_tint_data)?;
    let existing = BorrowedF32Buffer::extract(existing_tint_colors)?;

    let face_count = base.as_slice().len();
    let packed_slice = packed.as_slice();
    if packed_slice.len() < face_count * 4 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "packed_tint_data must contain at least face_count * 4 floats",
        ));
    }

    let packed_pairs: &[[f32; 4]] =
        unsafe { std::slice::from_raw_parts(packed_slice.as_ptr() as *const [f32; 4], face_count) };
    let existing_slice = existing.as_slice();
    let existing_pairs: &[[f32; 4]] = if existing_slice.len() >= face_count * 4 {
        unsafe {
            std::slice::from_raw_parts(existing_slice.as_ptr() as *const [f32; 4], face_count)
        }
    } else {
        &[]
    };

    let result = mtk_material::reapply_uniform_biome_tint_flat(
        base.as_slice(),
        packed_pairs,
        existing_pairs,
        mtk_material::BiomeColorSet {
            grass: grass_color,
            foliage: foliage_color,
            dry_foliage: dry_foliage_color,
            water: water_color,
        },
        has_custom_grass,
        has_custom_foliage,
        has_custom_dry_foliage,
    );

    let packed_mv = create_owned_f32_memoryview(py, result.packed_tint_data)?;
    let colors_mv = create_owned_f32_memoryview(py, result.tint_colors)?;
    Ok((packed_mv, colors_mv))
}

/// Compute 3D coordinate-aware biome tinting attributes using a VoxelStorage container.
#[pyfunction]
#[pyo3(signature = (
    face_texture_keys,
    block_xs,
    block_ys,
    block_zs,
    storage,
    resolver=None,
    smooth=true,
    colormaps=None,
))]
pub fn compute_mesh_biome_attributes_from_storage(
    py: Python<'_>,
    face_texture_keys: Vec<String>,
    block_xs: Vec<i32>,
    block_ys: Vec<i32>,
    block_zs: Vec<i32>,
    storage: &PyVoxelStorage,
    resolver: Option<&PyBiomeResolver>,
    smooth: bool,
    colormaps: Option<&PyVanillaColormaps>,
) -> PyResult<Py<PyAny>> {
    let face_count = face_texture_keys.len();
    let default_resolver = mtk_material::BiomeResolver::new();
    let res_ref = resolver.map(|r| &r.inner).unwrap_or(&default_resolver);
    let default_cms = mtk_material::get_default_colormaps();
    let effective_cms = colormaps.map(|c| c.inner.as_ref()).unwrap_or(&default_cms);

    let (packed_tint_data, tint_colors) = py.detach(|| {
        let mut packed = Vec::with_capacity(face_count);
        let mut cols = Vec::with_capacity(face_count);

        for i in 0..face_count {
            let key = &face_texture_keys[i];
            let bx = block_xs.get(i).copied().unwrap_or(0);
            let by = block_ys.get(i).copied().unwrap_or(0);
            let bz = block_zs.get(i).copied().unwrap_or(0);

            let (col, has_cg, has_cf, has_cdf) = if smooth {
                let sm = mtk_voxel::biome::get_smoothed_column_biome(
                    |x, y, z| storage.inner.get_biome(x, y, z),
                    bx,
                    by,
                    bz,
                );
                (
                    sm,
                    sm.has_custom_grass,
                    sm.has_custom_foliage,
                    sm.has_custom_dry_foliage,
                )
            } else {
                let b_name = storage.inner.get_biome(bx, by, bz);
                let sm = mtk_voxel::biome::get_point_column_biome(b_name, bx, bz);
                (
                    sm,
                    sm.has_custom_grass,
                    sm.has_custom_foliage,
                    sm.has_custom_dry_foliage,
                )
            };

            let tint_info = res_ref.get_tint_info(key, None, None);
            let tt = tint_info.tint_type;
            let (c, has_custom) = match tt {
                mtk_material::TINT_TYPE_GRASS => {
                    if has_cg {
                        (col.grass_color, true)
                    } else {
                        (
                            effective_cms
                                .sample_grass_linear_uv(col.colormap_uv[0], col.colormap_uv[1]),
                            false,
                        )
                    }
                }
                mtk_material::TINT_TYPE_FOLIAGE => {
                    if has_cf {
                        (col.foliage_color, true)
                    } else {
                        (
                            effective_cms
                                .sample_foliage_linear_uv(col.colormap_uv[0], col.colormap_uv[1]),
                            false,
                        )
                    }
                }
                mtk_material::TINT_TYPE_DRY_FOLIAGE => {
                    if has_cdf {
                        (col.dry_foliage_color, true)
                    } else {
                        (
                            effective_cms.sample_dry_foliage_linear_uv(
                                col.colormap_uv[0],
                                col.colormap_uv[1],
                            ),
                            false,
                        )
                    }
                }
                mtk_material::TINT_TYPE_WATER => (col.water_color, false),
                mtk_material::TINT_TYPE_HARDCODED => (
                    tint_info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0]),
                    true,
                ),
                _ => (
                    if tint_info.is_hardcoded {
                        tint_info.hardcoded_color.unwrap_or([1.0, 1.0, 1.0, 1.0])
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    },
                    false,
                ),
            };

            let tint_type_val = if tint_info.is_hardcoded || has_custom {
                mtk_material::TINT_TYPE_HARDCODED as f32
            } else {
                tt as f32
            };

            packed.push([
                tint_info.base_tint_weight,
                tint_info.overlay_tint_weight,
                tint_info.tint_weight,
                tint_type_val,
            ]);
            cols.push(c);
        }

        (packed, cols)
    });

    let dict = PyDict::new(py);
    dict.set_item("packed_tint_data", packed_tint_data)?;
    dict.set_item("tint_colors", tint_colors)?;
    Ok(dict.into())
}

/// Clears any cached resource-pack colormap overrides, falling back to embedded vanilla.
#[pyfunction]
pub fn clear_cached_colormaps() {
    mtk_material::biome::clear_cached_colormaps();
}
