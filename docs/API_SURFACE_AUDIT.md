# API 暴露面审计与清理准备 (2026-11-05)

> 本文是**诊断快照**，不是兼容承诺。结论按“零消费者即删除”执行；下一轮清理时直接改调用方与文档，不保留废弃层。
>
> 审计对象：`libmozitoolkit` 的 Rust crate 根导出、Python 稳定面，以及唯一消费者 `../MoziToolKit`。
> 基线：当前工作区 `bindings/mtk-py` 已构建，`../MoziToolKit/dev/lib/libmtk_py.so` 可导入。

---

## 1. 摘要 (Executive Summary)

| 指标 | 当前值 | 说明 |
| :--- | ---: | :--- |
| Python 稳定顶层类 | 25 | 不含 `unstable.testing` |
| Python 稳定顶层方法 | 266 | 类方法/属性合计 |
| Python 稳定顶层函数 | 37 | 不含 7 个常量 |
| Python `unstable.testing` 入口 | 27 | 其中至少 2 个零消费者 |
| Rust crate 根 `pub use` 名称 | 497 | 含 bindings/adapters |
| 其中无跨 crate 代码消费者 | 238 | 约 **48%** |
| 核心 crate 根 `pub use` 名称 | 440 | 不含 bindings/adapters |
| 核心 crate 中无跨 crate 代码消费者 | 205 | 约 **47%** |
| 核心 crate `pub mod` 数量 | 78 | 内部模块也全部对外可达 |
| 零消费者整 crate | `mtk-wasm` | 无任何 Python/Rust/前端调用方 |

**核心结论：**

1. Python 稳定面明显大于真实生产消费面。大量方法只被 Python 测试或工具脚本使用，却占据稳定顶层。
2. Rust crate 根导出接近一半没有跨 crate 消费者；同时内部模块普遍 `pub mod`，导致“没有根导出”也不等于“没有暴露”。
3. 最近围绕 `.mtkscene` / colormap / capabilities 的提交新增了多层 DTO、验证对象、能力探测与全局色盘状态，随后又删掉其中一层（`PySceneCapabilities`），说明前期缺少“先确认消费者再暴露”的门禁。
4. 文档已明显滞后：`bindings/mtk-py/README.md` 仍写 `api_version = 1`，并列出多个已删除的方法；`docs/API_REFERENCE.md` / `docs/BINDINGS_DESIGN.md` 仍引用已不存在的 Python API。
5. 本轮不执行删除；下一轮按第 7 节批次直接物理删除并同步调用方/文档。

---

## 2. 审计方法 (Methodology)

### 2.1 Python 稳定面

通过已构建模块内省：

```bash
cd ../MoziToolKit
PYTHONPATH=dev/lib python3 - <<'PY'
import inspect, libmtk_py
for name in sorted(n for n in dir(libmtk_py) if not n.startswith("_")):
    obj = getattr(libmtk_py, name)
    if inspect.isclass(obj):
        methods = [m for m in dir(obj) if not m.startswith("_")]
        print("CLASS", name, len(methods), methods)
    elif callable(obj):
        print("FUNC", name)
    else:
        print("CONST", name, obj)
PY
```

然后对 `../MoziToolKit` 全部 `.py` 做符号检索，排除 `.venv`、`__pycache__`、`dev/lib`，并按 `bridge/operators/ui/utils/dev`（生产）、`tests`、`tools` 分类。

- 生产：`bridge/`、`operators/`、`ui/`、`utils/`、`dev/` 等。
- 测试：`tests/`。
- 工具：`tools/`。

### 2.2 Rust 根导出

对每个 `crates/*/src/lib.rs` 的 `pub use` 解析导出名，然后在工作区非测试 `.rs` 中统计**定义 crate 之外**的代码引用。`test_hits` 仅作参考，不视为外部消费者。

- “零外部”= 除定义 crate 自身与测试/bench 外，没有任何代码引用该名称。
- 该统计不替代人工判断：若一个符号是 CLI 的稳定输出契约，即使只被 CLI 内部使用，也应视为有意公开；反之，仅为方便 `pub use` 的转发应删除。

### 2.3 范围限制

- `../yefira` 是 Java 模组，只共享协议；没有 Python 调用方，因此 Python 消费者检索以 `../MoziToolKit` 为准。
- 本次未做运行时性能回归；清理后必须重新跑完整门禁。

---

## 3. Python 稳定面诊断

### 3.1 P0：完全零消费者，直接删除

| 符号 | 位置 | 说明 |
| :--- | :--- | :--- |
| `compute_canonical_state_hash` | 顶层函数 | Python 侧零调用；生产使用 `VoxelStorage.compute_sparse_state_hash()` / `VoxelPointCloud.compute_sparse_state_hash()`。Rust 内部仍保留即可。 |
| `PyBufferOwner` | 顶层类注册 | 模块内零 Python 调用；仅作为绑定内部 buffer owner 使用，不应注册为公开类。 |
| `MeshData.has_dynamic_entity_chunks` | `MeshData` 属性 | 零 Python 调用；`dynamic_entity_atlases` 已足够判断。 |
| `AssetCache.is_valid` | `AssetCache` 方法 | 零调用；与顶层 `is_valid_cache_package(path, fingerprint)` 重复。 |
| `ScenePackageReader.read_ctm_rules_json` | `ScenePackageReader` 方法 | 零调用；`read_ctm_solver()` / `unpack()` 已覆盖。 |
| `VanillaColormaps.load_embedded` | `VanillaColormaps` 方法 | 零调用。 |
| `VanillaColormaps.sample_grass_linear` | `VanillaColormaps` 方法 | 零调用。 |
| `VanillaColormaps.sample_foliage_linear` | `VanillaColormaps` 方法 | 零调用。 |
| `VanillaColormaps.sample_dry_foliage_linear` | `VanillaColormaps` 方法 | 零调用。 |
| `VoxelWorld.set_model_db` | `VoxelWorld` 方法 | 零调用；`CHANGELOG` 早已记录删除，但代码仍注册。 |

### 3.2 P1：仅测试/工具使用，下沉 `unstable.testing` 或删除

> 稳定顶层只允许生产消费者。以下符号若测试仍需要，统一放 `unstable.testing`；若测试可改用生产 API 或 Rust 单元测试，则直接删除。

#### 协议 / 工具类

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `decode_packet` | `tools/mock_sync_server.py`、测试 | 与 `encode_*` 一起放 `unstable.testing` |
| `mesh_attribute_schema` | `tools/sync_attribute_schema.py`、测试 | 若保留为工具契约，放 `unstable.testing`；否则删除 |
| `retired_mesh_attributes` | `tools/sync_attribute_schema.py`、测试 | 同上 |
| `clear_cached_colormaps` | 测试 | 删除全局色盘缓存 API；色盘显式传参 |
| `AssetCache.extract_atlas_texture` | `tools/validate_pbr_visual_regression.py` | 移 `unstable.testing` 或删除 |
| `is_face_transparent_f32` | 无 | 删除 |
| `voxel_get_all_blocks` | 无 | 删除 |

#### `MeshData`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `append_mesh` | 测试 helper | 删除 Python 导出；Rust 内部保留 |
| `has_attribute` | 测试 | 删除；`attribute_info` 可替代 |
| `loop_starts_memoryview` | 测试 | 删除；`to_buffers`/Rust 内部保留 |
| `loop_totals_memoryview` | 测试 | 同上 |
| `source_texture_palette_indices` | 测试 | 删除；生产从 Blender 属性直接读取 |
| `to_buffers` | 测试 | 若无宿主无关消费者，删除或移 `unstable.testing` |
| `uv_layout` | 测试 | 删除；如确需调试信息，放 `unstable.testing` |

#### `MesherConfig`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `has_atlas` | 测试 | 删除 |
| `has_biome_resolver` | 测试 | 删除 |
| `has_colormaps` | 测试 | 删除 |
| `set_bounds` | 测试 | 删除；边界由 `VoxelStorage` / `VoxelWorld` 管理 |

#### `BakedModelDatabase`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `contains` | 测试 | 删除；`get_mesh` 返回 `Option` 即可判断 |
| `get_states` | 测试 | 移 `unstable.testing` 或删除 |
| `remap_to_atlas` | 测试 | 移 `unstable.testing` 或删除 |

#### `VoxelPointCloud`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `push` | 测试 | 删除；测试改用 `from_indexed_arrays` |
| `from_mesh_data` | 测试 | 删除；测试改用 `from_indexed_arrays` / `to_storage` |
| `to_mesh_data` | 测试 | 删除；测试改用 `to_buffers` 或生产路径 |
| `to_buffers` | 测试 | 与 `MeshData.to_buffers` 一并决策；无消费者则删除 |

#### `VoxelStorage`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `clear` | 无生产调用 | 删除 Python 导出 |
| `get_block` / `set_block` | 测试 | 删除；生产使用点云批量导入 |
| `get_bounds` / `set_bounds` | 测试 | 删除；边界由导入/构造决定 |
| `get_non_empty_sections` | 测试 | 移 `unstable.testing` |
| `get_block_entity_json` / `set_block_entity_json` | 测试 | 移 `unstable.testing` 或删除 |

#### `VoxelWorld`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `clear` | 无生产调用 | 删除 Python 导出 |
| `get_block` / `set_block` | 测试 | 删除；生产使用 bulk 流程 |
| `get_bounds` / `set_bounds` | 测试 | 删除 |
| `get_block_entity_json` / `set_block_entity_json` | 测试 | 移 `unstable.testing` 或删除 |
| `ensure_dynamic_entities_resolved_sync` | 测试 | 删除；解析应由内部流程自动完成 |

#### `ScenePackageReader` / `SceneMetadata` / `ValidationReport`

| 符号 | 当前消费者 | 建议 |
| :--- | :--- | :--- |
| `ScenePackageReader.effective_capabilities` | 测试 | 删除；`metadata` 直接给 dict |
| `ScenePackageReader.has_colormaps` | 测试 | 删除；`unpack` 结果中的 `colormaps` 是否为 `None` 即可判断 |
| `ScenePackageReader.has_ctm_rules` | 测试 | 删除；`read_ctm_solver()` 是否为 `None` 即可判断 |
| `ScenePackageReader.has_dynamic_entity_slots` | 测试 | 删除；`read_dynamic_entity_slots()` 是否为空即可判断 |
| `ScenePackageReader.install_colormaps` | 测试 | 删除；全局安装与 Data In/Out 冲突 |
| `ScenePackageReader.read_atlas_mapping_json` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.read_biome_resolver` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.read_colormap` | 测试 | 删除；`unpack` 已返回 |
| `ScenePackageReader.read_colormaps` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.read_ctm_solver` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.read_dynamic_entity_slots` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.read_models` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.read_point_cloud` | `bridge/scene.py` fallback、测试 | `unpack` 已返回；删除单独读取 |
| `ScenePackageReader.validate` | 测试 | 若保留，返回普通 dict；否则删除 |
| `ScenePackageReader.extract_atlas_texture` | `tools/` | 移 `unstable.testing` 或删除 |
| `SceneMetadata.capabilities` | 测试 | 删除；能力字段直接并入 `metadata` dict |
| `ValidationReport` | 仅 `validate()` 测试 | 随 `validate()` 一起删除，或改为 dict |

> 关键结论：`ScenePackageReader` 从 24 个方法收敛为 `open` + `metadata`（dict）+ `unpack` + `read_texture_rgba/f32`（图集切片访问）即可覆盖生产路径。`AssetCache` 与 `ScenePackageReader` 的重复 `read_texture_*` 保留一个内部实现，不新增公开 trait。

### 3.3 过度繁杂的稳定 API

#### 3.3.1 生物群系染色

- `compute_biome_tint_attributes` 当前 15 个参数，并同时存在 `compute_biome_tint_attributes_paletted`、`compute_mesh_biome_attributes_from_storage`、`reapply_uniform_biome_tint`。
- Rust 侧 `mtk-material` 根导出中同时存在 `compute_mesh_biome_attributes`、`_custom`、`_custom_with_colormaps`、`_with_colormaps`、`_paletted_flat`、`_per_face`、`reapply_uniform_biome_tint`、`_flat` 等多套变体，其中大部分无跨 crate 消费者。
- 同时存在进程级 colormap 缓存 API（`set_cached_colormaps`、`clear_cached_colormaps`、`has_active_colormaps_override`），与 Data In/Data Out 冲突。

**建议**：收敛为一个 `BiomeTintRequest` 强类型 DTO + 一个批量入口；colormap 作为显式参数传入，不保留全局安装/清除 API；只保留生产实际调用的 `compute_biome_tint_attributes_paletted` 与 `reapply_uniform_biome_tint`。

#### 3.3.2 `MeshData`

35 个方法中包含 13 个 `*_memoryview`、通用属性读写、`to_buffers` 与多个测试专用访问器。生产实际使用集中在：

- 构造：`from_flat_polygons` / `from_polygon_offsets`
- 计数：`vertex_count` / `triangle_count` / `face_count` / `quad_count`
- 缓冲：`positions_memoryview` / `normals_memoryview` / `uvs_memoryview` / `indices_memoryview` / `quad_indices_memoryview` / `loop_uvs_memoryview` / `face_materials_memoryview` / `colors_memoryview`
- 属性：`add_attribute_from_buffer` / `add_string_attribute` / `get_string_attribute` / `attribute_info` / `attribute_names` / `attribute_memoryview`
- 其他：`profile` / `used_materials` / `compact_materials` / `dynamic_entity_atlases` / `dynamic_entity_slots`

**建议**：删除 3.2 中的测试专用方法；`to_buffers` 若保留，必须是唯一宿主无关出口，禁止再新增 `to_numpy()` / `to_flat_*`。

#### 3.3.3 `VoxelStorage` / `VoxelWorld` / `LiveSyncSession`

- 三者合计 71 个方法，存在大量重复：`get_storage` / `get_world_mesh` / `set_config` / `set_skin_cache_dir` / `hot_reload` / `get_dynamic_entity_atlases` / `get_dynamic_entity_slots`。
- `VoxelStorage` / `VoxelWorld` 的逐方块与边界方法在 Python 生产中零调用，只被测试用来搭建 fixture。
- `LiveSyncSession` 的 `start` / `start_with_cache` 是两条真实路径，可保留；其余“便利别名”不应再增加。

**建议**：删除测试专用逐方块/边界/JSON 方法；生产路径统一为“点云/稀疏列存 → Storage → World → Mesh”的批量链路。若需要测试 fixture，在 Rust 单元测试或 `unstable.testing` 中构造，不污染稳定面。

#### 3.3.4 `unstable.testing`

当前 27 个入口中，`is_face_transparent_f32`、`voxel_get_all_blocks` 完全零消费者；其余多为测试对齐用途。它不是“随便加”的垃圾桶：只允许放真实测试/工具需要、且无法用生产 API 覆盖的底层算子；新增前同样要过第 9 节准入清单。

### 3.4 DNA 适配器 wheel (`libmtk_blender_dna`)

生产实际使用：`api_version`、`copy_f32/i32/u32/u8`、`as_memoryview_f32/i32/u32/u8`。

零消费者：

| 符号 | 说明 | 建议 |
| :--- | :--- | :--- |
| `read_f32` | 无 Python 调用 | 删除；如未来需要直读，再按准入清单新增 |
| `read_i32` | 无 Python 调用 | 删除 |
| `read_u32` | 无 Python 调用 | 删除 |
| `read_u8` | 无 Python 调用 | 删除 |
| `as_memoryview_raw` | 仅契约测试提及，无生产调用 | 删除；未类型化裸视图本身也违反规则 4 |

删除 Python `read_*` 后，`mtk-blender-dna` 中对应的 `read_*_from_ptr` 也会失去跨 crate 消费者，应一并降为 `pub(crate)` 或删除。

---

## 4. Rust 根导出诊断

### 4.1 按 crate 统计

| Crate | 根 `pub use` 名称 | 无跨 crate 消费者 | 典型零消费者示例 |
| :--- | ---: | ---: | :--- |
| `mtk-core` | 56 | 16 | `process_mesh_extrude_repair`、`process_random_extrude_mesh`、`ExtrudeMeshInput/Output`、`RandomExtrudeMeshInput/Output`、`interpolate_bilinear_*`、`mc_local_to_centered_z_up` |
| `mtk-cull` | 31 | 15 | `subtract_rect`、`fluid_group`、`is_same_fluid`、`is_face_occluded_by_state`、`should_skip_rendering`、`is_empty_rect`、`is_full_rect` |
| `mtk-model` | 46 | 30 | `ElementJson`、`FaceJson`、`RotationJson`、`VariantEntry`、`BakedFaceGeometry`、`bake_face_exact`、`calculate_facing`、`rotate_*` |
| `mtk-voxel` | 60 | 31 | `VoxelSource`、`VoxelReader`、`ingest_from_source`、`DeltaMesher`、`SparseVoxelColumns`、`calculate_face_ao*`、`emit_fluid_geometry` |
| `mtk-sync` | 23 | 14 | `SyncClient`、`ClientCommand`、`ClientMessage`、`BaselineAssembler`、`DeltaStagingBuffer`、`Opcode`、`PROTOCOL_MAGIC` |
| `mtk-resource` | 40 | 16 | `CTM_47_LOOKUP`、`OVERLAY_17_LOOKUP`、`coordinate_random`、`get_face_tangents`、`PbrCompanions`、`UnstitchRegion` |
| `mtk-texture` | 43 | 18 | `StitchedAtlas/Chunk/Slot`、`StitcherHolder/Region`、`DynamicSlotInfo`、`sample_alpha_u8`、`batch_analyze_transparent_faces_u8` |
| `mtk-material` | 86 | 40 | `sample_colormap_pixel*` 系列、`remap_*` 底层数学、`PbrDecode*`、`TintInfo`、`Mesh*Result` 包装类型 |
| `mtk-save` | 7 | 3 | `ChunkParser`、`RegionFile`、`decode_block_entity`（仅 crate 内部/测试） |
| `mtk-package` | 16 | 8 | `ChecksumType`、`CompressionType`、`HeaderFlags`、`TableOfContents`、`ChunkEntry` |
| `libmtk` | 32 | 14 | `SceneExportResult`、`SceneWriteRequest`、`ScenePackageWriteOptions`、`SceneBundle`、`build_scene_interchange`、`write_scene_package` |
| `mtk-py` | 38 | 31 | 所有 `Py*` 类的 Rust 根导出；仅 `#[pymodule]` 注册需要 |
| `mtk-wasm` | 1 | 1 | 整个 crate 无消费者 |

### 4.2 公开模块泄漏

核心 crate 共有 **78 个 `pub mod`**。例如 `mtk-core` 直接公开 `attributes`、`extrude`、`mesh`、`polygon`、`profile`、`subdivide`、`uv` 等内部模块；即使 crate 根没有 `pub use`，下游仍可通过 `mtk_core::extrude::internal_fn` 直接引用。

**建议**：

- 内部模块改为 `mod`；需要跨 crate 的 item 在 crate 根用 `pub use` 精确导出。
- 对外 `pub use` 只保留“领域入口类型 + 顶层算子”，其余 `pub(crate)`。
- 对 `mtk-core` / `mtk-voxel` / `mtk-material` / `mtk-model` 先做一轮“根导出瘦身”，再删除无消费者函数。

### 4.3 零消费者整 crate 与重复适配器

- **`bindings/mtk-wasm`**：无任何 Rust/Python/前端消费者，且其 API 仍保留旧式 `append_unit_cube_face` / `get_flat_*` 风格。按规则 1 应删除整个 crate，或至少在找到真实消费者前不纳入构建/文档/CI。
- **`adapters/blender/mtk-blender-dna`**：`copy_positions_to_ptr`、`copy_normals_to_ptr`、`copy_indices_to_ptr`、`copy_loop_uvs_to_ptr`、`copy_colors_to_ptr`、`copy_point_cloud_*_to_ptr` 全部是通用 `copy_*_to_ptr` 的零消费者别名，应删除；只保留通用 `copy_*` / `read_*` / `validate_*`。
- **`mtk-host-memory::slice_layout`**：仅内部/测试使用，降为 `pub(crate)`。

---

## 5. 文档漂移 (Documentation Drift)

已确认的过期内容：

- `bindings/mtk-py/README.md`：
  - 写 `api_version` 当前为 `1`，实际为 `2`；
  - 列出已删除的 `MeshData.with_capacity`、`get_flat_secondary_uvs`、`add_string_attribute_from_palette`、`get_attribute_data`；
  - 列出已删除的 `FaceCuller.is_face_occluded` / `cache_len`；
  - 列出已删除的 `VoxelStorage.dirty_section_count` / `clear_dirty_sections`；
  - 仍写 `sample_colormap_pixel` / `srgb_to_linear` / `linear_to_srgb` / `blend_biome_colors` 等未导出或已移除接口。
- `docs/API_REFERENCE.md` / `docs/BINDINGS_DESIGN.md`：
  - 仍引用 `process_random_extrude_mesh`、`cull_faces`、`remap_mesh_multi_uvs_parallel` 等已删除/未导出 API；
  - 仍描述与当前 `ScenePackageReader` / `VanillaColormaps` 不一致的入口。

**要求**：下一轮删除 API 的同一提交必须同步更新这些文档；文档里不允许出现“已废弃但保留”的说明。

---

## 6. 建议保留的生产契约（防止过度删除）

以下为 `../MoziToolKit` 生产路径实际消费的核心面，清理时应保留（可改签名，但不要误删）：

- **Mesh**：`MeshData.from_flat_polygons` / `from_polygon_offsets`、计数属性、`positions/normals/uvs/indices/quad_indices/loop_uvs/face_materials/colors_memoryview`、属性读写、`profile`、`used_materials`、`compact_materials`、`dynamic_entity_atlases/slots`。
- **Voxel**：`VoxelPointCloud.from_indexed_arrays` / `to_storage` / 调色板与索引 memoryview / `compute_sparse_state_hash`；`VoxelStorage.from_point_cloud` / `to_point_cloud` / `create_debug_world*` / `get_biome` / 边界字段；`VoxelWorld.from_storage` / `rebuild_all` / `rebuild_dirty` / `get_world_mesh` / `get_storage` / `hot_reload` / `set_config` / `set_skin_cache_dir` / `used_chunk_ids` / `last_profile`。
- **Sync**：`LiveSyncSession.start` / `start_with_cache` / `stop` / `poll_events` / `request_resync` / `get_storage` / `get_world_mesh` / 状态字段 / `get_dynamic_entity_atlases/slots`。
- **Material / Biome**：`BiomeResolver`、`MaterialResolver`、`GridAtlasSpec`、`VanillaColormaps`（作为不透明句柄）、`compute_biome_tint_attributes*`、`reapply_uniform_biome_tint`、`build_material_palette`、`get_biome_meta`、`get_all_biomes`、`sample_colormap_colors`。
- **Assets / Scene**：`ResourcePackStack`、`precompile_all_assets`、`AssetCache`（open/manifest/atlas/models/biome/colormaps/texture read）、`ScenePackageReader.open` / `metadata` / `unpack` / `read_texture_rgba/f32`、`export_scene_package`。
- **Save / GPU-free geometry**：`inspect_minecraft_save`、`load_and_mesh_minecraft_save`、`analyze_mesh_hard_edges`、`adaptive_pixel_split_mesh`、`process_flat_mesh_extrude_repair`、`generate_random_extrude_heights`、`repair_mesh_fluid_uv_inplace`、UV 几何四则。

---

## 7. 下一轮清理批次 (Proposed Cleanup Batches)

每个批次独立提交、独立回滚；先改调用方，再删旧 API，最后同步文档与测试。

### Batch 1：Python 稳定面 P0/P1 删除

- [x] 删除第 3.1 节全部符号。
- [x] 将第 3.2 节协议/工具/测试专用符号移入 `unstable.testing` 或删除：
  - `decode_packet`, `mesh_attribute_schema`, `retired_mesh_attributes` 移入 `unstable.testing`；
  - `clear_cached_colormaps` 删除；
  - `AssetCache.extract_atlas_texture` 移入 `unstable.testing.extract_cache_atlas_texture`；
  - `MeshData`: 删除 `append_mesh`, `has_attribute`, `loop_starts_memoryview`, `loop_totals_memoryview`, `source_texture_palette_indices`, `uv_layout`；保留 `to_buffers` 作为 Tier 3 契约；
  - `MesherConfig`: 删除 `has_atlas`, `has_biome_resolver`, `has_colormaps`, `set_bounds`；
  - `BakedModelDatabase`: 删除 `contains`；`get_states`, `remap_to_atlas` 移入 `unstable.testing`；
  - `VoxelPointCloud`: 删除 `from_mesh_data`, `to_mesh_data`；保留 `to_buffers` 作为 Tier 3 契约；保留 `push` 供测试夹具使用；
  - `VoxelStorage`: 删除 `clear`, `get_bounds`；`get_non_empty_sections`, `get/set_block_entity_json` 移入 `unstable.testing`；保留 `set_block`/`set_bounds` 供测试夹具装配；
  - `VoxelWorld`: 删除 `clear`, `get_bounds`, `set_bounds`, `get_block`, `set_block`, `get/set_block_entity_json`；`ensure_dynamic_entities_resolved_sync` 移入 `unstable.testing`；
- [x] `bridge/` 中对应 `hasattr` 分支同步删除；调用方直接调用唯一入口。
- [x] 更新 Python 测试：能用生产 API 的改用生产 API；只能测内部算子的放 `unstable.testing`。
- [x] 目标：稳定顶层不再存在“仅测试/工具使用”的方法。

### Batch 2：场景交换 API 收敛

- [x] `bridge/scene.py` 删除 `if hasattr(reader, "unpack")` 及逐项 `read_*` 回退，唯一路径走 `unpack`。
- [x] `ScenePackageReader` 收敛为 `open` + `metadata`（dict）+ `unpack` + `read_texture_rgba/f32`。
- [x] 删除 `ValidationReport` / `effective_capabilities` / `capabilities` 包装；需要校验时返回普通 dict。
- [x] `libmtk::scene` 根导出只保留 `export_scene_package` / `ScenePackageReader` / `SceneMetadata` / `ValidationReport`（若仍需要）/ 版本常量；内部 `SceneExportResult` / `SceneWriteRequest` / `ScenePackageWriteOptions` / `SceneBundle` 降为 `pub(crate)`。

### Batch 3：生物群系 / colormap API 收敛

- [x] 删除进程级 colormap 缓存 API（`set_cached_colormaps` / `clear_cached_colormaps` / `has_active_colormaps_override`）。
- [x] colormap 作为显式参数传入；`VanillaColormaps` 只作为不透明句柄，删除未消费的 `sample_*` / `load_embedded`。
- [ ] 合并 `compute_mesh_biome_attributes*` 变体：生产入口只保留 `compute_biome_tint_attributes_paletted` 与 `reapply_uniform_biome_tint`（名称可按最终设计调整，但只能一个）。
- [ ] 将 20 参数函数收敛为强类型 `BiomeTintRequest` 或明确分组参数。

### Batch 4：Rust 可见性瘦身（按 crate 逐个）

顺序：`mtk-core` → `mtk-voxel` → `mtk-material` → `mtk-model` → `mtk-sync` → `mtk-package` → `libmtk`。

- [x] 清理 `mtk-core` 废弃的嵌套挤出修复与随机挤出算子（`process_mesh_extrude_repair`, `process_random_extrude_mesh`, `ExtrudeMeshInput/Output`, `RandomExtrudeMeshInput/Output`）。
- [x] 收敛 `mtk-cull` 内部矩形差集切分算子可见性（`subtract_rect`, `is_empty_rect`, `is_full_rect`）。
- [x] 收敛 `mtk-model` 内部 OBJ 解析器可见性（`WavefrontObjParser`, `ModObjLoader`）。
- [x] 收敛 `mtk-sync` 内部协同客户端与暂存缓冲区可见性（`SyncClient`, `BaselineAssembler`, `DeltaStagingBuffer`）。
- [ ] 内部 `pub mod` 改 `mod`；crate 根只 `pub use` 有跨 crate 消费者的 item。
- [ ] 删除第 4.1 节中零外部消费者的根导出；仍被本 crate 其他模块使用的降为 `pub(crate)`。
- [ ] 删除零消费者函数/类型；单实现 trait 与包装类型一并删除。
- [ ] 每完成一个 crate 跑 `cargo test -p <crate>` 与 `cargo clippy -p <crate> --all-targets -- -D warnings`。

### Batch 5：绑定与适配器收敛

- [ ] `bindings/mtk-py` 根 `pub use` 改为 `pub(crate) use`；只保留 `#[pymodule]` 注册所需。
- [ ] 删除 `libmtk_blender_dna` 的 `read_*` 与 `as_memoryview_raw` 零消费者导出，并同步删除 Rust 侧失去消费者的 `read_*_from_ptr`。
- [ ] 删除 `adapters/blender/mtk-blender-dna` 中零消费者别名；只保留通用 `copy_*` / `read_*` / `validate_*`。
- [ ] `mtk-host-memory::slice_layout` 降为 `pub(crate)`。
- [ ] 决策 `mtk-wasm`：删除整个 crate（推荐）或在找到真实消费者前不纳入 workspace/CI/文档。

### Batch 6：文档与门禁

- [ ] 按第 5 节重写 `bindings/mtk-py/README.md`，以实际导出面为准。
- [ ] 更新 `docs/API_REFERENCE.md` / `docs/BINDINGS_DESIGN.md` / crate README。
- [ ] 增加 API 审计脚本或 CI 检查：稳定顶层零测试专用、Rust 根导出必须有外部消费者。
- [ ] 完成 `cargo fmt` / `clippy` / `test` / Python 测试全绿。

---

## 8. 验收标准

- Python 稳定顶层不存在仅测试/工具使用的方法；`unstable.testing` 只保留真实测试所需算子。
- 每个 Python 稳定符号都能在 `../MoziToolKit` 生产代码中指向具体调用点。
- 每个 Rust crate 根导出都能在工作区其他 crate 中指向具体调用点；内部模块默认私有。
- `mtk-wasm` 有明确消费者，或被删除。
- 文档中不再出现已删除符号；`bindings/mtk-py/README.md` 与实际内省结果一致。
- 全门禁通过：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build-all-py
PYTHONPATH=../MoziToolKit/dev/lib uv run --directory ../MoziToolKit pytest ../libmozitoolkit/bindings/mtk-py/tests/ -q
PYTHONPATH=../MoziToolKit/dev/lib uv run --directory ../MoziToolKit pytest ../libmozitoolkit/adapters/blender/mtk-blender-dna-py/tests/ -q
```

---

## 附录 A：核心 crate 零跨 crate 消费者根导出（完整快照）

> 说明：以下是 crate 根 `pub use` 名称中，在定义 crate 之外的非测试 Rust 代码里**零引用**的完整集合。它们不一定是“必须删除”的最终结论（部分可能是 CLI 有意契约），但每一个都必须在下轮给出保留理由，否则删除根导出或降为 `pub(crate)`。

### `mtk-core` (16)
`ExtrudeMeshInput`, `ExtrudeMeshOutput`, `LightLevel`, `MaterialSlotId`, `RandomExtrudeMeshInput`, `RandomExtrudeMeshOutput`, `TintIndex`, `Vec4`, `cellular_noise_3d`, `interpolate_bilinear_2d`, `interpolate_bilinear_3d`, `interpolate_bilinear_4d`, `is_mesh_uv_collapsed`, `mc_local_to_centered_z_up`, `process_mesh_extrude_repair`, `process_random_extrude_mesh`

### `mtk-cull` (15)
`CoplanarRelation`, `CullCategory`, `MeshCullResult`, `derive_parametric_face_shapes`, `extract_quad_face_occlusion_rect`, `fluid_group`, `is_empty_rect`, `is_face_completely_occluded`, `is_face_occluded_by_state`, `is_full_rect`, `is_inherently_waterlogged_name`, `is_non_full_or_partial_block`, `is_same_fluid`, `should_skip_rendering`, `subtract_rect`

### `mtk-model` (30)
`BakedElement`, `BakedFaceGeometry`, `BakedObjFace`, `BuiltinTransform`, `ElementJson`, `EntityCategory`, `FaceJson`, `ModObjLoader`, `ModelMeshOptions`, `MultipartCondition`, `MultipartRule`, `ObjRawFace`, `ResolvedElement`, `ResolvedFace`, `RotationJson`, `TextureValue`, `VariantEntry`, `VariantMatch`, `WavefrontObjParser`, `apply_uvlock_to_uvs`, `bake_face_exact`, `calculate_facing`, `default_face_uv`, `get_face_canonical_vertex`, `get_face_uvlock_transform`, `is_air_like`, `recalculate_winding`, `rotate_direction`, `rotate_element_point`, `rotate_point`

### `mtk-voxel` (31)
`BiomeMeta`, `DeltaMesher`, `EMPTY_SECTION_CRC`, `PaddedVoxelArray`, `PointCloudVoxelSource`, `SmoothedBiomeColumn`, `SparseVoxelColumns`, `VoxelReader`, `VoxelSource`, `WorldMeshBuildResult`, `ao_brightness_to_level`, `ao_level_to_brightness`, `calculate_corner_average`, `calculate_face_ao`, `calculate_face_ao_float`, `calculate_face_ao_with_inset`, `calculate_fluid_corner_heights`, `calculate_fluid_flow_vector`, `crc32`, `crc32_update`, `emit_fluid_geometry`, `extract_canonical_state_str`, `get_empty_section_crc`, `get_fluid_base_height`, `get_smoothed_biome_data`, `ingest_from_source`, `is_face_cubic`, `is_face_inset`, `sample_fluid_height`, `should_flip_quad_diagonal`, `vertex_ao`

### `mtk-sync` (14)
`BaselineAckResult`, `BaselineAssembler`, `CancelReason`, `ClientCommand`, `ClientMessage`, `DeltaStagingBuffer`, `ErrorCode`, `Opcode`, `PROTOCOL_MAGIC`, `PROTOCOL_VERSION`, `ProtocolError`, `ResyncReason`, `SyncClient`, `SyncLifecycle`

### `mtk-resource` (16)
`AnimationFrame`, `AtlasFilterPattern`, `BlockMatch`, `CTM_47_LOOKUP`, `ConnectLogic`, `CtmMethod`, `CtmSymmetry`, `DEFAULT_NAMESPACE`, `MemoryPack`, `OVERLAY_17_LOOKUP`, `PbrCompanions`, `TextureMetadata`, `UnstitchRegion`, `coordinate_random`, `get_face_tangents`, `is_companion_asset_path`

### `mtk-texture` (18)
`DYNAMIC_CHUNK_END`, `DynamicAtlasChunk`, `DynamicSlotInfo`, `MAX_DYNAMIC_CHUNKS`, `SpriteKind`, `StitchedAtlas`, `StitchedChunk`, `StitchedSlot`, `Stitcher`, `StitcherHolder`, `StitcherRegion`, `apply_edge_clamping_padding`, `bake_paletted_permutation`, `batch_analyze_transparent_faces_u8`, `extract_palette_colors`, `is_face_transparent_u8`, `sample_alpha_u8`, `smallest_encompassing_power_of_two`

### `mtk-material` (40)
`BLOCK_TINT_REGISTRY`, `BiomeTintFlatBuffers`, `DecodedPbr`, `HARDCODED_BLOCK_TINTS`, `MeshAnimationRerouteResult`, `MeshBiomeAttributesResult`, `MeshMaterialRoute`, `MeshMultiUvRemapResult`, `MeshPalettedUvRemapResult`, `PbrDecodeSummary`, `TintInfo`, `apply_dark_forest_grass_modifier`, `blend_biome_colors`, `classify_tint_category`, `compute_mesh_biome_attributes`, `compute_mesh_biome_attributes_custom`, `compute_resolved_tint_color`, `decode_grid_atlas_uv`, `decode_ior`, `decode_labpbr13`, `get_hardcoded_tint`, `get_hardcoded_tint_hex`, `get_unit_cube_tint_index`, `hex_to_srgb`, `is_quad_uv_diamond`, `linear_to_srgb`, `remap_atlas_to_local`, `remap_grid_atlas_uv_to_local`, `remap_local_to_atlas`, `remap_mesh_multi_uvs_parallel_with_options`, `remap_sprite_to_sprite`, `resolve_biome_color_set`, `resolve_routed_sprite`, `sample_colormap_hex`, `sample_colormap_pixel`, `sample_colormap_pixel_u8`, `sample_colormap_pixel_uv`, `sample_colormap_pixel_uv_u8`, `srgb_to_linear`, `straighten_diamond_quad_uv`

### `mtk-save` (3)
`ChunkParser`, `RegionFile`, `decode_block_entity`

### `mtk-package` (8)
`CURRENT_VERSION_MINOR`, `ChecksumType`, `ChunkEntry`, `CompressionType`, `HeaderFlags`, `MAGIC_MTKP`, `MtkPackageWriter`, `TableOfContents`

### `libmtk` (14)
`AtlasLayerSource`, `BiomeAssetSource`, `CtmAssetSource`, `EntityAssetSource`, `SCENE_PACKAGE_FORMAT_VERSION`, `SceneAtlasSource`, `SceneBundle`, `SceneExportResult`, `ScenePackageWriteOptions`, `SceneSourceCapabilities`, `SceneWriteRequest`, `build_scene_interchange`, `prebake_all_models_with_progress`, `write_scene_package`

### 绑定 / 适配器附注

- `bindings/mtk-py` 的 31 个零 Rust 外部引用名称全部是 `Py*` 类与 Python 顶层函数。它们对 Python 有消费者，但对 Rust 其他 crate 无消费者；应改为 `pub(crate) use` / `pub(crate)`，不影响 Python 注册。
- `bindings/mtk-wasm` 整个 crate 无消费者，见 4.3。
- `adapters/blender/mtk-blender-dna` 的 `DnaResult` 无外部引用；`copy_*_to_ptr` 别名与 `read_*_from_ptr` 的进一步收敛见 4.3 与 Batch 5。
