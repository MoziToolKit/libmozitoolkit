//! In-memory `.mtkcache` loader for WebAssembly save & modded world rendering.

use std::sync::Arc;

use js_sys::Uint8Array;
use libmtk::{AssetCacheReader, PackageManifest};
use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_model::BakedModelDatabase;
use mtk_resource::CtmSolver;
use mtk_texture::{AtlasAddressMap, BannerBaker};
use wasm_bindgen::prelude::*;

use crate::render::serialize_metadata_json;

/// Precompiled `.mtkcache` asset cache handle held in WASM linear memory.
///
/// Atlas texture PNGs stay compressed inside the container and are read on demand
/// via `read_texture_png()` for asynchronous browser `createImageBitmap` decoding.
#[wasm_bindgen]
pub struct WasmAssetCache {
    reader: Arc<AssetCacheReader>,
    manifest: PackageManifest,
    texture_names: Vec<String>,
    models: Arc<BakedModelDatabase>,
    atlas_mapping: Arc<AtlasAddressMap>,
    biome_resolver: Arc<BiomeResolver>,
    colormaps: Option<Arc<VanillaColormaps>>,
    ctm_solver: Option<Arc<CtmSolver>>,
}

#[wasm_bindgen]
impl WasmAssetCache {
    /// Parses a `.mtkcache` file from a browser `Uint8Array` (e.g. CDN fetch or user upload).
    pub fn from_bytes(bytes: &[u8]) -> Result<WasmAssetCache, JsValue> {
        let reader = AssetCacheReader::from_bytes(bytes.to_vec())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let manifest = reader
            .manifest()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let texture_names = reader.texture_names();
        let models = Arc::new(
            reader
                .load_models()
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
        );
        let atlas_mapping = Arc::new(
            reader
                .load_atlas_mapping()
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
        );
        let biome_resolver = Arc::new(
            reader
                .load_biome_resolver()
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
        );
        let colormaps = reader
            .load_colormaps()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let ctm_solver = reader
            .load_ctm_solver()
            .map_err(|e| JsValue::from_str(&e.to_string()))?
            .map(Arc::new);

        Ok(Self {
            reader: Arc::new(reader),
            manifest,
            texture_names,
            models,
            atlas_mapping,
            biome_resolver,
            colormaps,
            ctm_solver,
        })
    }

    /// Returns manifest, texture chunk names, and atlas chunk descriptors as JSON.
    pub fn metadata_json(&self) -> Result<String, JsValue> {
        serialize_metadata_json(
            &self.manifest,
            &self.texture_names,
            Some(self.atlas_mapping.as_ref()),
        )
    }

    /// Returns all bundled atlas texture chunk identifiers.
    pub fn texture_names(&self) -> Vec<String> {
        self.texture_names.clone()
    }

    /// Reads raw compressed PNG bytes for an atlas texture chunk without decoding in WASM.
    pub fn read_texture_png(&self, chunk_id: &str) -> Result<Uint8Array, JsValue> {
        let png = self
            .read_texture_png_bytes(chunk_id)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Uint8Array::from(png.as_slice()))
    }
}

impl WasmAssetCache {
    pub(crate) fn reader(&self) -> Arc<AssetCacheReader> {
        Arc::clone(&self.reader)
    }

    pub(crate) fn read_texture_png_bytes(&self, chunk_id: &str) -> Result<Vec<u8>, String> {
        self.reader
            .read_texture_png(chunk_id)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn texture_names_ref(&self) -> &[String] {
        &self.texture_names
    }

    pub(crate) fn models(&self) -> Arc<BakedModelDatabase> {
        Arc::clone(&self.models)
    }

    pub(crate) fn atlas_mapping(&self) -> Arc<AtlasAddressMap> {
        Arc::clone(&self.atlas_mapping)
    }

    pub(crate) fn biome_resolver(&self) -> Arc<BiomeResolver> {
        Arc::clone(&self.biome_resolver)
    }

    pub(crate) fn colormaps(&self) -> Option<Arc<VanillaColormaps>> {
        self.colormaps.clone()
    }

    pub(crate) fn ctm_solver(&self) -> Option<Arc<CtmSolver>> {
        self.ctm_solver.clone()
    }

    pub(crate) fn load_banner_baker(&self) -> Result<BannerBaker, String> {
        self.reader.load_banner_baker().map_err(|e| e.to_string())
    }
}
