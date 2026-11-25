//! `.mtkscene` package viewer and Three.js meshing bridge for WebAssembly.

use std::sync::Arc;

use js_sys::Uint8Array;
use libmtk::{PackageManifest, ScenePackageReader};
use mtk_cull::{FaceCuller, GlassCullMode, LeavesCullMode};
use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_model::BakedModelDatabase;
use mtk_resource::CtmSolver;
use mtk_texture::{AtlasAddressMap, AtlasChunkMeta};
use mtk_voxel::{CoordinateSystem, MesherConfig, VoxelWorld};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::render::{build_render_scene, WasmRenderScene};

#[derive(Serialize)]
struct WasmSceneMetadataPayload<'a> {
    manifest: &'a PackageManifest,
    texture_names: &'a [String],
    atlas_chunks: &'a [AtlasChunkMeta],
}

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

        let mut texture_names: Vec<String> = reader
            .inner_reader()
            .iter_chunk_identifiers(*b"ATLS")
            .filter(|id| id.starts_with("atlas/textures/"))
            .map(|id| id.to_string())
            .collect();
        texture_names.sort();

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
        let atlas_chunks: &[AtlasChunkMeta] = self
            .atlas_mapping
            .as_deref()
            .map(|m| m.chunks.as_slice())
            .unwrap_or(&[]);
        let payload = WasmSceneMetadataPayload {
            manifest: &self.metadata,
            texture_names: &self.texture_names,
            atlas_chunks,
        };
        serde_json::to_string(&payload).map_err(|e| JsValue::from_str(&e.to_string()))
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
    /// Uses `CoordinateSystem::YUpRightHanded` and `origin_centered = true`.
    /// Transient `VoxelStorage` / `VoxelWorld` allocations are dropped before returning.
    pub fn build_render_scene(
        &self,
        enable_ao: Option<bool>,
        mesh_fluids: Option<bool>,
        enable_ctm: Option<bool>,
        enable_animated_textures: Option<bool>,
    ) -> Result<WasmRenderScene, JsValue> {
        let sparse_voxels = self
            .reader
            .read_sparse_voxels()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let storage = sparse_voxels.reconstruct_storage();
        drop(sparse_voxels);

        let use_ctm = enable_ctm.unwrap_or(true);
        let config = MesherConfig {
            coordinate_system: CoordinateSystem::YUpRightHanded,
            origin_centered: true,
            weld_vertices: true,
            enable_ao: enable_ao.unwrap_or(true),
            mesh_fluids: mesh_fluids.unwrap_or(true),
            enable_animated_textures: enable_animated_textures.unwrap_or(true),
            atlas_address_map: self.atlas_mapping.clone(),
            biome_resolver: self.biome_resolver.clone(),
            colormaps: self.colormaps.clone(),
            ctm_solver: if use_ctm {
                self.ctm_solver.clone()
            } else {
                None
            },
            ..Default::default()
        };

        let culler = FaceCuller::new(LeavesCullMode::Fancy, GlassCullMode::Group);
        let mut world =
            VoxelWorld::from_storage(storage, Some(config), Some(culler), self.models.clone());
        let mesh = world
            .rebuild_all()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(build_render_scene(mesh, self.atlas_mapping.as_deref()))
    }
}
