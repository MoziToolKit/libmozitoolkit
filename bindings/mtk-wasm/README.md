# mtk-wasm

[![Crate](https://img.shields.io/badge/crate-mtk--wasm-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![WebAssembly](https://img.shields.io/badge/WASM-wasm--bindgen-blue.svg)](https://rustwasm.github.io/docs/wasm-bindgen/)
[![Three.js](https://img.shields.io/badge/Three.js-Ready-green.svg)](https://threejs.org/)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-wasm`** 是基于 [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) 构建的 WebAssembly 绑定库，用于浏览器内 `.mtkscene` 场景包加载、体素网格化并输出 Three.js / WebGPU 友好的零拷贝渲染批次。

---

## 1. 核心定位（S1 精简版 3D 查看器）

- **单向浏览器管线**：`.mtkscene`（`Uint8Array`） → `WasmScenePackage` → `WasmRenderScene` / `WasmRenderBatch` → `THREE.BufferGeometry`。
- **轻量内存策略**：
  1. 贴图以原始 PNG 字节流暴露（`read_texture_png`），由浏览器通过 `Blob` + `createImageBitmap` 异步解码，不在 WASM 线性内存中持有大尺寸未压缩 RGBA 缓冲；
  2. 几何网格在 Rust 内按图集分块（`atlas_chunk_id`）拆批，并将 `PerLoop` UV/染色/AO 对齐为 `PerVertex` 缓冲，通过 WASM 线性内存指针零拷贝构造 `THREE.BufferAttribute`；
  3. `take_batch(index)` 将批次所有权直接转移给 JS，避免二次拷贝峰值；
  4. `build_render_scene()` 在返回前释放临时 `VoxelStorage` / `VoxelWorld` 内存。

---

## 2. 导出 API 清单

### 2.1 全局控制
- `init_panic_hook()`：初始化浏览器控制台可读 panic 堆栈。
- `version() -> string`：返回绑定版本号。
- `set_hardware_concurrency(count: number)` / `get_hardware_concurrency() -> number`：配置并发线程数。

### 2.2 `WasmScenePackage`
- `WasmScenePackage.from_bytes(bytes: Uint8Array) -> WasmScenePackage`：从内存字节打开 `.mtkscene`。
- `metadata_json() -> string`：返回 `manifest`、`texture_names` 与 `atlas_chunks` 描述 JSON。
- `texture_names() -> string[]`：列出包内所有 `atlas/textures/*.png` chunk ID。
- `read_texture_png(chunk_id: string) -> Uint8Array`：读取原始 PNG 字节（不经过像素解码）。
- `build_render_scene(enable_ao?: boolean, mesh_fluids?: boolean, enable_ctm?: boolean, enable_animated_textures?: boolean) -> WasmRenderScene`：按 Y-Up 右手坐标系构建 Three.js 渲染批次。

### 2.3 `WasmRenderScene` & `WasmRenderBatch`
- `WasmRenderScene`：`batch_count`、`total_vertices`、`total_triangles`、`take_batch(index)`。
- `WasmRenderBatch`：
  - 元数据：`chunk_id`、`is_animated`、`has_animation`、`vertex_count`、`triangle_count`、`albedo_chunk_id`、`overlay_chunk_id`、`normal_chunk_id`、`orm_chunk_id`、`aux_chunk_id`
  - 零拷贝指针与长度：
    - `positions_ptr() / positions_len()` (`Float32Array`, 3 floats/vertex)
    - `normals_ptr() / normals_len()` (`Float32Array`, 3 floats/vertex)
    - `uvs_ptr() / uvs_len()` (`Float32Array`, 2 floats/vertex)
    - `colors_ptr() / colors_len()` (`Float32Array`, 4 floats/vertex, 线性 `[tint_r, tint_g, tint_b, ao]`)
    - `tint_weights_ptr() / tint_weights_len()` (`Float32Array`, 2 floats/vertex, `[base_tint_factor, overlay_tint_factor]`)
    - `indices_ptr() / indices_len()` (`Uint32Array`, 3 indices/triangle)
    - `anim_timings_ptr() / anim_timings_len()` (`Float32Array`, 3 floats/vertex，仅动图批次非空)
    - `anim_frame_sizes_ptr() / anim_frame_sizes_len()` (`Float32Array`, 3 floats/vertex，仅动图批次非空)

---

## 3. Three.js 最小调用示例

```javascript
import init, { init_panic_hook, WasmScenePackage } from './pkg/mtk_wasm.js';
import * as THREE from 'three';

const wasm = await init();
init_panic_hook();

const bytes = new Uint8Array(await fetch('/demo.mtkscene').then(r => r.arrayBuffer()));
const pkg = WasmScenePackage.from_bytes(bytes);
const renderScene = pkg.build_render_scene(true, true, true, true);

for (let i = 0; i < renderScene.batch_count; i++) {
    const batch = renderScene.take_batch(i);
    const geom = new THREE.BufferGeometry();

    geom.setAttribute('position', new THREE.BufferAttribute(
        new Float32Array(wasm.memory.buffer, batch.positions_ptr(), batch.positions_len()).slice(), 3));
    geom.setAttribute('normal', new THREE.BufferAttribute(
        new Float32Array(wasm.memory.buffer, batch.normals_ptr(), batch.normals_len()).slice(), 3));
    geom.setAttribute('uv', new THREE.BufferAttribute(
        new Float32Array(wasm.memory.buffer, batch.uvs_ptr(), batch.uvs_len()).slice(), 2));
    geom.setAttribute('color', new THREE.BufferAttribute(
        new Float32Array(wasm.memory.buffer, batch.colors_ptr(), batch.colors_len()).slice(), 4));
    geom.setIndex(new THREE.BufferAttribute(
        new Uint32Array(wasm.memory.buffer, batch.indices_ptr(), batch.indices_len()).slice(), 1));

    const pngBytes = pkg.read_texture_png(batch.albedo_chunk_id);
    const bitmap = await createImageBitmap(new Blob([pngBytes], { type: 'image/png' }));
    const tex = new THREE.Texture(bitmap);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.magFilter = THREE.NearestFilter;
    tex.minFilter = THREE.NearestMipmapLinearFilter;
    tex.needsUpdate = true;

    const mat = new THREE.MeshStandardMaterial({
        map: tex,
        vertexColors: true,
        alphaTest: 0.5,
    });
    scene.add(new THREE.Mesh(geom, mat));
    batch.free();
}
renderScene.free();
pkg.free();
```

---

## 4. 构建与测试

```bash
# 原生单元与往返测试
cargo test -p mtk-wasm

# WASM 交叉编译检查与 release 构建
cargo check -p mtk-wasm --target wasm32-unknown-unknown
wasm-pack build bindings/mtk-wasm --target web --release
```
