# mtk-py (`libmtk_py`)

[![Crate](https://img.shields.io/badge/crate-mtk--py-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/Python-3.11%2B%20%28ABI3%29-blue.svg)](https://www.python.org)
[![PyO3](https://img.shields.io/badge/PyO3-0.30%2B-green.svg)](https://pyo3.rs)
[![Status](https://img.shields.io/badge/Roadmap%20Priority-P0%20Core-brightgreen.svg)]()
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

`mtk-py` 是基于 [PyO3](https://pyo3.rs) 与 CPython Stable ABI (`abi3-py311`) 构建的高性能 Python 扩展二进制模块（编译产物为 `libmtk_py`）。

---

## 目录 (Table of Contents)

- [1. 架构定位与演进优先级](#1-架构定位与演进优先级)
- [2. 核心导出模块与 API 清单](#2-核心导出模块与-api-清单)
- [3. 构建、编译与测试指南](#3-构建编译与测试指南)
- [4. 真实代码调用示例](#4-真实代码调用示例)
  - [示例 1：零拷贝网格创建与 Blender 数据灌入](#示例-1零拷贝网格创建与-blender-数据灌入)
  - [示例 2：体素世界存储与多核并行网格化 (`VoxelWorld`)](#示例-2体素世界存储与多核并行网格化-voxelworld)
- [5. Feature 开关与依赖](#5-feature-开关与依赖)
- [6. 开源协议 (License)](#6-开源协议-license)

---

## 1. 架构定位与演进优先级

- **演进优先级**：`P0`（**核心绑定与当前生产事实标准**）。
- **核心定位**：作为 **MoziToolKit 2.0 (Blender 4.2+ 扩展插件)** 的唯一核心胶水层（Bridge Backend），全面取代前端低效的纯 Python 网格切分、UV 修复、遮挡剔除、体素网格化与图集资产预编译算法。
- **设计哲学**：
  1. **Data-In, Data-Out 纯数据批处理契约**：仅负责跨语言数据传输与内存生命周期管理，不内嵌宿主专有状态。
  2. **零拷贝内存视图 (Zero-Copy Buffer Protocol)**：底层几何缓冲（`positions`、`normals`、`uvs`、`indices`、自定义属性）直接通过 Python `memoryview` 与 Buffer 协议输出，支持 Blender `b_mesh.vertices.foreach_set` 与 NumPy 数组毫秒级批量灌入。
  3. **Blender 4.2+ 扩展轮子隔离规范**：独立打包为规范 Wheel 文件并存放在 `MoziToolKit/wheels/`，由 Blender 扩展系统本地隔离加载，严禁全局 `bpy` 污染。

---

## 2. 核心导出模块与 API 清单

Python 模块名：`libmtk_py`

- `version() -> str`：原生扩展包语义版本（如 `"0.1.0"`，同时支持属性 `__version__`）。
- `api_version() -> int`：与 Blender 胶水层握手的整数 API 版本（当前 `2`）。

### 2.1 几何网格与自定义属性 (`mesh` & `attributes`)
- **`MeshData` (`PyMeshData`)**：连续内存紧凑几何网格缓冲容器。
  - 属性：`vertex_count`, `triangle_count`, `face_count`, `quad_count`, `is_empty`, `dynamic_entity_atlases` (返回 `{chunk_id: bytes}` 字典，无动态图集时返回 `{}`), `dynamic_entity_slots`
  - 构造与修改：`MeshData()`, `MeshData.from_flat_polygons(...)`, `MeshData.from_polygon_offsets(positions, loop_vertices, loop_uvs, face_offsets, normals=None, face_materials=None)`（CSR 面偏移直接构造）, `clear()`
  - 内存视图（标准零拷贝通道，推荐）：`positions_memoryview()`, `normals_memoryview()`, `uvs_memoryview()`, `indices_memoryview()`, `quad_indices_memoryview()`, `loop_uvs_memoryview()`, `face_materials_memoryview()`, `colors_memoryview()`
  - 宿主无关标准连续缓冲模型（Tier 3）：`to_buffers()` 零拷贝导出包含 `positions`, `normals`, `uvs`, `indices`, `loop_starts`, `loop_totals` 等的 Python typed memoryview 字典。
  - 核心 wheel 不再包含 Blender 指针写入代码。零拷贝宿主写入由独立 `libmtk_blender_dna` wheel 提供，通过 `memoryview` 接收核心 buffer，再写入 Blender 指针；核心不需要知道适配器的存在。
  - 顶点与面自定义属性：`add_string_attribute(name, domain, values)`, `add_attribute_from_buffer(name, domain, data_type, buffer)`, `get_string_attribute(name)`, `attribute_memoryview(name)`, `attribute_info(name)`, `attribute_names()`
- **`analyze_mesh_hard_edges(positions, edge_verts, face_loop_starts, face_loop_totals, loop_verts, sharp_angle_rad, explicit_sharp_edges=None) -> PyBytes`**：基于二面角与边界的高性能硬边/锐边拓扑分析算子，支持零拷贝 Buffer Protocol。

### 2.2 遮挡剔除与状态机 (`cull`)
- **`FaceCuller` (`PyFaceCuller`)**：原版方块与复杂连接体遮挡状态机。
  - `FaceCuller(leaves_cull_mode=0, glass_cull_mode=0)`
  - `clear_cache()`

### 2.3 自适应像素网格切分 (`subdivide`)
- **`adaptive_pixel_split_mesh(mesh, ...)`**：整网格批量自适应像素切分。
- *底层单步切分算子已移至 `unstable.testing`（`calculate_face_target_grid`, `calculate_pixel_grid_cut_factors`, `slice_face_by_pixel_grid`）。*

### 2.4 智能挤出与 UV 修复 (`extrude` & `uv`)
- **挤出算子**：
  - `process_flat_mesh_extrude_repair(positions, uvs, indices, face_mat, ...)`：扁平数组极速挤出修复。
  - `generate_random_extrude_heights(centers, ...)`：为随机离散挤出生成高度。
- **UV 几何数学算法**：
  - `repair_mesh_fluid_uv_inplace(positions, loop_verts, face_loop_starts, face_loop_totals, uvs, ...)`：基于网格索引拓扑的高性能原地流体 UV 斜坡批量修复。
  - `calculate_uv_area(quad_uvs)`、`get_uv_bounds(quad_uvs)`、`get_uv_center(quad_uvs)`、`scale_uv(quad_uvs, factor)`、`get_fluid_top_uvs()`、`get_fluid_side_uvs()`。
  - *底层单步 UV 算子已移至 `unstable.testing`（`is_uv_collapsed`、`detect_uv_rotation`、`straighten_uv`、`normalize_uv_for_atlas_tiling`、`restore_atlas_tiling_uv`、`repair_quad_fluid_uv`、`batch_repair_fluid_uv`）。*

### 2.5 体素存储与网格化 (`voxel` & `mesher`)
- **`VoxelPointCloud` (`PyVoxelPointCloud`)**：全量无剔除 3D 体素点云容器，支持 Blender 点网格 attributes 往返和零拷贝内存视图。
  - `to_storage() -> VoxelStorage`, `to_buffers() -> dict`, `from_indexed_arrays(...)`
  - `positions_memoryview()`, `block_x_memoryview()`, `block_y_memoryview()`, `block_z_memoryview()`
  - `get_state_palette()`, `get_biome_palette()`, `get_visual_entities_palette()`, `set_visual_entities_palette(palette)`, `state_indices_memoryview()`, `biome_indices_memoryview()`：点云只存 palette + 索引，视觉实体以紧凑 JSON 调色板数组传递，不在前端暴露裸编辑框。
  - `compute_sparse_state_hash(min_pos, size, dimension) -> int`：计算权威稀疏状态哈希指纹（xxHash64），用于 0ms 重连前置自洽性校验。
  - `scan_unknown_blocks(model_db=None, atlas=None, custom_aliases=None) -> List[str]`：扫描缺少模型或贴图的未知方块。
- **`VoxelStorage` (`PyVoxelStorage`)**：16x16x16 稀疏 Chunk Section 体素容器，带脏标记追踪。
  - `to_point_cloud(config=None) -> VoxelPointCloud`, `load_from_point_cloud(cloud)`
  - `bounds` (getter), `get_visual_entities_palette()`, `set_visual_entities_palette(palette)`
  - `create_debug_world()`：一键加载内置嵌入的原版调试世界快照（529 sections, 32,539 blocks）。
  - `set_bounds(min_x, min_y, min_z, size_x, size_y, size_z)`, `set_block(x, y, z, state, tint)`
- **`VoxelWorld` (`PyVoxelWorld`)**：统一 3D 场景引擎，支持增量区块缓存与全量世界网格管理。
  - `to_point_cloud() -> VoxelPointCloud`
  - `from_storage(storage, config=None, culler=None, model_db=None, num_threads=None)`
  - `create_debug_world(config=None, culler=None, model_db=None, num_threads=None)`
  - `rebuild_all() -> MeshData`, `rebuild_dirty() -> Dict`, `get_world_mesh() -> MeshData`, `sync_from_storage(storage)`, `clear_cache()`, `hot_reload(config=None, model_db=None)`, `num_threads` (getter)
- **`MesherConfig` (`PyMesherConfig`)**：网格化配置（AO 开关、平滑生物群系过渡 `smooth_biome_blend`、流体曲面、坐标系转换、线程数、顶点焊接）。
- **`LiveSyncSession` (`PyLiveSyncSession`)**：原生 WebSocket 实时协同客户端。
  - `start(url, auto_reconnect=True, max_reconnect_attempts=5, auth_token=None, subscribed_entities=None)`, `stop()`
  - `auth_token` 会随默认 HELLO 发送（RFC v3.1 §4.1；Python 侧先 `strip()`；
    两端令牌会各自 trim 后比较），启用 Yefira `authToken` 时必须传入。
  - `subscribed_entities`：可显式指定订阅的连续视觉实体类型列表（如 `["minecraft:painting"]`）；未提供时默认订阅 `supported_visual_entity_ids()` 全部支持类型。
  - `poll_events(coalesce_meshes=False)`：`True` 时只保留最新 `WORLD_MESH_READY` generation；事件类型包含 `STATUS_CHANGE`, `SELECTION_UPDATED`, `HANDSHAKE`, `WORLD_MESH_READY`, `STREAM_PROGRESS`, `STREAM_FINISHED`, `DELTA_APPLIED`, `VISUAL_ENTITIES_UPDATED` (`upserted: List[str]`, `deleted_uuids: List[str]`), `VERIFIED`, `WARNING`, `ERROR`。
  - `get_dynamic_entity_atlases()`：`{chunk_id: RGBA bytes}` 动态实体图集映射（无单数访问器）
  - `start_with_cache(url, auto_reconnect=True, max_reconnect_attempts=5, server_uuid="", epoch=0, sequence=0, min_pos=(0,0,0), size=(0,0,0), state_hash=0, dimension="minecraft:overworld", auth_token=None)`：携带缓存握手参数直连，服务端命中即返回 `0x00 VERIFIED_SKIP_REBUILD`，实现 0ms 零网格重建秒级直连。
  - `load_from_point_cloud(point_cloud: VoxelPointCloud)`：从 Blender 原生持久化的伴生点云恢复底层 `VoxelStorage` 拓扑，使 0ms 快速路径可无缝衔接后续增量变更。
  - `get_storage() -> VoxelStorage`：获取当前底层 `VoxelStorage` 副本。
  - `client_generation`, `current_sequence`, `current_epoch`, `state_hash`, `server_uuid`, `subscribed_entities`
  - `clear_cache()`, `hot_reload(config=None, model_db=None)`, `request_resync()`
  - `STREAM_PROGRESS.percent` 已由 Rust 归一化为 0..100；`HANDSHAKE.decision_code` 为 RFC v3.1 原始值
- **`LiveSyncServer` (`PyLiveSyncServer`)**：原生 RFC v3.1 / v3.2 WebSocket 实时协同服务端。
  - `LiveSyncServer(bind_address="127.0.0.1:8765", auth_token=None, allow_loopback_without_token=True, server_uuid=None)`
  - `stop()`, `is_running()`, `client_count()`, `local_addr()`, `server_uuid`
  - `set_selection(min_pos, size, dimension="minecraft:overworld")`, `clear_selection()`
  - `submit_baseline(client_id, min_pos, size, dimension, raw_states, block_palette, raw_biomes, biome_palette, block_entities=None) -> int`：全自动计算 state_hash、建立选区与调色板、编码切片分帧并流式推送给客户端（返回计算的 `state_hash`）。
  - `submit_deltas(rel_coords, block_states, block_entities=None)`：全自动增量调色板注册、分片打包 DeltaOp 与 DeltaEntity 并广播。
  - `poll_events() -> List[dict]`（事件包含 `client_connected`, `client_disconnected`, `baseline_requested`, `client_resync`）。

### 2.7 资源包预烘焙与图集 (`resource` & `texture`)
- **`ResourcePackStack` (`PyResourcePackStack`)**：虚拟文件系统 (VFS) 资源包层叠加载器。
- **`probe_pack(path: str) -> dict`**：快速探测资源包（类型、格式版本、有效性、`has_renderable_assets`、`has_block_assets`、`asset_summary` 资产统计等）。
- **`path_provides_vanilla_client(path: str) -> bool`**：严格门禁检测路径/ZIP 是否提供原版客户端完整权威资产（核心方块模型、状态与纹理）。
- **`evaluate_cache_status(cache_dir, stack_pack_count, has_vanilla, expected_fingerprint=None) -> str`**：缓存状态判定门禁（返回 `"NO_PACKS"` | `"NO_VANILLA"` | `"READY"` | `"STALE"` | `"MISSING"`）。
- **`BakedAtlas`**：烘焙图集容器；测试/开发用的 `AtlasBuilder` 位于 `libmtk_py.unstable.testing`。
- **`precompile_all_assets(..., resolver_overrides_json=None)`**：端到端一键无头预烘焙管线；可传入 Python 维护的兼容规则 JSON。
- **像素分析**：`batch_analyze_transparent_faces_f32`；单点采样 `sample_uv_alpha_f32` 位于 `unstable.testing`。

### 2.8 生物群系与材质解析 (`material`)
- **`BiomeResolver` (`PyBiomeResolver`)**：原版 66 种生物群系调色板映射与混合器。
- **`MaterialResolver` (`PyMaterialResolver`)**：着色器材质槽位与外部别名解算器。
- **即时 Biome 重着色**：`reapply_uniform_biome_tint(base_tt, packed, old_colors, colors..., flags...)`
  返回两个零拷贝 memoryview（`packed_tint_data` / `tint_colors`），供 `foreach_set` 直接写入。
  - `remap_mesh_multi_uvs_paletted(..., palette, face_indices, ...)`：palette-native、零拷贝 id/metric buffer
  - `resolve_mesh_routes(face_texture_keys, atlas, aliases=None, enable_animated_textures=false)`
  - `reroute_mesh_animation(uvs, face_loop_ranges, face_texture_keys, face_old_chunk_ids, atlas, aliases=None, enable_animated_textures=false)`
- **`GridAtlasSpec` (`PyGridAtlasSpec`)**：网格图集元数据规范。
- **缓存解析**：`resolve_active_cache_package(cache_dir, fingerprint=None)` 返回 active `.mtkcache` 路径。
- **色彩数学与常量**：`compute_biome_tint_attributes_paletted`, `compute_mesh_biome_attributes_from_storage`, `sample_colormap_colors`, `TINT_TYPE_NONE..DRY_FOLIAGE`

### 2.9 模型烘焙 (`model`)
- **`BakedModelDatabase` (`PyBakedModelDatabase`)**：预编译方块几何模型库。

### 2.10 场景交换包与 Mini-Atlas (`scene`)
- **`ScenePackageReader`**：`.mtkscene` 单文件中间交换容器读取器（只读无副作用设计，构造时不污染全局进程状态）。
  - `open(path)`：快速打开并校验场景元数据；
  - `unpack(origin_centered=True, z_up=True) -> dict`：**高阶解包门面**，单次调用原生解码并交付全部核心组件（`metadata`, `point_cloud`, `models`, `atlas`, `atlas_mapping_json`, `texture_names`, `biome_resolver`, `colormaps`, `dynamic_entity_slots`, `ctm_solver`）；
  - `metadata -> SceneMetadata`：获取场景元数据对象；
  - `read_texture_rgba(chunk_id, flip_v=False) -> (width, height, memoryview)`：直接解压缩并读取单张图集切片为连续 RGBA 内存视图；
  - `read_texture_f32(chunk_id, flip_v=False) -> (width, height, memoryview)`：直接解压缩并读取单张图集切片为 [0.0, 1.0] 浮点连续缓冲。
- **`SceneMetadata`**：场景包元数据对象。
  - 属性：`format_version`, `bounds`, `voxel_count`, `unique_states_count`, `unique_biomes_count`, `unique_entities_count`, `visual_entities_count`, `atlas_resolution`, `has_animated_atlas`, `generator`, `created_at_epoch_secs`；
  - `to_dict() -> dict`：快速转化为 Python 字典（包含嵌套的 `capabilities` 字典）。
- **`export_scene_package(path, point_cloud, cache, options=None, **kwargs) -> SceneMetadata`**：
  端到端场景交换导出算子：支持传入结构化 `options` 字典或关键字参数（`max_atlas_size`, `mip_level`, `dynamic_entity_atlases`, `ctm_solver`, `enable_ctm`, `dynamic_entity_slots`）。自动扫描伴生体素点云、执行模型树剪枝、从激活缓存裁剪贴图、自适应缝合动态实体皮肤与旗帜纹理、剪枝并嵌入 CTM 规则与子贴图、自包含打包 Biome 色盘与 Resolver、拼接微型多通道图集、并行重映射模型 UV 并压缩打包为 `.mtkscene`。
- **稀疏体素数据结构 (`mtk_voxel`)**：
  - `SparseVoxelPayload` / `SparseVoxelColumns`：正交离散调色板（`state_palette`, `biome_palette`, `entity_palette`）与紧凑 SoA 列存布局，实现与伴生体素点云的 1:1 零拷贝互转。

### 2.11 视觉实体与渲染计划 (`entity`)
- **连续视觉实体格式**：通用连续视觉实体（涵盖 Painting、ItemFrame 以及未来 Mob、Display 等）在 Python 层统一以规范 JSON 字典 / 字符串或紧凑离散调色板传递：
  - 规范字段：`id`, `uuid`, `custom_name`, `kind`, `position`, `rotation`（权威 Euler XYZ 弧度，局部模型空间，yaw 0 = North / 朝向 -Z）, `scale`, `facing`（只作为元数据供参考，宿主严禁重复旋转）
  - 点云 / 存储通过 `get_visual_entities_palette()`, `set_visual_entities_palette(palette)` 访问。
- **`supported_visual_entity_ids() -> List[str]`**：查询 native core (`mtk-model`) 声明支持的连续视觉实体类型清单（单一事实源 SSOT，如 `["minecraft:painting", "minecraft:item_frame", "minecraft:glow_item_frame", "minecraft:armor_stand"]`）。
- **`build_entity_render_plan(entities=None, atlas=None, config=None, atlas_mapping_json=None, bounds=None, origin_centered=True, coordinate_system="z_up", revision=0, authoritative=True, deleted_keys=None) -> EntityRenderPlan`**：批量编译视觉实体渲染计划，一次性完成实体验证、确定性唯一键分配、局部空间网格烘焙、图集槽位分配以及宿主空间几何变换计算（$O(1)$ 跨语言 FFI 通信）。支持传入 `deleted_keys` 配合 `authoritative=False` 构造增量局部重构计划。
- **`host_transform_to_mc(location, rotation_euler, scale, bounds=None, origin_centered=True, coordinate_system="z_up") -> (pos, rot, scale)`**：反向逆变换算子，将宿主视口刚体变换还原为规范 Minecraft 世界坐标 `((pos_x, pos_y, pos_z), (pitch, yaw, roll), (sx, sy, sz))`，实现视口提取时的前后端单一事实源（SSOT）。
- **`EntityRenderPlan`**：纯数据契约 DTO。
  - 属性：`schema_version`, `revision`, `authoritative`, `bounds`, `node_count`, `nodes`, `deleted_keys`, `warnings`
  - 方法：`to_json() -> str`
- **`EntityRenderNode`**：单一实体渲染节点。
  - 属性：`key` (稳定唯一键), `display_name` (规范命名), `location` ((x, y, z)), `rotation_euler` ((pitch, yaw, roll)), `scale` ((sx, sy, sz)), `matrix` ([f32; 16]), `mesh` (局部空间 `PyMeshData`), `material_routes` (材质槽位列表), `metadata` (元数据字典), `entity_json` (原始实体 JSON 描述)。

### 2.12 性能探针与执行耗时审计 (`profile`)
- **`ExecutionProfile` (`PyExecutionProfile`)**：端到端操作规范性能报告对象。
  - 属性：`total_duration_ms`（总壁钟耗时）, `accounted_duration_ms`（顶层阶段累计耗时）, `unaccounted_duration_ms`（未统计工作耗时盲区）, `stages`（层级阶段列表）。
  - 方法：`to_dict()` 转换为包含 `name`, `category`, `depth`, `start_offset_ms`, `duration_ms`, `items_count` 的结构化字典。
- **`StageSpan` (`PyStageSpan`)**：规范化阶段跨度。
  - 属性：`name`（阶段名）, `category`（命名空间/类别如 `"mesh"`, `"io"`, `"bake"`, `"cull"`, `"host"`）, `depth`（层级深度，0=顶层，1=子阶段）, `start_offset_us`, `start_offset_ms`（相对启动时刻偏移）, `duration_ms`, `items_count`。

### 2.13 测试 / 开发专用 API (`unstable.testing`)
以下低阶算子不属于稳定生产 API，仅供测试验证与开发工具调用：
`AtlasBuilder`, `ModelBaker`, `calculate_face_target_grid`, `calculate_pixel_grid_cut_factors`,
`slice_face_by_pixel_grid`, `is_uv_collapsed`, `is_orthogonal_angle`, `detect_uv_rotation`,
`straighten_uv`, `normalize_uv_for_atlas_tiling`, `uv_requires_atlas_tiling`,
`restore_atlas_tiling_uv`, `repair_quad_fluid_uv`, `batch_repair_fluid_uv`,
`repair_extruded_side_uv`, `sample_uv_alpha_f32`, `decode_packet`, `encode_welcome`,
`encode_baseline_begin`, `encode_baseline_chunk`, `encode_baseline_end`, `encode_delta_op`,
`encode_palette_update`, `mesh_attribute_schema`, `retired_mesh_attributes`,
`extract_cache_atlas_texture`, `model_database_get_states`, `model_database_remap_to_atlas`,
`model_database_get_mesh`, `voxel_storage_get_non_empty_sections`, `voxel_storage_get_block_entity_json`,
`voxel_storage_set_block_entity_json`, `voxel_world_ensure_dynamic_entities_resolved_sync`,
`voxel_dirty_section_count`, `voxel_set_section_snapshot`, `validate_scene_package`,
`create_colormaps_from_bytes`。

---

## 3. 构建、编译与测试指南

> [!IMPORTANT]
> 必须严格在当前工作区的虚拟环境 (`./.venv`) 中执行构建，严禁使用全局 Python 或 `pip install` 污染全局 `bpy`！

### 3.1 本地极速直连开发（Dev Direct-Link，推荐）
```bash
cd <libmozitoolkit 工作区根目录>

# 一键编译所有供 Blender 使用的原生扩展（mtk-py + mtk-blender-dna-py）
# 自动启用 --features extension-module 杜绝 Blender 载入段错误
cargo build-all-py

# 或仅构建 mtk-py:
cargo build-py

# 构建产物位于 target/release/，已由 MoziToolKit/dev/loader.py 自动软链直通
```

### 3.2 生产环境 Wheel 打包构建（Release Packaging）
```bash
cd <libmozitoolkit 工作区根目录>
source .venv/bin/activate

# 本地开发模式构建 (生成动态库供测试)
maturin develop -m bindings/mtk-py/Cargo.toml

# 生产 Release 轮子构建 (产出至 target/wheels/，maturin 自动注入 extension-module)
maturin build --release -m bindings/mtk-py/Cargo.toml
```

### 3.3 同步至 Blender 插件前端 (`MoziToolKit`)
```bash
# 将编译生成的 .whl 拷贝到 MoziToolKit 的 wheels 目录（仅发布打包时需要）
cp target/wheels/libmtk_py-*.whl ../MoziToolKit/wheels/
```

### 3.4 运行 Python 绑定测试
```bash
# 运行 mtk-py 专属单元测试
pytest bindings/mtk-py/tests/

# 运行 MoziToolKit 的 Bridge 测试套件
pytest ../MoziToolKit/tests/
```

---

## 4. 真实代码调用示例

### 示例 1：零拷贝网格创建与 Blender 数据灌入
```python
import libmtk_py
import numpy as np

# 1. 构造连续几何缓冲
positions = [
    0.0, 0.0, 0.0,
    1.0, 0.0, 0.0,
    1.0, 1.0, 0.0,
    0.0, 1.0, 0.0,
]
uvs = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]
indices = [0, 1, 2, 0, 2, 3]

mesh = libmtk_py.MeshData.from_flat_polygons(
    positions, [0, 1, 2, 3], uvs, [0], [4], None, [0]
)
print(f"Vertices: {mesh.vertex_count}, Faces: {mesh.face_count}")

# 2. 零拷贝提取 memoryview 并在 NumPy / Blender foreach_set 中使用
pos_mv = mesh.positions_memoryview()
pos_np = np.frombuffer(pos_mv, dtype=np.float32).reshape(-1, 3)

# 3. 添加自定义属性
mesh.add_string_attribute("mtk_source_texture", "face", ["minecraft:block/stone", "minecraft:block/stone"])
print("Has attribute:", "mtk_source_texture" in mesh.attribute_names())
```

### 示例 2：体素世界存储与多核并行网格化 (`VoxelWorld`)
```python
import libmtk_py

storage = libmtk_py.VoxelStorage()
storage.set_bounds(0, 0, 0, 16, 16, 16)
storage.set_block(0, 0, 0, "minecraft:stone")

config = libmtk_py.MesherConfig(enable_ao=True, z_up_coordinates=True)
world = libmtk_py.VoxelWorld.from_storage(storage, config)
mesh = world.rebuild_all()
print("Vertices:", mesh.vertex_count, "Quads:", mesh.quad_count)
```

---

## 5. Feature 开关与依赖

`mtk-py` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `extension-module` | 否 | 严禁设为默认（否则 `cargo test` 会因缺失 `libpython` 符号报错）。由 `cargo build-py` / `cargo build-all-py` 或 `maturin` 在构建 Python C 扩展动态链接库时启用。 |
| `zip` | 是 | 传递给 `mtk-resource`，支持直接读取 `.zip` / `.jar` 资源包。 |
| `parallel` | 是 | 启用 `rayon` 多线程并发加速（模型展开、体素网格化、图像处理）。 |

---

## 6. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
