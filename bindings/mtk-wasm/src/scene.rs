//! `.mtkscene` package viewer and Three.js meshing bridge for WebAssembly.

use std::sync::Arc;

use js_sys::Uint8Array;
use libmtk::{PackageManifest, ScenePackageReader};
use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_model::BakedModelDatabase;
use mtk_resource::CtmSolver;
use mtk_texture::AtlasAddressMap;
use mtk_voxel::VoxelWorld;
use wasm_bindgen::prelude::*;

use crate::config::WasmMesherConfig;
use crate::render::{build_render_scene, serialize_metadata_json, WasmRenderScene};

/// Self-contained `.mtkscene` viewer handle backed by in-memory package bytes.
///
/// Texture PNGs stay compressed inside the container and are decoded asynchronously by
/// the browser via `read_texture_png()` + `Blob` / `createImageBitmap`, keeping WASM
/// linear memory minimal.
#[wasm_bindgen]
pub struct WasmScenePackage {
    reader: ScenePackageReader,
    metadata: PackageManifest,
    texture_names: Vec<String>,
    models: Option<Arc<BakedModelDatabase>>,
    atlas_mapping: Option<Arc<AtlasAddressMap>>,
    biome_resolver: Option<Arc<BiomeResolver>>,
    colormaps: Option<Arc<VanillaColormaps>>,
    ctm_solver: Option<Arc<CtmSolver>>,
}

#[wasm_bindgen]
impl WasmScenePackage {
    /// Parses an `.mtkscene` file from a browser `Uint8Array` without touching the filesystem.
    pub fn from_bytes(bytes: &[u8]) -> Result<WasmScenePackage, JsValue> {
        let reader = ScenePackageReader::from_bytes(bytes.to_vec())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let metadata = reader.effective_metadata();
        let models = reader
            .read_models()
            .map_err(|e| JsValue::from_str(&e.to_string()))?
            .map(Arc::new);
        let atlas_mapping = reader
            .read_atlas_mapping()
            .map_err(|e| JsValue::from_str(&e.to_string()))?
            .map(Arc::new);
        let biome_resolver = reader
            .read_biome_resolver()
            .map_err(|e| JsValue::from_str(&e.to_string()))?
            .map(Arc::new);
        let colormaps = reader
            .read_colormaps()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let ctm_solver = reader
            .read_ctm_solver()
            .map_err(|e| JsValue::from_str(&e.to_string()))?
            .map(Arc::new);

        let texture_names = reader.texture_names();

        Ok(Self {
            reader,
            metadata,
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
            &self.metadata,
            &self.texture_names,
            self.atlas_mapping.as_deref(),
        )
    }

    /// Returns all bundled atlas texture chunk identifiers (e.g. `"atlas/textures/blocks_chunk_001.png"`).
    pub fn texture_names(&self) -> Vec<String> {
        self.texture_names.clone()
    }

    pub(crate) fn read_texture_png_bytes(&self, chunk_id: &str) -> Result<Vec<u8>, String> {
        self.reader
            .read_texture_png(chunk_id)
            .map_err(|e| e.to_string())
    }

    /// Reads raw compressed PNG bytes for an atlas texture chunk without decoding in WASM.
    pub fn read_texture_png(&self, chunk_id: &str) -> Result<Uint8Array, JsValue> {
        let png = self
            .read_texture_png_bytes(chunk_id)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Uint8Array::from(png.as_slice()))
    }

    /// Reconstructs the sparse voxel world and emits Three.js-ready render batches.
    ///
    /// Transient `VoxelStorage` / `VoxelWorld` allocations are dropped before returning.
    /// Takes ownership of `config` when supplied.
    pub fn build_render_scene(
        &self,
        config: Option<WasmMesherConfig>,
    ) -> Result<WasmRenderScene, JsValue> {
        let sparse_voxels = self
            .reader
            .read_sparse_voxels()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let storage = sparse_voxels.reconstruct_storage();
        drop(sparse_voxels);

        let cfg = config.unwrap_or_default();
        let mesher_config = cfg.to_mesher_config(
            self.atlas_mapping.clone(),
            self.biome_resolver.clone(),
            self.colormaps.clone(),
            self.ctm_solver.clone(),
        );
        let culler = cfg.to_face_culler();

        let mut world = VoxelWorld::from_storage(
            storage,
            Some(mesher_config),
            Some(culler),
            self.models.clone(),
        );
        let mesh = world
            .rebuild_all()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(build_render_scene(
            mesh,
            self.atlas_mapping.as_deref(),
            cfg.weld_mode_value(),
        ))
    }
}
