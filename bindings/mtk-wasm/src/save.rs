//! Browser Minecraft Java Edition save loader + dynamic block-entity skin/banner bridge.

use std::collections::HashMap;
use std::sync::Arc;

use glam::IVec3;
use js_sys::{Array, Reflect, Uint8Array};
use libmtk::AssetCacheReader;
use mtk_block_entity_textures::BlockEntityTextures;
use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_model::BakedModelDatabase;
use mtk_resource::CtmSolver;
use mtk_save::{LevelData, RegionBytes, SaveLoader};
use mtk_texture::{
    dynamic_chunk_texture_id, parse_dynamic_chunk_texture_id, AtlasAddressMap, RgbaBuffer,
    DYNAMIC_ATLAS_HEIGHT, DYNAMIC_ATLAS_WIDTH,
};
use mtk_voxel::{PendingSkinRequest, TextureResolveMode, VoxelStorage, VoxelWorld};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::cache::WasmAssetCache;
use crate::config::WasmMesherConfig;
use crate::render::{build_render_scene, WasmRenderScene};

#[derive(Serialize)]
struct LevelInspectPayload {
    level: LevelData,
    dimensions: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct LoadRegionsSummary {
    pub(crate) loaded_sections: usize,
    pub(crate) voxel_count: usize,
    pub(crate) bounds: [i32; 6],
}

struct RegionBlobOwned {
    rx: i32,
    rz: i32,
    data: Vec<u8>,
}

/// Interactive save-world session backed by an in-memory `.mtkcache` and browser-fed `.mca` regions.
#[wasm_bindgen]
pub struct WasmSaveWorld {
    cache_reader: Arc<AssetCacheReader>,
    cache_texture_names: Vec<String>,
    base_models: Arc<BakedModelDatabase>,
    base_atlas_mapping: Arc<AtlasAddressMap>,
    biome_resolver: Arc<BiomeResolver>,
    colormaps: Option<Arc<VanillaColormaps>>,
    ctm_solver: Option<Arc<CtmSolver>>,
    world: VoxelWorld,
    has_initial_mesh: bool,
    last_config: Option<WasmMesherConfig>,
}

#[wasm_bindgen]
impl WasmSaveWorld {
    /// Inspects `level.dat` bytes and discovers available dimensions from save-relative file paths.
    pub fn inspect_level(
        level_dat_bytes: &[u8],
        relative_paths: Vec<String>,
    ) -> Result<String, JsValue> {
        let level = LevelData::from_bytes(level_dat_bytes)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let dimensions =
            SaveLoader::discover_dimensions_from_paths(relative_paths.iter().map(String::as_str));
        serde_json::to_string(&LevelInspectPayload { level, dimensions })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Returns JSON `{ regions: [{rx, rz, path}], external_chunks: [{cx, cz, path}] }` for `[min, max]`.
    pub fn plan_region_files(
        dimension: &str,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
        relative_paths: Vec<String>,
    ) -> Result<String, JsValue> {
        let plan = SaveLoader::plan_dimension_region_files(
            dimension,
            IVec3::new(min_x, min_y, min_z),
            IVec3::new(max_x, max_y, max_z),
            relative_paths.iter().map(String::as_str),
        );
        serde_json::to_string(&plan).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Creates a new save-world builder bound to a loaded `.mtkcache`.
    #[wasm_bindgen(constructor)]
    pub fn new(cache: &WasmAssetCache) -> Result<WasmSaveWorld, JsValue> {
        let base_models = cache.models();
        let base_atlas_mapping = cache.atlas_mapping();
        let biome_resolver = cache.biome_resolver();
        let colormaps = cache.colormaps();
        let ctm_solver = cache.ctm_solver();
        let banner_baker = cache
            .load_banner_baker()
            .map_err(|e| JsValue::from_str(&e))?;

        let mut textures = BlockEntityTextures::default();
        textures.set_allow_network(false);
        textures.set_banner_baker(banner_baker);

        let default_cfg = WasmMesherConfig::default();
        let config = default_cfg.to_mesher_config(
            Some(Arc::clone(&base_atlas_mapping)),
            Some(Arc::clone(&biome_resolver)),
            colormaps.clone(),
            ctm_solver.clone(),
        );
        let culler = default_cfg.to_face_culler();
        let mut world = VoxelWorld::new(Some(config), Some(culler), Some(Arc::clone(&base_models)));
        world.set_block_entity_textures(Some(Box::new(textures)));

        Ok(Self {
            cache_reader: cache.reader(),
            cache_texture_names: cache.texture_names_ref().to_vec(),
            base_models,
            base_atlas_mapping,
            biome_resolver,
            colormaps,
            ctm_solver,
            world,
            has_initial_mesh: false,
            last_config: None,
        })
    }

    /// Loads caller-supplied `.mca` (and optional `.mcc`) byte blobs for `[min, max]`.
    ///
    /// - `regions_js`: JS array of `{ rx: number, rz: number, data: Uint8Array }`
    /// - `external_chunks_js`: optional/null JS array of `{ cx: number, cz: number, data: Uint8Array }`
    pub fn load_regions(
        &mut self,
        regions_js: JsValue,
        external_chunks_js: JsValue,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> Result<String, JsValue> {
        let regions = parse_region_blobs(&regions_js)?;
        let external_chunks = parse_external_chunks(&external_chunks_js)?;
        let region_refs: Vec<RegionBytes<'_>> = regions
            .iter()
            .map(|r| RegionBytes {
                rx: r.rx,
                rz: r.rz,
                data: &r.data,
            })
            .collect();

        let summary = self
            .load_regions_internal(
                &region_refs,
                &external_chunks,
                IVec3::new(min_x, min_y, min_z),
                IVec3::new(max_x, max_y, max_z),
            )
            .map_err(|e| JsValue::from_str(&e))?;

        serde_json::to_string(&summary).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Lists unresolved player-head skin requests discovered in the loaded selection,
    /// including canonical `candidate_urls` for browser `fetch`.
    pub fn pending_skin_requests_json(&self) -> Result<String, JsValue> {
        let requests = self.collect_pending_skin_requests();
        serde_json::to_string(&requests).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Registers a browser-fetched player skin PNG for `key` and marks matching skull sections dirty.
    ///
    /// Returns `false` if the PNG is invalid or matches a known mirror default-skin fallback.
    pub fn register_skin_png(&mut self, key: &str, png_bytes: &[u8]) -> Result<bool, JsValue> {
        Ok(self.register_skin_png_internal(key, png_bytes))
    }

    /// Lists all active static (`.mtkcache`) and dynamic (`dynamic/chunk_<id>.png`) texture IDs.
    pub fn texture_names(&self) -> Vec<String> {
        let mut names = self.cache_texture_names.clone();
        for (chunk_id, _) in self.world.dynamic_entity_atlases() {
            let chunk_name = dynamic_chunk_texture_id(chunk_id);
            if !names.contains(&chunk_name) {
                names.push(chunk_name);
            }
        }
        names
    }

    /// Reads PNG bytes for either a dynamic entity atlas chunk (`dynamic/chunk_<id>.png`) or a static cache chunk.
    pub fn read_texture_png(&self, chunk_id: &str) -> Result<Uint8Array, JsValue> {
        let png = self
            .read_texture_png_bytes_internal(chunk_id)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Uint8Array::from(png.as_slice()))
    }

    /// Prepares banner & skin dynamic textures and builds (or incrementally updates) the Three.js scene.
    ///
    /// Takes ownership of `config` when supplied.
    pub fn build_render_scene(
        &mut self,
        config: Option<WasmMesherConfig>,
    ) -> Result<WasmRenderScene, JsValue> {
        self.build_render_scene_internal(config.as_ref())
            .map_err(|e| JsValue::from_str(&e))
    }
}

impl WasmSaveWorld {
    #[cfg(test)]
    pub(crate) fn from_cache_for_test(cache: &WasmAssetCache) -> Result<Self, String> {
        Self::new(cache).map_err(|e| e.as_string().unwrap_or_else(|| "js error".to_string()))
    }

    pub(crate) fn load_regions_internal(
        &mut self,
        regions: &[RegionBytes<'_>],
        external_chunks: &HashMap<(i32, i32), Vec<u8>>,
        min_block: IVec3,
        max_block: IVec3,
    ) -> Result<LoadRegionsSummary, String> {
        self.world.clear_cache();
        self.world.storage = VoxelStorage::new();
        self.world.set_model_db(Some(Arc::clone(&self.base_models)));
        let mut fresh_config = self.world.config.clone();
        fresh_config.atlas_address_map = Some(Arc::clone(&self.base_atlas_mapping));
        self.world.set_config(fresh_config);
        self.has_initial_mesh = false;
        self.last_config = None;

        let loaded_sections = SaveLoader::load_box_from_regions_into_storage(
            regions,
            external_chunks,
            min_block,
            max_block,
            &mut self.world.storage,
            None,
        )
        .map_err(|e| e.to_string())?;

        self.world
            .prepare_block_entity_textures(None, TextureResolveMode::Cached, None);

        let voxel_count = self
            .world
            .storage
            .count_non_air_in_bounds(min_block, max_block);
        let (bx, by, bz, sx, sy, sz) = self.world.get_bounds();
        Ok(LoadRegionsSummary {
            loaded_sections,
            voxel_count,
            bounds: [bx, by, bz, sx, sy, sz],
        })
    }

    pub(crate) fn collect_pending_skin_requests(&self) -> Vec<PendingSkinRequest> {
        self.world.pending_skin_requests()
    }

    pub(crate) fn register_skin_png_internal(&mut self, key: &str, png_bytes: &[u8]) -> bool {
        self.world.register_memory_skin(key, png_bytes.to_vec())
    }

    pub(crate) fn read_texture_png_bytes_internal(
        &self,
        chunk_id: &str,
    ) -> Result<Vec<u8>, String> {
        if let Some(dyn_chunk_id) = parse_dynamic_chunk_texture_id(chunk_id) {
            if let Some((_, rgba)) = self
                .world
                .dynamic_entity_atlases()
                .into_iter()
                .find(|(cid, _)| *cid == dyn_chunk_id)
            {
                return RgbaBuffer::encode_png_bytes(
                    DYNAMIC_ATLAS_WIDTH,
                    DYNAMIC_ATLAS_HEIGHT,
                    rgba,
                )
                .map_err(|e| e.to_string());
            }
            return Err(format!("Dynamic atlas chunk {dyn_chunk_id} not allocated"));
        }
        self.cache_reader
            .read_texture_png(chunk_id)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn build_render_scene_internal(
        &mut self,
        config: Option<&WasmMesherConfig>,
    ) -> Result<WasmRenderScene, String> {
        let default_cfg = WasmMesherConfig::default();
        let effective = config.unwrap_or(&default_cfg);
        let config_changed = self.last_config.as_ref() != Some(effective);
        if config_changed {
            let current_atlas = self
                .world
                .config
                .atlas_address_map
                .clone()
                .unwrap_or_else(|| Arc::clone(&self.base_atlas_mapping));
            let next_config = effective.to_mesher_config(
                Some(current_atlas),
                Some(Arc::clone(&self.biome_resolver)),
                self.colormaps.clone(),
                self.ctm_solver.clone(),
            );
            self.world.culler = effective.to_face_culler();
            self.world.set_config(next_config);
            self.last_config = Some(effective.clone());
        }

        if !self.has_initial_mesh || config_changed {
            self.world.rebuild_all().map_err(|e| e.to_string())?;
            self.has_initial_mesh = true;
        } else if !self.world.storage.dirty_sections.is_empty() {
            self.world
                .rebuild_dirty_in_place()
                .map_err(|e| e.to_string())?;
            self.world.storage.clear_dirty_sections();
        }

        let mesh = self
            .world
            .get_world_mesh()
            .ok_or_else(|| "World mesh is empty".to_string())?;
        Ok(build_render_scene(
            mesh,
            self.world.config.atlas_address_map.as_deref(),
        ))
    }
}

fn read_i32_field(obj: &JsValue, field: &str) -> Result<i32, JsValue> {
    let val = Reflect::get(obj, &JsValue::from_str(field))?;
    let num = val
        .as_f64()
        .ok_or_else(|| JsValue::from_str(&format!("Expected integer field '{field}'")))?;
    Ok(num.trunc() as i32)
}

fn parse_region_blobs(regions_js: &JsValue) -> Result<Vec<RegionBlobOwned>, JsValue> {
    let arr = Array::from(regions_js);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        let rx = read_i32_field(&item, "rx")?;
        let rz = read_i32_field(&item, "rz")?;
        let data_val = Reflect::get(&item, &JsValue::from_str("data"))?;
        let data = Uint8Array::new(&data_val).to_vec();
        out.push(RegionBlobOwned { rx, rz, data });
    }
    Ok(out)
}

fn parse_external_chunks(external_js: &JsValue) -> Result<HashMap<(i32, i32), Vec<u8>>, JsValue> {
    let mut map = HashMap::new();
    if external_js.is_null() || external_js.is_undefined() {
        return Ok(map);
    }
    let arr = Array::from(external_js);
    for item in arr.iter() {
        let cx = read_i32_field(&item, "cx")?;
        let cz = read_i32_field(&item, "cz")?;
        let data_val = Reflect::get(&item, &JsValue::from_str("data"))?;
        let data = Uint8Array::new(&data_val).to_vec();
        map.insert((cx, cz), data);
    }
    Ok(map)
}
