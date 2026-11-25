# mtk-wasm

[![Crate](https://img.shields.io/badge/crate-mtk--wasm-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![WebAssembly](https://img.shields.io/badge/WASM-wasm--bindgen-blue.svg)](https://rustwasm.github.io/docs/wasm-bindgen/)
[![Three.js](https://img.shields.io/badge/Three.js-Ready-green.svg)](https://threejs.org/)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-wasm`** 是基于 [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) 构建的 WebAssembly 绑定库，用于浏览器内 `.mtkscene` 场景包加载、`.mtkcache` + Minecraft Java 版存档按需读取与体素网格化，并输出 Three.js / WebGPU 友好的零拷贝渲染批次。

---

## 1. 核心定位（Web 3D 查看器管线）

- **场景包路线（S1）**：`.mtkscene`（`Uint8Array`） → `WasmScenePackage` → `WasmRenderScene` / `WasmRenderBatch` → `THREE.BufferGeometry`。
- **缓存 + 本地存档路线（S2）**：
  - `.mtkcache`（`Uint8Array`） → `WasmAssetCache`（提供烘焙模型、图集映射、生物群系、CTM 规则与 `BannerBaker` 旗帜合成模板）；
  - 浏览器选择本地存档目录 → `WasmSaveWorld.inspect_level`（解析 `level.dat` 与维度清单） → `WasmSaveWorld.plan_region_files`（按 `[min, max]` 选区仅筛选相交的 `.mca`/`.mcc`） → `WasmSaveWorld.load_regions` → `WasmRenderScene`；
  - 玩家皮肤走浏览器端异步跨域 `fetch`（`textures.minecraft.net` / `crafthead.net` / `minotar.net`）→ `register_skin_png` + `build_render_scene` 局部脏区块增量刷新。
- **轻量内存与安全传输策略**：
  1. 静态贴图与动态实体图集（`dynamic/chunk_<id>.png`）以原始 PNG 字节流暴露（`read_texture_png`），由浏览器通过 `Blob` + `createImageBitmap(..., { imageOrientation: 'flipY' })` 异步解码；
  2. 几何网格在 Rust 内按图集分块（`atlas_chunk_id`）拆批，并将 `PerLoop` UV/染色/AO 对齐为 `PerVertex` 缓冲（`uvs` 采用 `flipY = false` 自底向上约定，动图帧沿 `-v` 步进；`colors` 为线性 `[tint_r, tint_g, tint_b, ao]`）；JS 侧通过 WASM 线性内存指针单次 `.slice()` 拷贝至独立 `TypedArray` 后即可安全释放 `batch.free()`，避免跨语言序列化开销及后续 `memory.grow` 导致视图悬空；
  3. `take_batch(index)` 将批次所有权直接转移给渲染循环，避免二次拷贝峰值。

---

## 2. 导出 API 清单

### 2.1 全局控制
- `init_panic_hook()`：初始化浏览器控制台可读 panic 堆栈。
- `version() -> string`：返回绑定版本号。
- `set_hardware_concurrency(count: number)` / `get_hardware_concurrency() -> number`：配置并发线程数。

### 2.2 `WasmMesherConfig`
- `new WasmMesherConfig()`：网格化与面剔除强类型配置 DTO（默认：`coordinate_system = "y_up"`, `origin_centered = true`, `weld_vertices = false`, `leaves_cull_mode = "fancy"`, `glass_cull_mode = "group"`, `unknown_block_strategy = "fallback_cube"`，AO/流体/CTM/动图/平滑生物群系均为 `true`）。
- 支持读写属性：`enable_ao`、`mesh_fluids`、`enable_ctm`、`enable_animated_textures`、`smooth_biome_blend`、`origin_centered`、`weld_vertices`、`exclude_hidden_volume`、`enable_alternate_blocks`、`enable_random_offsets`、`coordinate_system`（`"y_up" | "z_up" | "minecraft"`）、`leaves_cull_mode`（`"fancy" | "fast" | "single_face" | "none"`）、`glass_cull_mode`（`"group" | "same_block" | "none"`）、`unknown_block_strategy`（`"fallback_cube" | "ignore_as_air"`）。

### 2.3 `WasmScenePackage`（S1 `.mtkscene` 查看器）
- `WasmScenePackage.from_bytes(bytes: Uint8Array) -> WasmScenePackage`：从内存字节打开 `.mtkscene`。
- `metadata_json() -> string`：返回 `manifest`、`texture_names` 与 `atlas_chunks` 描述 JSON。
- `texture_names() -> string[]`：列出包内所有 `atlas/textures/*.png` chunk ID。
- `read_texture_png(chunk_id: string) -> Uint8Array`：读取原始 PNG 字节（不经过像素解码）。
- `build_render_scene(config?: WasmMesherConfig) -> WasmRenderScene`：按 `WasmMesherConfig`（默认 Y-Up 右手坐标系；传入时移交所有权，无需额外调用 `config.free()`）构建 Three.js 渲染批次。

### 2.4 `WasmAssetCache` & `WasmSaveWorld`（S2 `.mtkcache` + 本地存档查看器）
- **`WasmAssetCache`**：
  - `WasmAssetCache.from_bytes(bytes: Uint8Array) -> WasmAssetCache`：加载 `.mtkcache`。
  - `metadata_json() -> string` / `texture_names() -> string[]` / `read_texture_png(chunk_id: string) -> Uint8Array`。
- **`WasmSaveWorld`**：
  - `WasmSaveWorld.inspect_level(level_dat_bytes: Uint8Array, relative_paths: string[]) -> string`：解析 `level.dat`（含 GZip / Raw NBT 与经典/现代出生点）及可用维度。
  - `WasmSaveWorld.plan_region_files(dimension, min_x, min_y, min_z, max_x, max_y, max_z, relative_paths) -> string`：返回与选框相交的 `.mca` 与 `.mcc` 相对路径规划。
  - `new WasmSaveWorld(cache: WasmAssetCache)`：创建绑定资产缓存与内置 `BannerBaker` 的存档会话。
  - `load_regions(regions, external_chunks, min_x, min_y, min_z, max_x, max_y, max_z) -> string`：按选区读入命中的 region 字节，并返回体素统计。
  - `pending_skin_requests_json() -> string`：返回未解析的玩家头颅 `{ key, skin_url, uuid, name, candidate_urls }` 列表（`candidate_urls` 由核心 `mtk-resource` 统一派生并规范 `https://`）。
  - `register_skin_png(key: string, png_bytes: Uint8Array) -> boolean`：校验皮肤 PNG 魔数并过滤镜像站默认兜底皮肤（Alex/Steve/Minotar），注入内存缓存并标脏关联区块。
  - `texture_names() -> string[]` / `read_texture_png(chunk_id: string) -> Uint8Array`：统一读取静态 `.mtkcache` 图集与动态 `dynamic/chunk_<id>.png`（`0xFFF0+` 旗帜/皮肤图集，避免与静态 `entities_chunk_001.png` 命名冲突）。
  - `build_render_scene(config?: WasmMesherConfig) -> WasmRenderScene`：首次全量构建或配置变化时重建，后续皮肤回填仅增量重算脏区块。

### 2.5 `WasmRenderScene` & `WasmRenderBatch`
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
import init, { init_panic_hook, WasmMesherConfig, WasmScenePackage } from './pkg/mtk_wasm.js';
import * as THREE from 'three';

const wasm = await init();
init_panic_hook();

const bytes = new Uint8Array(await fetch('/demo.mtkscene').then(r => r.arrayBuffer()));
const pkg = WasmScenePackage.from_bytes(bytes);
const cfg = new WasmMesherConfig();
const renderScene = pkg.build_render_scene(cfg);

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
