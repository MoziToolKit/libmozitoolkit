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

- `version() -> str`：原生扩展版本。
- `api_version() -> int`：与 Blender 胶水层握手的整数 API 版本（当前 `1`）。

### 2.1 几何网格与自定义属性 (`mesh` & `attributes`)
- **`MeshData` (`PyMeshData`)**：连续内存紧凑几何网格缓冲容器。
  - 属性：`vertex_count`, `triangle_count`, `face_count`, `is_empty`
  - 构造与修改：`MeshData()`, `MeshData.with_capacity(verts, indices, faces)`, `MeshData.from_flat_polygons(...)`, `MeshData.from_polygon_offsets(positions, loop_vertices, loop_uvs, face_offsets, normals=None, face_materials=None)`（CSR 面偏移直接构造）, `append_mesh(other)`, `clear()`
  - UV 布局：`uv_layout` 显式区分 `PerVertex` / `PerLoop`，不再用 `uvs.len()` 猜测；`loop_uvs_memoryview()` 自动展开为逐 loop。
  - 内存视图（标准零拷贝通道，推荐）：`positions_memoryview()`, `normals_memoryview()`, `uvs_memoryview()`, `indices_memoryview()`, `quad_indices_memoryview()`, `loop_uvs_memoryview()`, `loop_starts_memoryview()`, `loop_totals_memoryview()`, `face_materials_memoryview()`
  - 核心 wheel 不再包含 Blender 指针写入代码。零拷贝宿主写入由独立 `libmtk_blender_dna` wheel 提供，通过 `memoryview` 接收核心 buffer，再写入 Blender 指针；核心不需要知道适配器的存在。
  - 次级 UV / 面 tint：`get_flat_secondary_uvs()`, `face_tint_indices_memoryview()`
  - 顶点与面自定义属性：`add_string_attribute(name, domain, values)`, `add_string_attribute_from_palette(name, domain, palette, indices)`, `add_attribute_from_buffer(name, domain, data_type, buffer)`, `get_attribute_data(name)`, `has_attribute(name)`, `attribute_names()`
  - `add_string_attribute_from_palette(...)`：从 palette + u32 索引缓冲区在 Rust 内展开字符串，宿主无需为每个面创建 Python 字符串对象。
  - 来源贴图调色板：`source_texture_palette_indices() -> (palette, u32 memoryview)`，把逐面 `mtk_source_texture_key` 压缩成 palette + 索引，供 Live Sync 等跳过逐面字符串的宿主保留完整 provenance。
- **`AttributeDomain`**：属性域枚举，支持 `"point"` (顶点) 与 `"face"` (面)。
- **`mesh_attribute_schema() -> list[tuple[str, str, str, str]]`**：返回规范网格属性 `(name, domain, data_type, role)` 列表，是宿主唯一权威的属性契约来源。
- **`retired_mesh_attributes() -> list[str]`**：返回已退役、只允许清理的属性名清单。
- **`analyze_mesh_hard_edges(positions, edge_verts, face_loop_starts, face_loop_totals, loop_verts, sharp_angle_rad, explicit_sharp_edges=None) -> PyBytes`**：基于二面角与边界的高性能硬边/锐边拓扑分析算子，支持零拷贝 Buffer Protocol。

### 2.2 遮挡剔除与状态机 (`cull`)
- **`FaceCuller` (`PyFaceCuller`)**：原版方块与复杂连接体遮挡状态机。
  - `FaceCuller(leaves_cull_mode=0, glass_cull_mode=0)`
  - `is_face_occluded(block, neighbor, direction, pos_a=None, pos_b=None)`
  - `clear_cache()`, `cache_len`

### 2.3 自适应像素网格切分 (`subdivide`)
- **`calculate_face_target_grid(uvs, tex_w, tex_h, max_grid)`**：根据 UV 跨越的纹理像素计算切分密度。
- **`calculate_pixel_grid_cut_factors(uvs, tex_w, tex_h, margin, max_subdiv)`**：计算 U/V 方向的归一化网格切分线。
- **`slice_face_by_pixel_grid(positions, uvs, tex_w, tex_h, margin, max_subdiv)`**：对单多边形面进行自适应多边形裁剪与像素对齐切分。
- **`adaptive_pixel_split_mesh(mesh, ...)`**：整网格批量自适应像素切分。

### 2.4 智能挤出与 UV 修复 (`extrude` & `uv`)
- **挤出算子**：
  - `process_flat_mesh_extrude_repair(positions, uvs, indices, face_mat, ...)`：扁平数组极速挤出修复。
  - `generate_random_extrude_heights(centers, ...)`：为随机离散挤出生成高度。
- **UV 几何数学算法**：
  - `repair_mesh_fluid_uv_inplace(positions, loop_verts, face_loop_starts, face_loop_totals, uvs, ...)`：基于网格索引拓扑的高性能原地流体 UV 斜坡批量修复。
  - `calculate_uv_area(quad_uvs)`、`get_uv_bounds(quad_uvs)`、`get_uv_center(quad_uvs)`
  - `is_uv_collapsed(quad_uvs)`、`detect_uv_rotation(quad_uvs)`、`straighten_uv(quad_uvs)`
  - `scale_uv(quad_uvs, factor)`、`normalize_uv_for_atlas_tiling(quad_uvs)`、`restore_atlas_tiling_uv(...)`
  - `repair_quad_fluid_uv(...)`、`batch_repair_fluid_uv(...)`、`get_fluid_top_uvs()`、`get_fluid_side_uvs()`

### 2.5 体素存储与网格化 (`voxel` & `mesher`)
- **`VoxelPointCloud` (`PyVoxelPointCloud`)**：全量无剔除 3D 体素点云容器，支持 Blender 点网格 attributes 往返和零拷贝内存视图。
  - `to_mesh_data() -> MeshData`, `from_mesh_data(mesh) -> VoxelPointCloud`
  - `to_storage() -> VoxelStorage`, `from_indexed_arrays(...)`
  - `positions_memoryview()`, `block_x_memoryview()`, `block_y_memoryview()`, `block_z_memoryview()`
  - `get_state_palette()`, `get_biome_palette()`, `state_indices_memoryview()`, `biome_indices_memoryview()`：点云只存 palette + 索引，不再逐点物化字符串。
- **`VoxelStorage` (`PyVoxelStorage`)**：16x16x16 稀疏 Chunk Section 体素容器，带脏标记追踪。
  - `to_point_cloud(config=None) -> VoxelPointCloud`, `from_point_cloud(cloud) -> VoxelStorage`, `load_from_point_cloud(cloud)`
  - `create_debug_world()`：一键加载内置嵌入的原版调试世界快照（529 sections, 32,539 blocks）。
  - `set_bounds(min_x, min_y, min_z, size_x, size_y, size_z)`, `set_block(x, y, z, state, tint)`, `get_block(x, y, z)`
  - `dirty_section_count()`, `clear_dirty_sections()`
- **`VoxelWorld` (`PyVoxelWorld`)**：统一 3D 场景引擎，支持增量区块缓存与全量世界网格管理。
  - `to_point_cloud() -> VoxelPointCloud`, `load_from_point_cloud(cloud)`
  - `from_storage(storage, config=None, culler=None, model_db=None, num_threads=None)`
  - `create_debug_world(config=None, culler=None, model_db=None, num_threads=None)`
  - `rebuild_all() -> MeshData`, `rebuild_dirty() -> Dict`, `clear_cache()`, `hot_reload(config=None, model_db=None)`, `num_threads` (getter/setter)
- **`MesherConfig` (`PyMesherConfig`)**：网格化配置（AO 开关、流体曲面、坐标系转换、线程数、顶点焊接）。
- **`LiveSyncSession` (`PyLiveSyncSession`)**：原生 WebSocket 实时协同客户端。
  - `start(url, auto_reconnect=True, max_reconnect_attempts=5, auth_token=None)`, `stop()`
  - `auth_token` 会随默认 HELLO 发送（RFC v3.1 §4.1；Python 侧先 `strip()`；
    两端令牌会各自 trim 后比较），启用 Yefira `authToken` 时必须传入。
  - `poll_events(coalesce_meshes=False)`：`True` 时只保留最新 `WORLD_MESH_READY` generation
  - `get_dynamic_entity_atlases()`：`{chunk_id: RGBA bytes}` 动态实体图集映射（无单数访问器）
  - 持久化握手入口（`start_with_persisted_state` / `export_persisted_state_json` /
    `start()` 的 `cached_*` 参数）已物理删除：Blender 侧 `VoxelStorage` 无法持久化，
    该路径始终不可达。
  - `client_generation`, `current_sequence`, `current_epoch`, `state_hash`, `server_uuid`
  - `clear_cache()`, `set_config(config)`, `hot_reload(config=None, model_db=None)`, `request_resync()`
  - `STREAM_PROGRESS.percent` 已由 Rust 归一化为 0..100；`HANDSHAKE.decision_code` 为 RFC v3.1 原始值
- **二进制协议编解码**：`decode_packet(bytes)`, `compute_canonical_state_hash(...)`；服务端测试用的
  `encode_welcome(...)` / `encode_baseline_*` / `encode_delta_op` / `encode_palette_update` 位于
  `libmtk_py.unstable.testing`。

### 2.7 资源包预烘焙与图集 (`resource` & `texture`)
- **`ResourcePackStack` (`PyResourcePackStack`)**：虚拟文件系统 (VFS) 资源包层叠加载器。
- **`BakedAtlas`**：烘焙图集容器；测试/开发用的 `AtlasBuilder` 位于 `libmtk_py.unstable.testing`。
- **`precompile_all_assets(..., resolver_overrides_json=None)`**：端到端一键无头预烘焙管线；可传入 Python 维护的兼容规则 JSON。
- **像素分析**：`batch_analyze_transparent_faces_f32`, `batch_analyze_transparent_faces_u8`；单点采样 `sample_uv_alpha_f32` / `is_face_transparent_f32` 位于 `unstable.testing`。

### 2.8 生物群系与材质解析 (`material`)
- **`BiomeResolver` (`PyBiomeResolver`)**：原版 66 种生物群系调色板映射与混合器。
- **`MaterialResolver` (`PyMaterialResolver`)**：着色器材质槽位与外部别名解算器。
- **即时 Biome 重着色**：`reapply_uniform_biome_tint(base_tt, packed, old_colors, colors..., flags...)`
  返回两个零拷贝 memoryview（`packed_tint_data` / `tint_colors`），供 `foreach_set` 直接写入。
  - `remap_mesh_multi_uvs(..., enable_animated_textures=false)` / `remap_mesh_multi_uvs_paletted(...)`
  - `remap_mesh_multi_uvs_paletted(..., palette, face_indices, ...)`：palette-native、零拷贝 id/metric buffer
  - `resolve_mesh_routes(face_texture_keys, atlas, aliases=None, enable_animated_textures=false)`
  - `reroute_mesh_animation(uvs, face_loop_ranges, face_texture_keys, face_old_chunk_ids, atlas, aliases=None, enable_animated_textures=false)`
- **`GridAtlasSpec` (`PyGridAtlasSpec`)**：网格图集元数据规范。
- **缓存解析**：`resolve_active_cache_package(cache_dir, fingerprint=None)` 返回 active `.mtkcache` 路径。
- **色彩数学与常量**：`compute_biome_tint_attributes`, `compute_mesh_biome_attributes_from_storage`, `sample_colormap_colors`, `blend_biome_colors`, `sample_colormap_pixel`, `srgb_to_linear`, `linear_to_srgb`, `TINT_TYPE_NONE..DRY_FOLIAGE`

### 2.9 模型烘焙 (`model`)
- **`BakedModelDatabase` (`PyBakedModelDatabase`)**：预编译方块几何模型库。
  - `get_mesh(state_str, clip_hidden=true)`, `remap_to_atlas(atlas)`, `get_states()`

### 2.10 场景交换包与 Mini-Atlas (`scene`)
- **`ScenePackageReader`**：`.mtkscene` 单文件中间交换容器读取器。
  - `open(path)`：快速打开并读取场景元数据；
  - `metadata -> SceneMetadata`：获取场景 AABB 范围、体素总量与图集分辨率等；
  - `read_point_cloud() -> VoxelPointCloud`：提取稀疏体素列存并构造点云；
  - `read_models() -> BakedModelDatabase`：读取已重映射至 Mini-Atlas 的 Tree-Shaken 方块模型库；
  - `read_atlas() -> BakedAtlas`：读取内嵌的自包含微型多通道 PBR 图集；
  - `read_atlas_mapping_json() -> str`：获取图集 UV 映射 JSON 字符串。
- **`SceneMetadata`**：场景包元数据对象。
  - 属性：`format_version`, `bounds`, `voxel_count`, `unique_states_count`, `unique_biomes_count`, `unique_entities_count`, `atlas_resolution`, `generator`, `created_at_epoch_secs`；
  - `to_dict() -> dict`：快速转化为 Python 字典。
- **`export_scene_package(path, point_cloud, cache, max_atlas_size=2048, mip_level=0) -> SceneMetadata`**：
  端到端场景交换导出算子：自动扫描伴生体素点云、执行模型树剪枝、从激活缓存裁剪贴图、拼接微型多通道图集、并行重映射模型 UV 并压缩打包为 `.mtkscene`。
- **稀疏体素数据结构 (`mtk_voxel`)**：
  - `SparseVoxelPayload` / `SparseVoxelColumns`：正交离散调色板（`state_palette`, `biome_palette`, `entity_palette`）与紧凑 SoA 列存布局，实现与伴生体素点云的 1:1 零拷贝互转。

### 2.11 测试 / 开发专用 API (`unstable.testing`)
以下低阶入口不属于稳定 API，仅在测试、Mock 服务端和开发工具中使用：
`AtlasBuilder`, `ModelBaker`, `calculate_face_target_grid`, `calculate_pixel_grid_cut_factors`,
`slice_face_by_pixel_grid`, `is_uv_collapsed`, `is_orthogonal_angle`, `detect_uv_rotation`,
`straighten_uv`, `normalize_uv_for_atlas_tiling`, `uv_requires_atlas_tiling`,
`restore_atlas_tiling_uv`, `repair_quad_fluid_uv`, `batch_repair_fluid_uv`,
`repair_extruded_side_uv`, `sample_uv_alpha_f32`, `is_face_transparent_f32`,
`encode_*`, `voxel_dirty_section_count`, `voxel_set_section_snapshot`, `voxel_get_all_blocks`。

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
print("Has attribute:", mesh.has_attribute("mtk_source_texture"))
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
