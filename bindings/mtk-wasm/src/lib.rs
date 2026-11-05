//! # `mtk-wasm` (WebAssembly and JavaScript Bindings)
//!
//! Provides high-performance WebAssembly bindings for `libmtk` via `wasm-bindgen`,
//! with full support for WebGPU geometry buffers, voxel meshing, occlusion culling,
//! and multi-threaded Rayon execution via Web Workers (`wasm-bindgen-rayon`).

use js_sys::{Float32Array, Uint16Array, Uint32Array};
use wasm_bindgen::prelude::*;

use mtk_core::constants::concurrency;
use mtk_core::Direction;
use mtk_core::FaceAttributes;
use mtk_core::MeshData;
use mtk_core::Quad;
use mtk_cull::FaceCuller;
use mtk_cull::{GlassCullMode, LeavesCullMode};
use mtk_voxel::{CoordinateSystem, MesherConfig, ModelSource, SectionMesher, VoxelStorage};

#[cfg(feature = "parallel")]
pub use wasm_bindgen_rayon::init_thread_pool;

/// Sets panic hook for readable console error stacks in browser devtools.
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Returns libmtk version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Injects or configures the hardware concurrency count (e.g. from `navigator.hardwareConcurrency`).
#[wasm_bindgen]
pub fn set_hardware_concurrency(count: usize) {
    concurrency::set_hardware_concurrency(count);
}

/// Returns the detected or configured hardware concurrency count.
#[wasm_bindgen]
pub fn get_hardware_concurrency() -> usize {
    concurrency::get_safe_hardware_concurrency()
}

// =============================================================================
// 1. MeshData & Direct Geometry Buffer Views
// =============================================================================

/// JavaScript/WASM wrapper for `MeshData`.
#[wasm_bindgen]
pub struct WasmMeshData {
    pub(crate) inner: MeshData,
}

impl Default for WasmMeshData {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmMeshData {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: MeshData::new(),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn vertex_count(&self) -> usize {
        self.inner.vertex_count()
    }

    #[wasm_bindgen(getter)]
    pub fn triangle_count(&self) -> usize {
        self.inner.triangle_count()
    }

    #[wasm_bindgen(getter)]
    pub fn face_count(&self) -> usize {
        self.inner.face_count()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Appends another `WasmMeshData` into this mesh.
    pub fn append_mesh(&mut self, other: &WasmMeshData) {
        self.inner.append_mesh(&other.inner);
    }

    /// Appends a unit cube face in given direction (0: Down, 1: Up, 2: North, 3: South, 4: West, 5: East).
    pub fn append_unit_cube_face(
        &mut self,
        direction_id: u8,
        material_slot: u16,
    ) -> Result<(), JsValue> {
        let dir = match direction_id {
            0 => Direction::Down,
            1 => Direction::Up,
            2 => Direction::North,
            3 => Direction::South,
            4 => Direction::West,
            5 => Direction::East,
            _ => return Err(JsValue::from_str("Invalid direction index (0..5 expected)")),
        };
        let quad = Quad::unit_cube_face(dir);
        let attr = FaceAttributes {
            material_slot,
            ..Default::default()
        };
        self.inner.append_quad(&quad, &attr);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Direct Pointer & Memory View Accessors (Zero-Copy for WebGPU/WebGL)
    // -------------------------------------------------------------------------

    /// Raw pointer to contiguous float32 vertex positions `[x0, y0, z0, x1, ...]`.
    pub fn positions_ptr(&self) -> *const f32 {
        self.inner.positions_flat().as_ptr()
    }

    /// Total number of float32 entries in positions buffer (`vertex_count * 3`).
    pub fn positions_len(&self) -> usize {
        self.inner.positions_flat().len()
    }

    /// Raw pointer to contiguous float32 vertex normals `[nx0, ny0, nz0, ...]`.
    pub fn normals_ptr(&self) -> *const f32 {
        self.inner.normals_flat().as_ptr()
    }

    /// Total number of float32 entries in normals buffer (`vertex_count * 3`).
    pub fn normals_len(&self) -> usize {
        self.inner.normals_flat().len()
    }

    /// Raw pointer to contiguous float32 vertex UVs `[u0, v0, u1, v1, ...]`.
    pub fn uvs_ptr(&self) -> *const f32 {
        self.inner.uvs_flat().as_ptr()
    }

    /// Total number of float32 entries in UVs buffer (`vertex_count * 2`).
    pub fn uvs_len(&self) -> usize {
        self.inner.uvs_flat().len()
    }

    /// Raw pointer to contiguous uint32 triangle indices.
    pub fn indices_ptr(&self) -> *const u32 {
        self.inner.indices.as_ptr()
    }

    /// Total number of triangle index entries (`triangle_count * 3`).
    pub fn indices_len(&self) -> usize {
        self.inner.indices.len()
    }

    /// Raw pointer to contiguous uint16 face material slot IDs.
    pub fn face_materials_ptr(&self) -> *const u16 {
        self.inner.face_materials.as_ptr()
    }

    /// Total number of face material slot records.
    pub fn face_materials_len(&self) -> usize {
        self.inner.face_materials.len()
    }

    // -------------------------------------------------------------------------
    // TypedArray Copies (for standard JS/Three.js integration)
    // -------------------------------------------------------------------------

    /// Returns a copied Float32Array of vertex positions `[x0, y0, z0, x1, y1, z1, ...]`.
    pub fn get_flat_positions(&self) -> Float32Array {
        Float32Array::from(self.inner.positions_flat())
    }

    /// Returns a copied Float32Array of vertex normals `[nx0, ny0, nz0, ...]`.
    pub fn get_flat_normals(&self) -> Float32Array {
        Float32Array::from(self.inner.normals_flat())
    }

    /// Returns a copied Float32Array of vertex UVs `[u0, v0, u1, v1, ...]`.
    pub fn get_flat_uvs(&self) -> Float32Array {
        Float32Array::from(self.inner.uvs_flat())
    }

    /// Returns a copied Uint32Array of triangle indices.
    pub fn get_indices(&self) -> Uint32Array {
        Uint32Array::from(self.inner.indices.as_slice())
    }

    /// Returns a copied Uint16Array of face material slots.
    pub fn get_face_materials(&self) -> Uint16Array {
        Uint16Array::from(self.inner.face_materials.as_slice())
    }
}

// =============================================================================
// 2. Face Culling Engine Binding
// =============================================================================

/// JavaScript/WASM wrapper for `FaceCuller` occlusion and geometry optimization engine.
#[wasm_bindgen]
pub struct WasmFaceCuller {
    pub(crate) inner: FaceCuller,
}

#[wasm_bindgen]
impl WasmFaceCuller {
    /// Creates a new `FaceCuller` with specified culling modes.
    ///
    /// - `leaves_cull_mode`: 0: SingleFace, 1: Fancy, 2: Fast, 3: None
    /// - `glass_cull_mode`: 0: Group, 1: SameBlock, 2: None
    #[wasm_bindgen(constructor)]
    pub fn new(leaves_cull_mode: u8, glass_cull_mode: u8) -> Result<WasmFaceCuller, JsValue> {
        let leaves = match leaves_cull_mode {
            0 => LeavesCullMode::SingleFace,
            1 => LeavesCullMode::Fancy,
            2 => LeavesCullMode::Fast,
            3 => LeavesCullMode::None,
            _ => {
                return Err(JsValue::from_str(
                    "Invalid leaves_cull_mode (0: SingleFace, 1: Fancy, 2: Fast, 3: None)",
                ))
            }
        };

        let glass = match glass_cull_mode {
            0 => GlassCullMode::Group,
            1 => GlassCullMode::SameBlock,
            2 => GlassCullMode::None,
            _ => {
                return Err(JsValue::from_str(
                    "Invalid glass_cull_mode (0: Group, 1: SameBlock, 2: None)",
                ))
            }
        };

        Ok(Self {
            inner: FaceCuller::new(leaves, glass),
        })
    }

    /// Clears the internal block culling metadata cache.
    pub fn clear_cache(&self) {
        self.inner.clear_cache();
    }

    /// Number of cached blockstate occlusion metadata entries.
    pub fn cache_len(&self) -> usize {
        self.inner.cache_len()
    }
}

// =============================================================================
// 3. Voxel Storage & Mesher Configuration
// =============================================================================

/// Configuration options for the Section mesher.
#[wasm_bindgen]
pub struct WasmMesherConfig {
    pub(crate) inner: MesherConfig,
    pub(crate) num_threads: Option<usize>,
}

#[wasm_bindgen]
impl WasmMesherConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(
        enable_ao: bool,
        mesh_fluids: bool,
        z_up_coordinates: bool,
        num_threads: Option<usize>,
    ) -> Self {
        let inner = MesherConfig {
            enable_ao,
            mesh_fluids,
            coordinate_system: if z_up_coordinates {
                CoordinateSystem::ZUpRightHanded
            } else {
                CoordinateSystem::Minecraft
            },
            ..Default::default()
        };
        Self { inner, num_threads }
    }
}

/// Sparse voxel storage managing 16x16x16 chunk sections with dirty tracking.
#[wasm_bindgen]
pub struct WasmVoxelStorage {
    pub(crate) inner: VoxelStorage,
}

impl Default for WasmVoxelStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmVoxelStorage {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: VoxelStorage::new(),
        }
    }

    /// Configures the active world bounding box in block coordinates.
    pub fn set_bounds(
        &mut self,
        min_x: i32,
        min_y: i32,
        min_z: i32,
        size_x: i32,
        size_y: i32,
        size_z: i32,
    ) -> bool {
        self.inner
            .set_bounds(min_x, min_y, min_z, size_x, size_y, size_z)
    }

    /// Sets the block state at global `(x, y, z)` coordinate.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_state: &str) -> bool {
        self.inner.set_block(x, y, z, block_state, None)
    }

    /// Retrieves the block state at global `(x, y, z)` coordinate.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> String {
        self.inner.get_block(x, y, z).to_string()
    }

    /// Number of sections currently marked dirty for meshing.
    pub fn dirty_section_count(&self) -> usize {
        self.inner.dirty_sections.len()
    }

    /// Clears all dirty section markers.
    pub fn clear_dirty_sections(&mut self) {
        self.inner.clear_dirty_sections();
    }
}

// =============================================================================
// 4. Section & World Mesher Generator
// =============================================================================

/// Section and World Mesher generator supporting multi-threaded Rayon execution in WASM.
#[wasm_bindgen]
pub struct WasmSectionMesher;

#[wasm_bindgen]
impl WasmSectionMesher {
    /// Meshes all non-empty sections in `WasmVoxelStorage` and returns a merged `WasmMeshData`.
    pub fn mesh_world(
        storage: &WasmVoxelStorage,
        config: Option<WasmMesherConfig>,
        culler: Option<WasmFaceCuller>,
    ) -> Result<WasmMeshData, JsValue> {
        let default_config = MesherConfig::default();
        let cfg = config.as_ref().map(|c| &c.inner).unwrap_or(&default_config);
        let num_threads = config.as_ref().and_then(|c| c.num_threads);

        let default_culler = FaceCuller::default();
        let cul = culler.as_ref().map(|c| &c.inner).unwrap_or(&default_culler);

        let non_empty = storage.inner.get_all_non_empty_sections();
        if non_empty.is_empty() {
            return Ok(WasmMeshData::new());
        }

        let padded_sections: Vec<_> = non_empty
            .into_iter()
            .map(|coord| storage.inner.get_section_padded_array(coord))
            .collect();

        let results = SectionMesher::mesh_sections_parallel_with_source_and_progress(
            &padded_sections,
            cul,
            |_| ModelSource::None,
            cfg,
            num_threads,
            None,
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let mut merged = WasmMeshData::new();
        for (_coord, mesh) in results {
            merged.inner.append_mesh(&mesh);
        }

        Ok(merged)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_mesh_data() {
        let mut mesh = WasmMeshData::new();
        assert_eq!(mesh.vertex_count(), 0);
        assert!(mesh.is_empty());

        let res = mesh.append_unit_cube_face(1, 10);
        assert!(res.is_ok());
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.face_count(), 1);

        assert_eq!(mesh.positions_len(), 12);
        assert_eq!(mesh.normals_len(), 12);
        assert_eq!(mesh.uvs_len(), 8);
        assert_eq!(mesh.indices_len(), 6);
        assert_eq!(mesh.face_materials_len(), 1);

        mesh.clear();
        assert_eq!(mesh.vertex_count(), 0);
        assert!(mesh.is_empty());
    }

    #[test]
    fn test_wasm_concurrency_and_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        set_hardware_concurrency(8);
        assert_eq!(get_hardware_concurrency(), 8);
        set_hardware_concurrency(0);
    }

    #[test]
    fn test_wasm_voxel_and_mesher() {
        let mut storage = WasmVoxelStorage::new();
        storage.set_bounds(0, 0, 0, 16, 16, 16);
        storage.set_block(0, 0, 0, "minecraft:stone");
        assert_eq!(storage.get_block(0, 0, 0), "minecraft:stone");
        assert_eq!(storage.dirty_section_count(), 1);

        let culler = WasmFaceCuller::new(0, 0).unwrap();
        let config = WasmMesherConfig::new(true, true, true, Some(2));

        let mesh = WasmSectionMesher::mesh_world(&storage, Some(config), Some(culler)).unwrap();
        assert!(!mesh.is_empty());
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.face_count(), 6);
    }
}
