# 跨语言绑定与胶水层架构设计 (Bindings Design)

本文档定义 `libmozitoolkit` 跨平台跨语言绑定（Bindings / FFI）的工程结构、导出协议、内存管理以及与 DCC 宿主（如 Blender）的高效对接规范。

---

## 1. 胶水层定位与核心原则

1. **计算与胶水严格分离**：所有的核心几何重构、网格细分、挤出 UV 修复、图集装箱、生物群系属性计算均在底层 Rust Crates 完成。Bindings 层仅作为数据封包、格式转换和跨语言内存映射的桥梁。
2. **零拷贝与扁平视图优先 (Zero-Copy First)**：对于海量几何数据（`positions`、`normals`、`uvs`、`indices`）与大尺寸贴图像素（`RgbaBuffer`），优先通过共享内存切片、Python Buffer Protocol (`memoryview`) 或 WebAssembly `TypedArray` 传递，严禁低效的跨语言逐元素循环复制。
3. **Data In, Data Out 批处理范式**：对于涉及复杂场景网格变换的操作（如挤出修复、随机挤出、像素细分），前端将网格数据以原生数组形式传入，Rust 核心完成全量拓扑推算与修复，仅返回修改增量或新拓扑，大幅降低跨语言调用频次。
4. **严格的内存生命周期管理**：FFI 与宿主语言交互时，必须提供配对的释放函数（如 `mtk_mesh_free`、`mtk_buffer_free`），杜绝跨语言内存泄漏或野指针。

---

## 2. 绑定演进路线图与目标平台

根据项目整体战略规划，跨语言绑定与工具链按以下优先级演进：

| 绑定 / 工具 | 演进优先级 | 核心目标与技术栈 | 状态与定位 |
| :--- | :---: | :--- | :--- |
| **`mtk-py`** | **P0 (当前核心)** | PyO3 + maturin + CPython ABI3 (`libmtk_py`) | **生产事实标准**：MoziToolKit (Blender 4.2+) 扩展插件的核心胶水后端 |
| **`mtk-cli`** | **P1 (工具与验证)** | Rust 原生 CLI (`mtk`) | **核心工具链**：无头资产预编译、模型排查与测试验证 |
| **`mtk-bench`** | **P1 (基准压测)** | 性能基准测试套件 | **质量防线**：4000+ 区块段多核网格化与遮挡剔除压测 |
| **`mtk-wasm`** | **P2 (远期预备)** | wasm-bindgen (`mtk_wasm`) | **生态预备**：面向 WebGPU / Three.js 网页端 3D 资产预览 |

```
bindings/
  ├── mtk-py/    -> [P0] Python CPython 扩展模块 (PyO3 + maturin) -> libmtk_py
  └── mtk-wasm/  -> [P2] 浏览器与 Node.js 模块 (wasm-bindgen + wasm-pack) -> mtk_wasm
```

---

### 2.1 Python 绑定 (`bindings/mtk-py`)

- **构建与分发规范**：
  - 必须在工作区专属虚拟环境（`.venv`）中调用 `maturin build --release` 编译标准 Wheel。
  - 严格遵循 Blender 4.2+ 扩展轮子规范，编译产物仅同步拷贝至 `MoziToolKit/wheels/` 并由 `blender_manifest.toml` 统一声明。
  - **严禁将编译产物安装（`pip install`）到系统全局 Python 或宿主 Blender 的全局 `bpy` 环境中**，杜绝破坏沙箱与环境隔离。

#### 核心导出模块与 API 概览

##### 1. 几何、细分与挤出算子 (`mesh`, `extrude`, `subdivide`)
- `PyMeshData`:
  - `positions_memoryview(py)`: 零拷贝返回 `[N, 3]` f32 顶点坐标缓冲。
  - `normals_memoryview(py)`: 零拷贝返回 `[N, 3]` f32 法线缓冲。
  - `uvs_memoryview(py)`: 零拷贝返回 `[N, 2]` f32 主 UV 缓冲。
  - `loop_uvs_memoryview(py)`: 零拷贝返回 `[L, 2]` f32 角隅 Loop 域 UV 缓冲，彻底杜绝 Python 逐角隅数组组装开销。
  - `loop_starts_memoryview(py)`: 零拷贝返回 `[F]` i32 Blender Polygon `loop_start` 偏移缓冲。
  - `loop_totals_memoryview(py)`: 零拷贝返回 `[F]` i32 Blender Polygon `loop_total` 顶点数缓冲（Quad 为 4）。
  - `indices_memoryview(py)`: 零拷贝返回 `[M]` u32 三角形/多边形索引。
  - `quad_indices_memoryview(py)`: 零拷贝返回 `[Q, 4]` u32 四边形顶角索引缓冲。
  - `face_materials_memoryview(py)`: 逐面材质插槽 ID。
  - **宿主内存适配层隔离 (`adapters/mtk-host-memory` / `adapters/blender/mtk-blender-dna`)**：通用 raw slice → pointer 复制位于 `adapters/mtk-host-memory`，Blender 语义包装位于 `adapters/blender/mtk-blender-dna`。两者都不依赖核心 crate；独立 `libmtk_blender_dna` wheel 通过 Python Buffer Protocol 接收核心 `memoryview`，并写入 Blender 指针。核心 wheel 不包含也不要求该适配器。
  - `cull_faces(tolerance=1e-3, cull_opposite=True, cull_duplicates=True) -> PyMeshData`: 执行空间遮挡与叠面剔除，无缝保留所有自定义属性与 Quad 拓扑。
- **高阶 Data-In Data-Out 算子**：
  - `process_flat_mesh_extrude_repair(...)`:
    使用扁平连续 1D 缓冲区（`positions`, `loop_vertices`, `loop_uvs`, `face_loop_starts`, `face_loop_totals`, `face_materials`）进行极致性能的批量挤出修复，彻底消除 Python 列表嵌套开销，完美适配 Blender `foreach_get` 与 NumPy 数组。
  - `adaptive_pixel_split_mesh(...)`:
    自适应像素网格细分，基于贴图分辨率与 UV 物理跨度切分，保留 Quad 拓扑并双线性插值所有顶点属性。
  - `slice_face_by_pixel_grid(...)` / `calculate_face_target_grid(...)` / `calculate_pixel_grid_cut_factors(...)` 属于测试/开发 API，位于 `unstable.testing`。

##### 2. 材质与生物群系 (`mtk.material`)
- `PyBiomeResolver`:
  - `BiomeResolver(models=None)`: 可选传入原始模型字典构建解析器。
  - `get_tint_info(texture_name, block_name=None, tint_index=None)`: 返回包含 `tint_type`, `tint_category`, `tint_weight`, `base_tint_weight`, `overlay_tint_weight`, `hardcoded_hex`, `is_hardcoded` 等完整元数据的字典。
  - `from_file(path)`: 从预编译的 `biome_mapping.json` 极速载入映射。
  - `to_json()` / `from_json(json_str)`: 序列化支持。
- `blend_biome_colors(biome_weights, tint_type="grass") -> [f32; 4]`: 多群系平滑线性色彩过渡混合。
- `sample_colormap_pixel(image_bytes, width, height, temp, hum, channels=4) -> [f32; 3]`: 原版 256x256 三角形 Colormap 像素采样。
- `mesh_attribute_schema()` / `retired_mesh_attributes()`: 暴露 `mtk-core::attributes` 的规范属性契约（活跃与退役清单），宿主据此生成镜像并清理旧数据。
- `compute_biome_tint_attributes(...)`: Rayon 多线程批量计算网格面的 Tint 颜色属性（支持全局预设、多群系加权及 `face_biomes` 逐面生物群系数组）。
- `compute_mesh_biome_attributes_from_storage(face_texture_keys, block_xs, block_ys, block_zs, storage, resolver=None, smooth=True)`: 按每个面的真实 3D 世界坐标从 `VoxelStorage` 查询 3D Quart Biome 并执行 5×5 (R=2) 平滑混合计算面染色属性。
- `get_biome_meta(name)` / `get_all_biomes()`: 查询 66 原版生物群系规范参数（温度、湿度、草方块/树叶/水体颜色）。
- `srgb_to_linear(color)` / `linear_to_srgb(color)`: 线性色彩空间数学；colormap 采样在 Rust 内完成，Python 不再暴露 UV 计算。
- `MaterialResolver`: 材质别名解算与多通道 UV 重映射 (`remap_mesh_multi_uvs_parallel`)。


##### 3. 体素与实时同步 (`mtk.voxel` & `mtk.sync`)
- `VoxelStorage`: 稀疏体素世界存储，纳秒级快照更新与选区包围盒裁剪；测试专用底层方法位于 `unstable.testing`。
- **技术栈**：`wasm-bindgen` + `wasm-pack` (发布为 npm package / ES Module)。
- **数据对接**：
  - 利用 `js_sys::Float32Array::view` / `js_sys::Uint32Array::view` 直接将 WASM 线性内存暴露给 JavaScript 前端。
  - Three.js / Babylon.js / WebGPU 渲染管线可直接通过 `BufferAttribute` 引用 WASM 内存并上传 GPU。
- **目标场景**：Web 网页端 3D 预览器、WebGPU 在线地图渲染、轻量前端数据转换工具。

---

## 3. 前后端协作契约与最佳实践

1. **Bridge 层作为唯一门面**：
   在 Blender 插件端 (`MoziToolKit`)，必须通过 `bridge/` 模块（如 `bridge/mesh.py`, `bridge/extrude.py`, `bridge/subdivide.py`, `bridge/assets.py`）统一封装对 `libmtk_py` 的调用，禁止在 UI 或 Operator 中直接调用底层的 CPython C 扩展。
2. **优雅降级与错误反馈**：
   当用户环境未正确加载 `libmtk_py` 动态库时，`bridge` 层必须给出清晰的引导提示（如指向偏好设置中的 Wheel 安装面板），而非直接抛出未捕获的 ImportError 导致崩溃。
3. **测试覆盖铁律**：
   任何新添加的核心能力，必须先在底层 Rust Crate 实现单元测试并保证 `cargo test` 绿灯，随后在 `mtk-py` 导出并编写对应的 Python 集成测试。
