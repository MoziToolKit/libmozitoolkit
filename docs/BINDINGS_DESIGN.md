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
| **`mtk-wasm`** | **P1 (Web 绑定)** | wasm-bindgen (`mtk_wasm`) | **Web 查看器**：面向 Three.js / WebGPU 的 `.mtkscene` 与 `.mtkcache`+存档浏览器预览 |

```
bindings/
  ├── mtk-py/    -> [P0] Python CPython 扩展模块 (PyO3 + maturin) -> libmtk_py
  └── mtk-wasm/  -> [P1] 浏览器与 WebGPU 模块 (wasm-bindgen + wasm-pack) -> mtk_wasm
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
  - `indices_memoryview(py)`: 零拷贝返回 `[M]` u32 三角形/多边形索引。
  - `quad_indices_memoryview(py)`: 零拷贝返回 `[Q, 4]` u32 四边形顶角索引缓冲。
  - `face_materials_memoryview(py)`: 逐面材质插槽 ID。
  - `to_buffers(py)`: 宿主无关标准连续缓冲模型（Tier 3），返回包含 `positions`, `normals`, `uvs`, `indices`, `loop_starts`, `loop_totals` 等的 typed memoryview 字典。
  - **宿主内存适配层隔离 (`adapters/mtk-host-memory` / `adapters/blender/mtk-blender-dna`)**：通用 raw slice → pointer 复制位于 `adapters/mtk-host-memory`，Blender 语义包装位于 `adapters/blender/mtk-blender-dna`。两者都不依赖核心 crate；独立 `libmtk_blender_dna` wheel 通过 Python Buffer Protocol 接收核心 `memoryview`，并写入 Blender 指针。核心 wheel 不包含也不要求该适配器。
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
- `sample_colormap_colors(temp, hum, colormaps=None) -> dict`: 原版 256x256 三角形 Colormap 线性 RGBA 与十六进制采样。
- `compute_biome_tint_attributes_paletted(...)`: Rayon 多线程批量计算网格面的 Tint 颜色属性（palette + u32 indices 输入，返回零拷贝 flat float32 `memoryview`；支持全局预设、多群系加权及 `face_biomes` 逐面生物群系数组）。
- `compute_mesh_biome_attributes_from_storage(face_texture_keys, block_xs, block_ys, block_zs, storage, resolver=None, smooth=True)`: 按每个面的真实 3D 世界坐标从 `VoxelStorage` 查询 3D Quart Biome 并执行 5×5 (R=2) 平滑混合计算面染色属性。
- `get_biome_meta(name)` / `get_all_biomes()`: 查询 66 原版生物群系规范参数（温度、湿度、草方块/树叶/水体颜色）。
- `MaterialResolver`: 材质别名解算与多通道 UV 重映射 (`remap_mesh_multi_uvs_paletted`)。


##### 3. 体素与实时同步 (`mtk.voxel` & `mtk.sync`)
- `VoxelStorage`: 稀疏体素世界存储，纳秒级快照更新与选区包围盒裁剪；测试专用底层方法位于 `unstable.testing`。
- **技术栈**：`wasm-bindgen` + `wasm-pack`（发布为 ES Module / npm package）。
- **配置与数据对接**：
  - 使用强类型 `WasmMesherConfig` DTO 控制坐标系（默认 `y_up`）、居中、焊接、AO/流体/CTM/动图及树叶/玻璃剔除模式。
  - 利用 WASM 线性内存指针 (`positions_ptr` / `indices_ptr` 等) 配合 JS 侧单次 `TypedArray(...).slice()` 脱离线性内存后立即 `batch.free()`，避免序列化开销以及后续 `memory.grow` 引起的视图悬空。
- **目标场景**：Web 网页端 `.mtkscene` / 本地 Minecraft 存档 3D 预览器（见 [`bindings/mtk-wasm/examples/viewer`](../bindings/mtk-wasm/examples/viewer)）、WebGPU 在线地图渲染。

---

## 3. 前后端协作契约与最佳实践

1. **Bridge 层作为唯一门面**：
   在 Blender 插件端 (`MoziToolKit`)，必须通过 `bridge/` 模块（如 `bridge/mesh.py`, `bridge/extrude.py`, `bridge/subdivide.py`, `bridge/assets.py`）统一封装对 `libmtk_py` 的调用，禁止在 UI 或 Operator 中直接调用底层的 CPython C 扩展。
2. **优雅降级与错误反馈**：
   当用户环境未正确加载 `libmtk_py` 动态库时，`bridge` 层必须给出清晰的引导提示（如指向偏好设置中的 Wheel 安装面板），而非直接抛出未捕获的 ImportError 导致崩溃。
3. **测试覆盖铁律**：
   任何新添加的核心能力，必须先在底层 Rust Crate 实现单元测试并保证 `cargo test` 绿灯，随后在 `mtk-py` 导出并编写对应的 Python 集成测试。

---

## 4. 线程与 GIL 契约（强制）

原生核心大量使用 rayon。绑定层与核心之间的线程边界只有两条规则，且都由
`tools/ci_guardrails.py` 机械校验：

1. **所有可能阻塞或派发线程的入口必须经 `crate::threading::run_detached(py, …)`。**
   等价于 `py.detach`，但它是唯一被门禁认可的边界标记。若在持有 GIL 的情况下让 worker
   线程回调 Python（`Python::attach`），worker 会等一个被"正阻塞在 `par_iter().collect()`
   的调用线程"持有的锁 —— 整进程死锁（2026-11 存档导入卡死即此）。
2. **进度回调可能来自任意线程。**
   `mtk_core::ProgressCallback` 是 `Send + Sync`，网格化与预烘会从 rayon worker 逐项上报。
   因此：
   - 绑定侧只允许在 `run_detached` 区域内把回调交给原生代码；
   - 宿主侧的回调**不得直接触碰宿主主线程资源**（Blender 的 `bpy`/`wm`），必须投递到
     线程安全队列，由主线程轮询消费（`MoziToolKit/utils/async_task.py` 的 `AsyncTask`
     就是标准范式：`queue.put` + modal timer 抽干）。
   - 回调抛出的异常会被吞掉（原生活动已经完成，没有回传通道）；需要暴露诊断就写日志/事件，
     不要指望回调的异常。

### 4.1 门禁实现

`check_threaded_bindings_detach()` 在 `bindings/mtk-py/src` 内做函数级扫描：

- 任何 `Option<ProgressCallback…>` 变量的**使用**，必须位于 `run_detached(...)` /
  `py.detach(...)` 的括号区间内；
- 任何直接出现的 `par_iter(` / `into_par_iter(` 也必须在同一区间内。

它只在"使用点"报错，而不是"函数里出现过 detach 就放行"——这正是旧实现漏掉的那类 bug
（同一函数里网格阶段 detach 了，加载阶段没有）。

### 4.2 线程预算与池所有权

策略与池都在 `mtk_core::threading` 一处：

- **默认**：`max(1, available_parallelism() - 2)`。用户机器上通常还有浏览器/后台任务，
  给宿主主线程和系统留两个核；
- **显式覆盖**：`libmtk_py.set_thread_count(n)`（`0` = 恢复默认），或插件偏好
  `thread_count`。一次设定对**所有**并行入口生效（网格化、存档解码、图集烘焙、
  材质重映射、预烘），不再有"只有部分站点听 num_threads"的情况；
- **池复用**：`threading::install(…)` / `install_with(Some(n), …)` 按线程数缓存
  `rayon::ThreadPool`（最多 4 个），不再每次调用 `ThreadPoolBuilder::build()`；
  在已有并行区域内嵌套调用会直接内联执行，避免 `n × n` 线程爆炸；
- **探测**：`libmtk_py.threading_info()` 返回 `{workers, override, available}`，宿主面板
  与测试用它确认真正生效的值。
