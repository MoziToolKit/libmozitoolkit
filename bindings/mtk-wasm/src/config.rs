//! Strong-typed mesher & face-culling configuration DTO for WebAssembly callers.

use std::sync::Arc;

use mtk_cull::{FaceCuller, GlassCullMode, LeavesCullMode};
use mtk_material::{BiomeResolver, VanillaColormaps};
use mtk_resource::CtmSolver;
use mtk_texture::AtlasAddressMap;
use mtk_voxel::{CoordinateSystem, MesherConfig, UnknownBlockStrategy};
use wasm_bindgen::prelude::*;

/// Explicit meshing and culling configuration for `WasmScenePackage` and `WasmSaveWorld`.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmMesherConfig {
    enable_ao: bool,
    mesh_fluids: bool,
    enable_ctm: bool,
    enable_animated_textures: bool,
    smooth_biome_blend: bool,
    origin_centered: bool,
    weld_vertices: bool,
    exclude_hidden_volume: bool,
    enable_alternate_blocks: bool,
    enable_random_offsets: bool,
    coordinate_system: CoordinateSystem,
    leaves_cull_mode: LeavesCullMode,
    glass_cull_mode: GlassCullMode,
    unknown_block_strategy: UnknownBlockStrategy,
}

impl Default for WasmMesherConfig {
    fn default() -> Self {
        Self {
            enable_ao: true,
            mesh_fluids: true,
            enable_ctm: true,
            enable_animated_textures: true,
            smooth_biome_blend: true,
            origin_centered: true,
            weld_vertices: false,
            exclude_hidden_volume: true,
            enable_alternate_blocks: true,
            enable_random_offsets: true,
            coordinate_system: CoordinateSystem::YUpRightHanded,
            leaves_cull_mode: LeavesCullMode::Fancy,
            glass_cull_mode: GlassCullMode::Group,
            unknown_block_strategy: UnknownBlockStrategy::FallbackCube,
        }
    }
}

#[wasm_bindgen]
impl WasmMesherConfig {
    /// Creates a new configuration with Web/Three.js defaults (`YUpRightHanded`, AO/fluids/CTM/anim enabled).
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(getter)]
    pub fn enable_ao(&self) -> bool {
        self.enable_ao
    }
    #[wasm_bindgen(setter)]
    pub fn set_enable_ao(&mut self, value: bool) {
        self.enable_ao = value;
    }

    #[wasm_bindgen(getter)]
    pub fn mesh_fluids(&self) -> bool {
        self.mesh_fluids
    }
    #[wasm_bindgen(setter)]
    pub fn set_mesh_fluids(&mut self, value: bool) {
        self.mesh_fluids = value;
    }

    #[wasm_bindgen(getter)]
    pub fn enable_ctm(&self) -> bool {
        self.enable_ctm
    }
    #[wasm_bindgen(setter)]
    pub fn set_enable_ctm(&mut self, value: bool) {
        self.enable_ctm = value;
    }

    #[wasm_bindgen(getter)]
    pub fn enable_animated_textures(&self) -> bool {
        self.enable_animated_textures
    }
    #[wasm_bindgen(setter)]
    pub fn set_enable_animated_textures(&mut self, value: bool) {
        self.enable_animated_textures = value;
    }

    #[wasm_bindgen(getter)]
    pub fn smooth_biome_blend(&self) -> bool {
        self.smooth_biome_blend
    }
    #[wasm_bindgen(setter)]
    pub fn set_smooth_biome_blend(&mut self, value: bool) {
        self.smooth_biome_blend = value;
    }

    #[wasm_bindgen(getter)]
    pub fn origin_centered(&self) -> bool {
        self.origin_centered
    }
    #[wasm_bindgen(setter)]
    pub fn set_origin_centered(&mut self, value: bool) {
        self.origin_centered = value;
    }

    #[wasm_bindgen(getter)]
    pub fn weld_vertices(&self) -> bool {
        self.weld_vertices
    }
    #[wasm_bindgen(setter)]
    pub fn set_weld_vertices(&mut self, value: bool) {
        self.weld_vertices = value;
    }

    #[wasm_bindgen(getter)]
    pub fn exclude_hidden_volume(&self) -> bool {
        self.exclude_hidden_volume
    }
    #[wasm_bindgen(setter)]
    pub fn set_exclude_hidden_volume(&mut self, value: bool) {
        self.exclude_hidden_volume = value;
    }

    #[wasm_bindgen(getter)]
    pub fn enable_alternate_blocks(&self) -> bool {
        self.enable_alternate_blocks
    }
    #[wasm_bindgen(setter)]
    pub fn set_enable_alternate_blocks(&mut self, value: bool) {
        self.enable_alternate_blocks = value;
    }

    #[wasm_bindgen(getter)]
    pub fn enable_random_offsets(&self) -> bool {
        self.enable_random_offsets
    }
    #[wasm_bindgen(setter)]
    pub fn set_enable_random_offsets(&mut self, value: bool) {
        self.enable_random_offsets = value;
    }

    /// Coordinate system: `"y_up"` (default), `"z_up"`, or `"minecraft"`.
    #[wasm_bindgen(getter)]
    pub fn coordinate_system(&self) -> String {
        match self.coordinate_system {
            CoordinateSystem::YUpRightHanded => "y_up".to_string(),
            CoordinateSystem::ZUpRightHanded => "z_up".to_string(),
            CoordinateSystem::Minecraft => "minecraft".to_string(),
        }
    }
    #[wasm_bindgen(setter)]
    pub fn set_coordinate_system(&mut self, value: &str) -> Result<(), JsValue> {
        self.coordinate_system = match value.trim().to_ascii_lowercase().as_str() {
            "y_up" => CoordinateSystem::YUpRightHanded,
            "z_up" => CoordinateSystem::ZUpRightHanded,
            "minecraft" => CoordinateSystem::Minecraft,
            other => {
                return Err(JsValue::from_str(&format!(
                    "Invalid coordinate_system '{other}', expected 'y_up', 'z_up', or 'minecraft'"
                )));
            }
        };
        Ok(())
    }

    /// Leaves cull mode: `"fancy"` (default), `"fast"`, `"single_face"`, or `"none"`.
    #[wasm_bindgen(getter)]
    pub fn leaves_cull_mode(&self) -> String {
        match self.leaves_cull_mode {
            LeavesCullMode::Fancy => "fancy".to_string(),
            LeavesCullMode::Fast => "fast".to_string(),
            LeavesCullMode::SingleFace => "single_face".to_string(),
            LeavesCullMode::None => "none".to_string(),
        }
    }
    #[wasm_bindgen(setter)]
    pub fn set_leaves_cull_mode(&mut self, value: &str) -> Result<(), JsValue> {
        self.leaves_cull_mode = match value.trim().to_ascii_lowercase().as_str() {
            "fancy" => LeavesCullMode::Fancy,
            "fast" => LeavesCullMode::Fast,
            "single_face" => LeavesCullMode::SingleFace,
            "none" => LeavesCullMode::None,
            other => {
                return Err(JsValue::from_str(&format!(
                    "Invalid leaves_cull_mode '{other}', expected 'fancy', 'fast', 'single_face', or 'none'"
                )));
            }
        };
        Ok(())
    }

    /// Glass cull mode: `"group"` (default), `"same_block"`, or `"none"`.
    #[wasm_bindgen(getter)]
    pub fn glass_cull_mode(&self) -> String {
        match self.glass_cull_mode {
            GlassCullMode::Group => "group".to_string(),
            GlassCullMode::SameBlock => "same_block".to_string(),
            GlassCullMode::None => "none".to_string(),
        }
    }
    #[wasm_bindgen(setter)]
    pub fn set_glass_cull_mode(&mut self, value: &str) -> Result<(), JsValue> {
        self.glass_cull_mode = match value.trim().to_ascii_lowercase().as_str() {
            "group" => GlassCullMode::Group,
            "same_block" => GlassCullMode::SameBlock,
            "none" => GlassCullMode::None,
            other => {
                return Err(JsValue::from_str(&format!(
                    "Invalid glass_cull_mode '{other}', expected 'group', 'same_block', or 'none'"
                )));
            }
        };
        Ok(())
    }

    /// Unknown block strategy: `"fallback_cube"` (default) or `"ignore_as_air"`.
    #[wasm_bindgen(getter)]
    pub fn unknown_block_strategy(&self) -> String {
        match self.unknown_block_strategy {
            UnknownBlockStrategy::FallbackCube => "fallback_cube".to_string(),
            UnknownBlockStrategy::IgnoreAsAir => "ignore_as_air".to_string(),
        }
    }
    #[wasm_bindgen(setter)]
    pub fn set_unknown_block_strategy(&mut self, value: &str) -> Result<(), JsValue> {
        self.unknown_block_strategy = match value.trim().to_ascii_lowercase().as_str() {
            "fallback_cube" => UnknownBlockStrategy::FallbackCube,
            "ignore_as_air" | "air" => UnknownBlockStrategy::IgnoreAsAir,
            other => {
                return Err(JsValue::from_str(&format!(
                    "Invalid unknown_block_strategy '{other}', expected 'fallback_cube' or 'ignore_as_air'"
                )));
            }
        };
        Ok(())
    }
}

impl WasmMesherConfig {
    pub(crate) fn to_mesher_config(
        &self,
        atlas_address_map: Option<Arc<AtlasAddressMap>>,
        biome_resolver: Option<Arc<BiomeResolver>>,
        colormaps: Option<Arc<VanillaColormaps>>,
        ctm_solver: Option<Arc<CtmSolver>>,
    ) -> MesherConfig {
        MesherConfig {
            coordinate_system: self.coordinate_system,
            origin_centered: self.origin_centered,
            weld_vertices: self.weld_vertices,
            selection_bounds: None,
            enable_ao: self.enable_ao,
            exclude_hidden_volume: self.exclude_hidden_volume,
            mesh_fluids: self.mesh_fluids,
            ctm_solver: if self.enable_ctm { ctm_solver } else { None },
            atlas_address_map,
            biome_resolver,
            colormaps,
            custom_aliases: None,
            enable_alternate_blocks: self.enable_alternate_blocks,
            enable_random_offsets: self.enable_random_offsets,
            enable_animated_textures: self.enable_animated_textures,
            unknown_block_strategy: self.unknown_block_strategy,
            smooth_biome_blend: self.smooth_biome_blend,
        }
    }

    pub(crate) fn to_face_culler(&self) -> FaceCuller {
        FaceCuller::new(self.leaves_cull_mode, self.glass_cull_mode)
    }
}
