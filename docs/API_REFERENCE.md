# libmozitoolkit 核心抽象与 API 导航手册 (API Reference)

`libmozitoolkit` (`libmtk`) 采用**原子化、高内聚、分层治理**的模块架构。各子模块的详细 API 签名、内部数据模型与专属代码示例已由各 Crate 专属 `README.md` 作为第一事实源维护。

本文档定义 **全局跨模块通用核心契约 (System-Wide Contracts)**，并提供全工作区模块的快速 API 导航矩阵。

---

## 1. 模块 API 导航矩阵 (Crates & Bindings Map)

| 层次 (Layer) | Crate / Binding | 核心定位与职责 | API 与技术手册 |
| :--- | :--- | :--- | :--- |
| **Layer 0: 基础几何** | **[`mtk-core`](../crates/mtk-core/README.md)** | 标准连续几何缓冲 (`MeshData`)、6向拓扑与标准化右手坐标转换、面切线基底推导 (`derive_quad_face_tangents`)、硬边分析 (`analyze_hard_edges`)、自适应像素网格切分 (`subdivide`)、智能挤出与 UV 修复 (`extrude`) | [查看 API 手册 ↗](../crates/mtk-core/README.md) |
| **Layer 1: 遮挡与清洗** | **[`mtk-cull`](../crates/mtk-cull/README.md)** | 1.21+ 邻域遮挡状态机 (`FaceCuller`)、高度感知流体面剔除 (`should_render_fluid_face`)、2D 矩形差集切分 (Subtract Rect)、外部网格叠面剔除 (`cull_mesh_faces`) | [查看 API 手册 ↗](../crates/mtk-cull/README.md) |
| **Layer 1: 模型与状态** | **[`mtk-model`](../crates/mtk-model/README.md)** | BlockState 状态机解析 (multipart/variants)、1.21+ Block Model JSON 递归继承展开与几何烘焙、`BakedModelDatabase` 多级回退、OBJ 加载 | [查看 API 手册 ↗](../crates/mtk-model/README.md) |
| **Layer 1: 资源包 VFS** | **[`mtk-resource`](../crates/mtk-resource/README.md)** | 虚拟文件系统 (Zip/Jar/Dir)、PBR 多层资源栈合并、`.png.mcmeta` 动图元数据、`atlases/*.json` 规范与自适应切线 CTM 47/17 连接纹理求解器 (`resolve_face_with_tangents`) | [查看 API 手册 ↗](../crates/mtk-resource/README.md) |
| **Layer 1: 贴图与图集** | **[`mtk-texture`](../crates/mtk-texture/README.md)** | 2D 矩形二叉装箱 Stitcher、多通道 PBR 图集烘焙、盔甲调色板置换与抗渗色填充 | [查看 API 手册 ↗](../crates/mtk-texture/README.md) |
| **Layer 1: 材质与生物群系** | **[`mtk-material`](../crates/mtk-material/README.md)** | 66 种原版生物群系调色板与色彩数学权威唯一事实源 (SSOT)、`BiomeResolver` 染色模型扫描器、Rayon 并行多 UV 重映射与别名解析 | [查看 API 手册 ↗](../crates/mtk-material/README.md) |
| **Layer 1: 体素世界与网格化** | **[`mtk-voxel`](../crates/mtk-voxel/README.md)** | 16×16×16 区块体素存储 (`SectionStorage` / `VoxelStorage`)、全量无剔除体素点云 (`VoxelPointCloud`)、稀疏列存体素流 (`SparseVoxelPayload` / `SparseVoxelColumns`)、平滑 AO 算法、物理流体曲面与原地拓扑批量修复 (`repair_mesh_fluid_uv_inplace`)、贪婪网格化、增量局部重构 (`DeltaMesher`)、统一 3D 场景引擎 (`VoxelWorld`) 与统一体素源抽象 (`VoxelSource`) | [查看 API 手册 ↗](../crates/mtk-voxel/README.md) |
| **Layer 1: 实时网络协同** | **[`mtk-sync`](../crates/mtk-sync/README.md)** | 原生多线程 WebSocket 协同客户端 (`SyncClient`)、小端序二进制协议编解码、两阶段流式增量更新与单一世界大网格 (`WorldMeshReady`) 会话管理、0ms 缓存状态恢复 (`compute_sparse_state_hash`) | [查看 API 手册 ↗](../crates/mtk-sync/README.md) |
| **Layer 1: 存档与空间切片** | **[`mtk-save`](../crates/mtk-save/README.md)** | 现代 Minecraft 存档加载核心：基于内置零拷贝 NBT、Anvil `.mca` 区域寻址、384 高度解包与按需空间切片流式数据源 (`VoxelSource`) | [查看 API 手册 ↗](../crates/mtk-save/README.md) |
| **Layer 1: 二进制包容器** | **[`mtk-package`](../crates/mtk-package/README.md)** | 统一二进制包与场景交换容器：64 字节头部 CRC 校验、分块 Zstd 压缩 TOC、`memmap2` 零拷贝解析与逐块惰性解压（支持 `AssetCache` 与 `SceneInterchange` 双 Profile） | [查看 API 手册 ↗](../crates/mtk-package/README.md) |
| **Layer 2: 应用编排层** | **[`libmtk`](../crates/libmtk/README.md)** | 提供端到端全量资产预编译管线 (`precompile_all_assets`) 与自包含场景交换容器编解码器 (`libmtk::scene`: `ScenePackageReader`, `export_scene_package`)，不重导出领域类型 | [查看 API 手册 ↗](../crates/libmtk/README.md) |
| **Layer 3: Python 绑定** | **[`mtk-py`](../bindings/mtk-py/README.md)** | **P0 核心绑定**：PyO3 驱动的 `libmtk_py` 模块，为 MoziToolKit (Blender 4.2+ 插件) 提供零拷贝 Buffer Protocol / NumPy 视图、极速批处理算子与 `.mtkscene` 场景包编解码支持 | [查看 API 手册 ↗](../bindings/mtk-py/README.md) |
| **Layer 3: 独立命令行** | **[`mtk-cli`](../crates/mtk-cli/README.md)** | **P1 核心工具**：独立无头命令行终端工具 (`mtk`)，支持图集烘焙、模型导出、CTM 求解与遮挡校验 | [查看 API 手册 ↗](../crates/mtk-cli/README.md) |
| **Layer 3: 性能基准测试** | **[`mtk-bench`](../crates/mtk-bench/README.md)** | **P1 压测套件**：4000+ 区块段多核网格化与数百万次遮挡剔除压力基准套件 | [查看 API 手册 ↗](../crates/mtk-bench/README.md) |
| **Layer 3: WebAssembly** | **[`mtk-wasm`](../bindings/mtk-wasm/README.md)** | **P2 远期预备**：wasm-bindgen WebAssembly 绑定，暴露线性内存 TypedArray 视图，面向 WebGPU / Three.js 网页预览 | [查看 API 手册 ↗](../bindings/mtk-wasm/README.md) |
| **Layer 3: 宿主内存适配** | **[`mtk-host-memory`](../adapters/mtk-host-memory/README.md)** / **[`mtk-blender-dna`](../adapters/blender/mtk-blender-dna/README.md)** | **P0 DNA 适配**：通用 raw slice → pointer 复制 + Blender 语义包装；独立 `libmtk_blender_dna` wheel，提供网格拓扑与点云属性的双向零拷贝直写与直读适配 | [查看 API 手册 ↗](../adapters/blender/mtk-blender-dna/README.md) |

---

## 2. 全局通用核心契约 (System-Wide Contracts)

### 2.1 标准化 3D 几何坐标系规范 (Coordinate Systems)
为了杜绝宿主耦合与坐标系混淆，底层内核统一遵循以下坐标变换规范：
- **Minecraft 原始坐标系**：
  - 局部方块坐标：$[0.0, 16.0]$（+X East, +Y Up, +Z South）。
  - 世界体素坐标：整数三元组 $(X, Y, Z)$。
- **标准化右手 Z-Up 坐标系 (Standard Right-Handed Z-Up)**：
  - $+X$ 轴向东 (East), $+Y$ 轴向北 (North), $+Z$ 轴向上 (Up)。
  - `mc_world_to_z_up(wx, wy, wz) -> Vec3`：将 Minecraft 世界坐标转换为标准右手 Z-Up 坐标。
- **标准化右手 Y-Up 坐标系 (Standard Right-Handed Y-Up)**：
  - 适用于 Godot / glTF / Three.js / WebGPU 默认管线（+Z 前向，视线朝 $-Z$）；
  - Minecraft 世界空间本身已满足该规范（+X East, +Y Up, +Z South），故 `CoordinateSystem::YUpRightHanded` 为**恒等映射** `(X, Y, Z) -> (X, Y, Z)`，用于在调用点显式声明目标约定；
  - 注意 Unity 使用**左手** Y-Up 系，需在宿主端额外镜像 X/Z，不可直接套用本枚举。
- **2D 贴图物理切线基底规范 (Texture Tangent Basis)**：
  - 统一由 `Direction::canonical_face_tangents(self) -> (Direction, Direction)` 作为权威事实源（SSOT）。
  - 支持利用 `derive_quad_face_tangents` 从四边形 3D 顶点与 2D UV 偏导数中动态解析物理切线，自动感知旋转模型与 UV 旋转。


---

### 2.2 扁平连续内存几何契约 (Flat Buffer Protocol)

为了确保跨 Crate 调用与跨语言（Python / WASM / C-ABI）极速数据传输，所有几何数据均标准化为扁平连续 1D 缓冲区：

#### ① `MeshData` (基础三角网格与 Quad 顶角容器)
```rust
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,           // 连续 3D 顶点位置流 [N, 3]
    pub normals: Vec<[f32; 3]>,             // 连续 3D 法线流 [N, 3]
    pub indices: Vec<u32>,                  // 三角形顶角索引 [M] (M = 3 * Triangles)
    pub quad_indices: Option<Vec<u32>>,     // 四边形顶角索引 [Q * 4] (保留 Quad 拓扑)
    pub uvs: Vec<[f32; 2]>,                 // 主 UV 坐标流，语义由 uv_layout 决定
    pub uv_layout: UvLayout,                // None / PerVertex / PerLoop
    pub secondary_uvs: Option<Vec<[f32; 2]>>, // 备用/局部 UV 坐标流 [N, 2]
    pub colors: Option<Vec<[f32; 4]>>,      // 顶点颜色 / Linear RGBA [N, 4]
    pub face_materials: Vec<MaterialSlotId>,// 逐面材质插槽映射 [F]
    pub face_tint_indices: Vec<TintIndex>,  // 逐面染色索引 [F]
    pub custom_attributes: HashMap<String, MeshAttribute>, // 按名称索引的泛型自定义属性 (Point/Corner/Face/Mesh 域)
}
```

#### ② `FlatPolygonMesh` (多边形循环拓扑连续缓冲，对接 Blender Polygon Loops)
```rust
pub struct FlatPolygonMesh {
    pub positions: Vec<[f32; 3]>,           // [N, 3] 顶点坐标
    pub loop_vertices: Vec<u32>,            // [L] Blender Mesh Loop 顶点索引
    pub loop_uvs: Vec<[f32; 2]>,            // [L, 2] 逐角隅 Loop 域 UV 坐标
    pub face_loop_starts: Vec<u32>,         // [F] 逐面 Loop 起始偏移
    pub face_loop_totals: Vec<u32>,         // [F] 逐面顶点总数 (Quad 为 4)
    pub face_materials: Vec<u32>,           // [F] 逐面材质插槽 ID
}
```

---

### 2.3 生物群系调色板与色彩数学单一事实源 (Biome SSOT)
- **唯一权威来源**：`mtk-material::biome`。
- **66 种原版官方生物群系规范表**：包含所有 Overworld, Nether, The End 群系的规范温度、湿度、草方块颜色、树叶颜色与水体颜色。
- **硬编码方块染色与红石信号强度 (Hardcoded Tints & Redstone Wire)**：
  - 云杉树叶、白桦树叶、睡莲与红石引线均为硬编码染色类别 (`TINT_TYPE_HARDCODED = 4`)；
  - 红石引线 (`redstone_wire`) 统一维护 0~15 级标准 sRGB Hex 表与 Linear RGBA 线性渐变色彩，发光强度按 `power / 15.0` 计算；
  - 烘焙网格注入 `mtk_biome_tint_data` (`[1.0, 1.0, 1.0, 4.0]`) 等规范属性；属性契约见 `mtk_core::MESH_ATTRIBUTE_SPECS`。
- **色彩空间规范**：
  - 宿主与渲染器内部色彩混合**必须在标准线性空间（Linear RGBA, $[0.0, 1.0]$）中进行**；
  - 导出/展示贴图时使用权威转换公式双向映射：
    $$\text{Linear} = \begin{cases} \frac{\text{sRGB}}{12.92} & \text{sRGB} \le 0.04045 \\ \left(\frac{\text{sRGB} + 0.055}{1.055}\right)^{2.4} & \text{sRGB} > 0.04045 \end{cases}$$

---

### 2.4 网格属性契约单一事实源 (Mesh Attribute SSOT)
- **唯一权威来源**：`mtk_core::MESH_ATTRIBUTE_SPECS`（名称、域、类型、`material` / `metadata` 角色）与 `RETIRED_MESH_ATTRIBUTES`（退役清单）。
- **宿主只消费、不复制**：Python / WASM 绑定通过 `mesh_attribute_schema()` 与 `retired_mesh_attributes()` 读取该契约；Blender 前端由 `tools/sync_attribute_schema.py` 生成镜像并在测试中做逐项校验。
- **当前活跃属性**：
  - 材质节点消费：`mtk_uv_transform`、`mtk_uv_rotation`、`mtk_anim_timing`、`mtk_anim_frame_size`、`mtk_biome_tint_data`、`mtk_biome_tint_color`；
  - 宿主元信息：`mtk_source_textures`、`mtk_source_texture_idx`、`mtk_atlas_chunk_id`、`mtk_block_x/y/z`。
- **已退役属性**（`mtk_source_texture_key`、`mtk_material_slot`、`mtk_atlas_texture_id`、`mtk_uv_mode`、`mtk_colormap_uv`、`mtk_face_dir`、`mtk_emission`、标量动画拆分、旧 tint 权重等）不再由核心生成；宿主只负责从旧 `.blend` 中清理。
- **`RETIRED_MESH_ATTRIBUTES` 是硬门禁**：`mtk-voxel` 的回归测试断言 mesher 输出不包含该清单中的任何成员。

---

### 2.5 统一错误处理系统 (`MtkError`)
`libmtk` 应用层将各领域错误聚合为单一 `MtkError`，并实现 `From<...>` 自动向上传播；绑定层不直接消费它，而是在边界处映射为宿主原生异常：
```rust
pub enum MtkError {
    Model(mtk_model::ModelError),
    Voxel(mtk_voxel::VoxelError),
    Texture(mtk_texture::error::TextureError),
    Resource(mtk_resource::error::ResourceError),
    Material(mtk_material::MaterialError),
    Save(mtk_save::SaveError),          // feature = "save"
    Package(mtk_package::PackageError),
    Io(std::io::Error),
    Json(serde_json::Error),
    Postcard(postcard::Error),
    ThreadPool(String),
}
```

---

### 2.6 资产预编译缓存格式规范 (`ASSET_CACHE_FORMAT_VERSION`)
由 `libmtk::prebake` 产出的资产预编译缓存目录结构受全局版本号严格约束（当前版本：`ASSET_CACHE_FORMAT_VERSION = 6`）：
```
cache_output_dir/
├── cache_manifest.json     # 全局清单、时间戳、CRC 校验与版本号
├── atlas_mapping.json      # 全量材质名 -> 图集坐标/UV 映射表
├── biome_mapping.json      # 全量方块模型染色规则与 Tint 元数据
├── atlases/                # 烘焙完成的 PNG 图集文件
│   ├── blocks_0.png        # 基础 Albedo 图集（sRGB）
│   ├── blocks_0_n.png      # OpenGL 法线 RGB + 高度 A（Non-Color）
│   ├── blocks_0_orm.png    # AO / Roughness / Metallic / Emission（Non-Color）
│   └── blocks_0_aux.png    # Encoded IOR / Porosity / SSS / Transmission（Non-Color）
└── models/                 # 烘焙完成的预解析二进制方块几何库
```
- **v6**：移除 `BakedModel.emissive_level` 及其模型缓存字段；旧 v5 缓存会被 manifest 版本校验拒绝并重建。
- **v5**：`models.bin` 由 Bincode 切换为 Postcard + Zstd，烘焙体积更小；旧 v4（Bincode）缓存会被 manifest 版本校验直接拒绝并重建。
- **外部兼容配置**：`PrecompileConfig.resolver_overrides` 可携带 Python 维护的 `ResolverOverrides`（导出器后缀、材质回退、tint/hardcoded 覆盖）；预编译期合并进 `BiomeResolver` 并写入 `resolver/overrides`（`COMP` chunk），缓存打开时安装为 `MaterialResolver` 规则。
- **资源包 Colormap 覆盖**：预编译期仍会把资源包的 `assets/minecraft/textures/colormap/{grass,foliage,dry_foliage}.png` 写入 `BIOM` chunk；`AssetCacheReader::open` 会读取并安装为当前采样色盘，缺失通道回退内置原版。Python 宿主不再解包或传递 colormap 文件。

---

### 2.7 1:1 Minecraft 原版视觉对齐契约 (Alternate Blocks & Plant Offsets)
为了在无多余网络开销的前提下完美还原原版世界视觉效果，系统采用客户端纯算术空间坐标哈希管线：
- **核心空间哈希与随机数生成器** (`mtk_core::random`):
  - `mc_coordinate_seed(x, y, z) -> i64`: 1:1 对齐 Java 原版 `Mth.getSeed(x, y, z)`。
  - `JavaRandom`: 48 位线性同余伪随机数生成器 (`java.util.Random` / `SingleThreadedRandomSource`)。
- **预烘焙交替模型组** (`mtk_model::BakedVariantGroup`):
  - 针对自然方块（泥土、石头、沙子、草方块、睡莲等）的多旋转分支预烘焙为离散模型，支持按空间坐标种子快速抽样 (`select_by_pos`)。
- **植物空间确定性抖动** (`mtk_core::OffsetType`):
  - 对花草、蕨类 (`OffsetType::XZ`) 及竹子 (`OffsetType::XYZ`) 依 `Mth.getSeed(x, 0, z)` 沿水平轴进行 $\pm 0.25$ 偏移，两格高植物上下层茎秆水平位移严格保持一致。
- **网格化源抽象与配置** (`mtk_voxel`):
  - `ModelSource`: 统一表征单模型 (`Single`)、预烘焙变体组 (`Variant`) 或纯方块回退 (`None`)。
  - `MesherConfig.enable_alternate_blocks` (默认 `true`): 控制是否启用交替模型采样。
  - `MesherConfig.enable_random_offsets` (默认 `true`): 控制是否启用植物坐标抖动。

---

### 2.8 现代 Minecraft 存档加载契约 (`mtk-save` & Spatial Slicing)
为支持现代 Minecraft 1.18+ / 1.20+ / 1.21+ (`DataVersion` >= 2844) 大规模存档按需读取与极速渲染，系统建立了流式空间切片与零拷贝解包契约：
- **`LevelData`**: 从 `level.dat` 零拷贝读取世界元数据（`level_name`、`version_name`、`data_version`、`spawn` 三维坐标、`time`、`day_time`、`hardcore` 等）。
- **`RegionFile`**: 针对 32x32 区块 Anvil `.mca` 文件直接寻址 4096 字节 Location Table，按扇区偏移直接 Seek 并仅解压目标区块。
- **`ChunkParser`**: 解密现代 Section 结构，对紧凑长整型数组（`bits_per_block >= 4`）进行纯位解包并将 Palette 状态规范化为唯一排序属性字符串（如 `minecraft:oak_stairs[facing=north,half=bottom]`）；同时解包 `biomes` 调色板与 4x4x4 quart 3D 生物群系，产出 `ParsedChunk`。
- **按需切片加载器 (`SaveLoader`)**:
  - `load_box_into_storage(save_dir, dimension, min_block, max_block, &mut storage)`: 依据用户 3D 边界盒直接定位所需相交的 Region / Chunk / Section，仅加载并填充所选区域进入 `VoxelStorage`（并自动填充 `storage.entities` 视觉实体），彻底杜绝数十 GB 全图扫描；解析完整 4×4×4 Section 群系后统一调用 `VoxelStorage::finalize_biome_metadata()` 派生地表列群系 `biome_column_map` 与主导 `primary_biome`（与实时同步、点云重建共用同一实现）。
  - `load_box_with_entities(save_dir, dimension, min_block, max_block, &mut storage, progress)`: 加载选区切片体素并同时提取解耦的连续视觉实体列表 `(sections, entities)`，亦填充 `storage.entities`。

---

### 2.9 真实物理进度上报与节流契约 (Physical Progress Reporting & Throttling)
为彻底杜绝长耗时计算中的黑盒假死与估算假进度，底层内核在 `mtk-core` 建立了通用的真实物理进度上报契约：
- **`ProgressReport`**: 包含阶段标识符 `stage`（如 `"meshing_sections"`、`"assembling_world_mesh"`）、真实物理已完成计数 `current`、总任务数 `total`、描述文本 `message` 与百分比计算 `percent()`。
- **`ProgressCallback`**: 跨线程回调契约 `&dyn Fn(ProgressReport) + Send + Sync`。
- **`ProgressThrottler`**: 面向多线程（Rayon）高频任务的原子无锁节流器，通过 `inc()` / `inc_by()` 配合步进控制（默认 1% 或每完成 $N$ 项触发一次），杜绝跨语言锁争抢并 100% 确保终态触发。
- **全链路模块接入 (`mtk-voxel`, `mtk-sync`, `mtk-save`, `libmtk::prebake`)**:
  - `VoxelWorld::rebuild_all_with_progress(callback)`: 在 Rayon 并行重构体素网格时，实时将各 Chunk Section 的真实烘焙进度向外派发。
  - `mtk-sync`: 实时将网格烘焙进度转化为 `SyncEvent::StreamProgress` 并向前端推送，配合 Blender 宿主状态栏 API（`wm.progress_*` 与 `workspace.status_text_set`）实现全链路底栏真实进度条。
  - `SaveLoader::load_box_into_storage_with_progress(...)`: 在遍历 Anvil MCA 区域与解压 Chunk NBT 期间实时汇报 `load_chunks` 物理进度，使世界存档导入在 MCA 解压与网格构建全流程透明可见。
  - `libmtk::precompile_all_assets_with_progress(...)` / `prebake_all_models_with_progress(...)`: 在端到端材质与资产预编译流程中，按阶段分发真实物理计数（`prebake_biome`、`prebake_atlas`、`prebake_models`、`prebake_manifest`），使图集拼接、独立材质对齐与模型变体烘焙全流程实时反馈在 DCC 宿主底栏。

---

### 2.10 通用二进制包与场景交换契约 (`MTKP` / Package Specification)
为了根除散文件磁盘 I/O 碎片，系统定义了统一的资产缓存与场景交换二进制包契约：
- **`Header & TOC` 架构**：固定 64 字节文件头、64 字节对齐分块、Zstd 块级压缩索引表，支持 `mmap` 零内存开销快速寻址。
- **两大标准 Profile**：
  - `AssetCache Profile` (`.mtkcache`)：存储全量预烘焙图集、全局方块模型数据库与群系映射。
  - `SceneInterchange Profile` (`.mtkscene`)：轻量离线场景交换包，内含稀疏体素列存 (`VOXL`)、Tree-Shaken 模型库 (`MODL`) 与动态裁剪拼接的微型多通道 PBR 图集 (`ATLS`)。
- 完整规范定义参见：[**`docs/PACKAGE_SPEC.md`**](./PACKAGE_SPEC.md)。

---

### 2.11 场景交换编解码器与 Mini-Atlas 导出契约 (`libmtk::scene`)
为支持跨机器、跨 DCC 宿主的无依赖便携场景分发，`libmtk::scene` 提供了端到端无损场景交换算子：
- **`export_scene_package(output_path, point_cloud, asset_cache, max_atlas_size, mip_level) -> SceneMetadata`**：
  - 提取 `VoxelPointCloud` 活跃方块状态与实体；
  - 自动剪枝并收集场景实际依赖的方块模型库 (`BakedModelDatabase`)；
  - 内存裁剪对应贴图并调用 `mtk_texture::Stitcher` 自动装箱为自包含 Scene Mini-Atlas；
  - 并行重映射模型 UV 坐标并以 Zstd 压缩打包为单一 `.mtkscene` 文件。
- **`ScenePackageReader`**：
  - `open(path)`：快速校验头部与 CRC，加载目录表；
  - `metadata`：读取场景包 AABB 边界、体素总数与图集分辨率等元数据；
  - `unpack(origin_centered=True, z_up=True) -> SceneUnpackResult`：单次调用原生解码并交付全部核心组件（`point_cloud`, `models`, `atlas`, `biome_resolver`, `colormaps`, `dynamic_entity_slots`, `ctm_solver`）；
  - `validate() -> SceneValidationReport`：执行场景包完整性与引用完整性校验。

---

### 2.12 性能探针与耗时审计契约 (Execution Profiling Contracts)
为确保全流水线端到端耗时测量精确、可解释、无暗箱盲区，`mtk-core` 建立了通用的性能探针基础设施与形式化时间线契约：
- **`StageSpan` 规范模型**：
  - `name`: 阶段名称（如 `"prep"`, `"extract_padded"`, `"parallel_meshing"`, `"assemble"`）；
  - `category`: 领域类别命名空间（`"mesh"`, `"io"`, `"bake"`, `"cull"`, `"subdivide"`, `"host"`）；
  - `depth`: 层级深度（`0` = 顶层阶段，`1` = 子阶段，`2` = 微步骤，由 Profiler 活跃栈自动推导）；
  - `start_offset_us`: 相对 Profiler 启动时刻的微秒绝对偏移；
  - `duration_us`: 该阶段壁钟耗时；
  - `items_count`: 处理图元/区块/实体数量（可选）。
- **时间线不变量 (Timeline Invariants)**：
  1. **顶层阶段顺序不重叠**：$\forall s_i, s_{i+1} \in \text{stages}$ (where $\text{depth}=0$), $s_i.\text{start\_offset\_us} + s_i.\text{duration\_us} \le s_{i+1}.\text{start\_offset\_us}$；
  2. **绝对时间线闭环**：$\forall s \in \text{stages}, \; s.\text{start\_offset\_us} + s.\text{duration\_us} \le \text{total\_duration\_us}$；
  3. **显式盲区审计**：`accounted_duration_us` 仅统计 $\text{depth} == 0$ 的顶层阶段和；未被埋点的系统盲区显式呈现为 `unaccounted_duration_us = total_duration_us - accounted_duration_us`，由 `debug_assert!` 守护 $\text{accounted} \le \text{total}$。
- **调用契约与子 Profile 规范挂载 (`ExecutionProfiler`)**：
  - 自动推导深度：通过 `profiler.measure(name, category, |p| { ... })` 维护活动栈，禁止手动指定 `depth`，彻底物理删除松散的 `record_stage` 方法；
  - `attach_sub_profile(sub_profile, category_override)` 四点语义：
    1. `category_override` 仅覆盖子 Profile 中 $\text{depth} == 0$ 的顶层阶段，保留其原有子阶段的细分类别；
    2. 绝不修改父级自身的 `total_duration_us`；
    3. 强制在父级活跃的 `measure` 作用域内调用（否则 panic）；
    4. 严格按照父阶段的绝对偏移平移时间线：`child.start_offset_us = parent.start_offset_us + child.start_offset_us`，且 `child.depth = parent.depth + 1 + child.depth`。

---

### 2.13 视觉实体渲染计划契约 (Visual Entity Render Plan Contract)
为解决画作、展示框、盔甲架等视觉实体的全链路 SSOT 治理与极速渲染装配，系统采用单一规范的渲染计划架构：
- **核心原则与职责划分**：
  - 核心（Rust）全权负责实体的规范命名、唯一稳定键生成（`uuid:...` 或 `synth:...`）、局部几何烘焙、UV/材质槽位路由、以及空间变换求解（坐标系变换、归零平移与欧拉角矩阵共轭计算）；
  - 宿主（DCC）作为纯声明式场景协调器（Reconciler），根据 `EntityRenderPlan` 返回的不可变节点列表执行 $O(K)$ 状态比对与就地装配，直接赋值 `obj.location`、`obj.rotation_euler` 与 `obj.scale`，彻底避免跨语言频繁交互与前端繁重矩阵解算。
- **核心数据结构**：
  - `EntityRenderTransform`：不可变空间变换 DTO，提供 `location: (f32, f32, f32)`、`rotation_euler: (f32, f32, f32)`、`scale: (f32, f32, f32)` 以及 `matrix: List[float]`（4x4 列主序变换矩阵）。
  - `EntityRenderNode`：单个实体渲染节点，提供 `key: str`、`display_name: str`、`transform: EntityRenderTransform`、`mesh: MeshData` 以及 `material_routes: List[dict]`（包含 `texture_key` 与 `material_slot`）。
  - `EntityRenderPlan`：完整渲染计划，包含 `nodes: List[EntityRenderNode]`、`node_count: int`、`deleted_keys: List[str]`、`warnings: List[dict]`、`revision: int` 与 `authoritative: bool`。
- **规范入口**：
  - `libmtk_py.build_entity_render_plan(entities=None, atlas_mapping_json=None, bounds=None, origin_centered=True, coordinate_system="z_up", revision=0, authoritative=True, deleted_keys=None)`：顶层通用构建算子，支持全量对账与局部增量重构。
  - `libmtk_py.host_transform_to_mc(location, rotation_euler, scale, bounds=None, origin_centered=True, coordinate_system="z_up")`：反向逆变换算子，将宿主刚体变换还原为规范 Minecraft 空间参数。






