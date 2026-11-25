//! # `mtk-wasm` (WebAssembly Bindings for Web `.mtkscene` & Save Viewer)
//!
//! Provides WebAssembly bindings for loading `.mtkscene` interchange packages
//! or `.mtkcache` + Minecraft save regions in the browser and emitting
//! Three.js / WebGPU-ready `PerVertex` geometry batches with zero-copy buffer
//! pointers and raw PNG atlas chunks.

mod cache;
mod config;
mod render;
mod save;
mod scene;
#[cfg(test)]
mod tests;

use mtk_core::threading;
use wasm_bindgen::prelude::*;

pub use cache::WasmAssetCache;
pub use config::WasmMesherConfig;
pub use render::{WasmRenderBatch, WasmRenderScene};
pub use save::WasmSaveWorld;
pub use scene::WasmScenePackage;

#[cfg(feature = "parallel")]
pub use wasm_bindgen_rayon::init_thread_pool;

/// Sets panic hook for readable console error stacks in browser devtools.
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Returns `mtk-wasm` version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Configures the hardware concurrency count (e.g. from `navigator.hardwareConcurrency`).
#[wasm_bindgen]
pub fn set_hardware_concurrency(count: usize) {
    threading::set_hardware_concurrency(count);
}

/// Returns the detected or configured hardware concurrency count.
#[wasm_bindgen]
pub fn get_hardware_concurrency() -> usize {
    threading::get_safe_hardware_concurrency()
}
