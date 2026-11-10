# mtk-voxel

[![Crate](https://img.shields.io/badge/crate-mtk--voxel-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Rayon](https://img.shields.io/badge/Rayon-Parallel-red.svg)](https://github.com/rayon-rs/rayon)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-voxel`** 是 `libmozitoolkit` (`libmtk`) 套件中的核心体素（Voxel）几何引擎与空间存储 Crate。遵循 **无宿主依赖 (Host-Agnostic)**、**纯数据输入输出 (Data-In, Data-Out)** 与 **多源解耦** 架构原则，提供高吞吐 16×16×16 Chunk Section 紧凑体素存储、3D 稀疏世界容器、物理流体网格重建、平滑环境光遮蔽 (Smooth AO)、多源体素抽象以及多线程高性能网格化器 (Mesher)。

---

## 目录 (Table of Contents)

- [1. 架构定位与核心职责](#1-架构定位与核心职责)
- [2. 核心模块与系统拓扑](#2-核心模块与系统拓扑)
- [3. 核心数据结构与抽象 Trait](#3-核心数据结构与抽象-trait)
  - [3.1 紧凑体素存储 (`storage`)](#31-紧凑体素存储-storage)
  - [3.2 统一体素源抽象 (`source`)](#32-统一体素源抽象-source)
  - [3.3 网格化配置与统计 (`types`)](#33-网格化配置与统计-types)
- [4. 核心算法与功能实现](#4-核心算法与功能实现)
  - [4.1 18×18×18 邻域采样与零分支遮挡剔除](#41-181818-邻域采样与零分支遮挡剔除)
  - [4.2 4 顶点平滑 AO 与各向异性对角线折痕消除](#42-4-顶点平滑-ao-与各向异性对角线折痕消除)
  - [4.3 物理流体表面与流向 UV 重建](#43-物理流体表面与流向-uv-重建)
  - [4.4 5×5 (R=2) 生物群系平滑过渡与边界防渗色](#44-55-r2-生物群系平滑过渡与边界防渗色)
  - [4.5 增量网格重构 (`DeltaMesher`)](#45-增量网格重构-deltamesher)
- [5. 快速上手示例 (Quick Start)](#5-快速上手示例-quick-start)
  - [示例 1：构建体素世界并生成标准化网格](#示例-1构建体素世界并生成标准化网格)
  - [示例 2：增量修改与局部网格更新 (`DeltaMesher`)](#示例-2增量修改与局部网格更新-deltamesher)
  - [示例 3：实现自定义 `VoxelSource` 外部数据源](#示例-3实现自定义-voxelsource-外部数据源)
- [6. Feature 开关与依赖](#6-feature-开关与依赖)
- [7. 开源协议 (License)](#7-开源协议-license)

---

## 1. 架构定位与核心职责

在 `libmozitoolkit` 体系中，`mtk-voxel` 承担**体素几何与空间组织权威核心**的职责：

```
                ┌─────────────────────────────────────────┐
                │        上层应用 / 绑定 / 宿主前端        │
                │ (mtk-sync / libmtk / mtk-py / Blender)  │
                └───────────────────┬─────────────────────┘
                                    │ 消费 / 驱动
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                              mtk-voxel                                 │
│  ┌────────────────────────┐  ┌──────────────────────────────────────┐  │
│  │   VoxelSource (抽象)   │  │        VoxelStorage (3D 稀疏世界)     │  │
│  │ (Anvil / Stream / DB)  │  │ (Section 字典 / 选区裁切 / 脏区块标记)  │  │
│  └───────────┬────────────┘  └──────────────────┬───────────────────┘  │
│              │ Ingestion                        │ PaddedVoxelArray     │
│              └─────────────────► ┌──────────────▼─────────────┐        │
│                                  │   SectionMesher / AO       │        │
│                                  │ (流体重建 / Biome 渐变)    │        │
│                                  └──────────────┬─────────────┘        │
└─────────────────────────────────────────────────┼──────────────────────┘
                                                  ▼
                                ┌───────────────────────────────────┐
                                │   MeshData (mtk-core POD 网格)    │
                                └───────────────────────────────────┘
```

- **纯数据流规范**：以紧凑的 1D/3D 缓冲、BlockState 字符串或 Palette 索引为输入，直接产出包含顶点位置、法线、四边形/三角形索引、UV 及 12 项标准面属性（来源贴图调色板、图集 Chunk ID、UV 变换/旋转、动画时序/帧尺寸、Biome Tint、方块坐标）的 `MeshData`。
- **宿主无关 (Host-Agnostic)**：核心内部采用标准 3D 几何坐标系运算，支持运行时输出 `Minecraft` 原生、`ZUpRightHanded` (Unreal/Blender) 或 `YUpRightHanded` (Godot/glTF/Three.js/WebGPU) 坐标。
- **极致吞吐设计**：采用 4,096 紧凑定长数组、18×18×18 邻域垫片 (Apron)、Palette 级着色预解析（消除内部热循环哈希查找）以及 Rayon 多区块并行。

---

## 2. 核心模块与系统拓扑

| 模块路径 | 职责与功能 |
| :--- | :--- |
| **`storage`**               | 16×16×16 单区块切片存储 (`SectionStorage`)、3D 稀疏世界容器 (`VoxelStorage`)、全量无剔除体素点云 (`VoxelPointCloud`)、伴生点云源 (`PointCloudVoxelSource`)、场景交换载荷 (`SparseVoxelPayload`, `SparseVoxelColumns`) 与密集 3D Biome (`SectionBiomes`)。 |
| **`source`**                | 统一体素源抽象 Trait：`VoxelReader`、`VoxelWriter`、`VoxelSource` 及批量灌流管线 `ingest_from_source`。 |
| **`mesher`**                | 高性能区块网格化器 (`SectionMesher`)、增量网格化器 (`DeltaMesher`) 与模型数据源枚举 (`ModelSource`)。 |
| **`fluid`**                 | 流体类型枚举 (`FluidType`) 与流体 UV 修复算子 (`repair_quad_fluid_uv`, `batch_repair_fluid_uv`, `repair_mesh_fluid_uv_inplace`, `get_fluid_top_uvs`, `get_fluid_side_uvs`)。 |
| **`biome`**                 | 5×5 (R=2) 盒式平滑生物群系柱计算 (`SmoothedBiomeColumn`)、`get_smoothed_column_biome` 与 `get_point_column_biome`。 |
| **`world`**                 | 统一 3D 场景引擎 (`VoxelWorld`)，封装存储管理、Rayon 并行重建、物理进度实时汇报 (`rebuild_all_with_progress`)、显式线程池配置 (`num_threads`)、单阶段流式跨区块缝合 (`merge_welded_sections`)、增量重建的 Section 边界缝合 (`merge_welded_sections_filtered`，保证 `rebuild_dirty` 与 `rebuild_all` 拓扑一致)、Section 增量网格缓存与清空 (`clear_cache`)、动态资产热重载 (`hot_reload_assets`)、使用图集 Chunk 追踪 (`used_chunk_ids`) 与多源摄取。 |
| **`types`**                 | 配置参数 `MesherConfig`、坐标系枚举 `CoordinateSystem` 与错误类型 `VoxelError`。 |

---

## 3. 核心数据结构与抽象 Trait

### 3.1 紧凑体素存储 (`storage`)

#### `SectionStorage`
16×16×16 单区块切片，采用定长数组 `Box<[u16; 4096]>` 配合局部 `palette: Vec<String>` 存储（0 号槽位固定为 `"minecraft:air"`）：
```rust
pub struct SectionStorage {
    pub coord: IVec3,
    pub palette: Vec<String>,
    pub voxels: Box<[u16; 4096]>,
    pub non_air_count: u32,
}
```
- `is_empty() -> bool`：若 `non_air_count == 0` 则为极速纯空判定。
- `get_local_state(x, y, z) -> &str`：根据局部坐标 `0..16` 获取方块状态。
- `set_local(x, y, z, state) -> bool`：更新局部方块，自动维护 Palette 与 `non_air_count`。
- `build_padded_array(get_neighbor_fn) -> PaddedVoxelArray`：向外扩展 1 格构建 18×18×18 采样窗口。

#### `PaddedVoxelArray`
网格化器的零分支采样垫片（包含 `-1..=16` 范围，空间尺寸 18×18×18 = 5,832 体素）：
```rust
pub struct PaddedVoxelArray {
    pub coord: IVec3,
    pub palette: Vec<String>,
    pub padded_voxels: Vec<u16>,
    pub is_empty: bool,
    pub biome_data: Option<Vec<SmoothedBiomeColumn>>, // 256 列平滑 Biome 数据
}
```

#### `VoxelStorage`
3D 稀疏世界容器，管理全局选区包围盒与稀疏 `SectionStorage` 哈希表：
```rust
pub struct VoxelStorage {
    pub min_x: i32, pub min_y: i32, pub min_z: i32,
    pub size_x: i32, pub size_y: i32, pub size_z: i32,
    pub sections: HashMap<IVec3, SectionStorage>,
    pub biome_column_map: HashMap<[i32; 2], String>,
    pub primary_biome: Option<String>,
    pub dirty_sections: HashSet<IVec3>,
    pub generation: AtomicU64,
}
```
- **选区裁剪与接缝脏标记**：调用 `set_bounds(...)` 自动修剪越界区块，并将边界接缝处的区块自动加入 `dirty_sections` 以便更新交界面遮挡。
- **派生群系元数据唯一入口**：`finalize_biome_metadata()` 从 Section 级 4×4×4 数据重建地表列群系 `biome_column_map` 与主导 `primary_biome`。存档导入、实时同步 baseline、点云重建三条 ingest 路径都必须走这里，禁止各自再实现一份；逐方块 `biome_map` 覆盖层已删除（Section 数据已完整，删除后实测 `get_biome` 与平滑列零差异）。
- **快照与增量更新**：`set_full_snapshot`、`set_section_snapshot`、`apply_delta_update`。
- **纯代码调试世界生成器**：`create_debug_world() -> Result<Self, VoxelError>`、`create_debug_world_from_states(...)`、`create_debug_world_from_pack_stack(...)` 采用与原版 Minecraft `DebugLevelSource` 严格一致的网格公式进行暴力展开，彻底摒弃硬编码体素快照，支持模组方块动态展开，内部采用 `OnceLock` 线程安全懒加载缓存，后续调用克隆仅需 <1ms。

#### `VoxelPointCloud` 与 `PointCloudVoxelSource`
全量无剔除的体素点云数据结构，专用于持久化场景存储、Blender 点网格属性桥接与用户交互式雕刻挖洞重构：
```rust
pub struct VoxelPointCloud {
    pub positions: Vec<f32>,     // 3D 浮点坐标 [x, y, z]（已对齐目标坐标系与原点居中）
    pub block_x: Vec<i32>,       // 绝对离散整数世界坐标
    pub block_y: Vec<i32>,
    pub block_z: Vec<i32>,
    pub state_palette: Vec<String>, // 唯一方块状态调色板
    pub state_indices: Vec<i32>,    // 逐点状态调色板索引
    pub biome_palette: Vec<String>, // 唯一生物群系调色板
    pub biome_indices: Vec<i32>,    // 逐点群系调色板索引
    pub entity_palette: Vec<String>, // 唯一方块实体 (Visual NBT JSON) 调色板
    pub entity_indices: Vec<i32>,    // 逐点实体调色板索引 (-1 为普通无 NBT 方块)
    pub light_levels: Vec<u8>,      // 复合光照等级
}
```
- `to_point_cloud(config) -> VoxelPointCloud`：全量提取所有非空体素（**绝对不参与任何面遮挡剔除**，内部实心方块 100% 完整保留，并提取方块实体 NBT）。
- `from_point_cloud(cloud) -> VoxelStorage`：从点云无损重建 `VoxelStorage`，支持用户手动删除局部点后自动生成空腔和露出内表面，完整保留存活方块实体。
- `to_mesh_data() / from_mesh_data()`：与标准纯点 `MeshData`（无多边形面）互转，无缝桥接 DCC 属性。
- `state_palette` / `biome_palette` / `entity_palette` 调色板体系：点云全面采用调色板与逐点 `i32` 索引，不再为每个点物化 `String`，方块实体 NBT 按需稀疏挂载。

---

### 3.2 统一体素源抽象 (`source`)

为了将体素存储与输入上游（如 Anvil 存档、LevelDB、实时网络流、点云逆向推算）彻底解耦，`mtk-voxel` 定义了统一抽象接口：

```rust
/// 只读空间访问
pub trait VoxelReader {
    fn get_block(&self, x: i32, y: i32, z: i32) -> &str;
    fn get_section(&self, sx: i32, sy: i32, sz: i32) -> Option<&SectionStorage>;
    fn contains_section(&self, sx: i32, sy: i32, sz: i32) -> bool;
    fn block_bounds(&self) -> Option<(IVec3, IVec3)>;
    fn get_biome(&self, x: i32, y: i32, z: i32) -> &str;
}

/// 写入与标记
pub trait VoxelWriter {
    fn set_block(&mut self, x: i32, y: i32, z: i32, state: &str, biome: Option<&str>);
    fn set_section(&mut self, sx: i32, sy: i32, sz: i32, section: SectionStorage);
    fn mark_section_dirty(&mut self, sx: i32, sy: i32, sz: i32);
    fn set_bounds(&mut self, min_x: i32, min_y: i32, min_z: i32, size_x: i32, size_y: i32, size_z: i32);
    fn clear(&mut self);
}

/// 跨数据源统一生产者
pub trait VoxelSource {
    fn source_name(&self) -> &str;
    fn estimated_section_count(&self) -> Option<usize>;
    fn has_section(&self, sx: i32, sy: i32, sz: i32) -> bool;
    fn load_section(&mut self, sx: i32, sy: i32, sz: i32) -> Result<Option<SectionStorage>, VoxelError>;
    fn section_bounds(&self) -> Option<(IVec3, IVec3)>;
    fn sample_biome(&self, x: i32, y: i32, z: i32) -> Option<&str>;
}
```

使用 `ingest_from_source(source, target, sections)` 可将数据无缝搬运至任意 `VoxelWriter`。

---

### 3.3 网格化配置与统计 (`types`)

```rust
pub struct MesherConfig {
    pub coordinate_system: CoordinateSystem, // Minecraft / ZUpRightHanded / YUpRightHanded
    pub origin_centered: bool,               // 是否将原点居中到底部中心
    pub weld_vertices: bool,                 // 是否合并共面/重合顶点形成流形拓扑
    pub selection_bounds: Option<([i32; 3], [i32; 3])>,
    pub enable_ao: bool,                     // 是否计算 4 顶点 AO 遮蔽
    pub exclude_hidden_volume: bool,         // 是否剔除复杂模型内部遮挡面
    pub mesh_fluids: bool,                   // 是否生成水/岩浆物理曲面
    pub ctm_solver: Option<Arc<CtmSolver>>,  // CTM 47/17 连接纹理求解器
    pub atlas_address_map: Option<Arc<AtlasAddressMap>>, // 图集 UV 映射表
    pub biome_resolver: Option<Arc<BiomeResolver>>,      // 生物群系解析器
    pub custom_aliases: Option<Arc<HashMap<String, Vec<String>>>>,
    pub enable_alternate_blocks: bool,       // 是否启用原版交替模型随机旋转（泥土、石头、睡莲等）
    pub enable_random_offsets: bool,         // 是否启用植物三维空间确定性抖动（花草、竹子等）
}
```

---

## 4. 核心算法与功能实现

### 4.1 18×18×18 邻域采样与零分支遮挡剔除

在遍历单区块的 4,096 个内部体素时，网格化器直接索引 `PaddedVoxelArray`（尺寸 18×18×18）：
- 内部坐标 `(lx, ly, lz)` 对应垫片坐标 `(lx+1, ly+1, lz+1)`；
- 6 个相邻方向的邻域方块直接通过固定偏移 `(px+dx, py+dy, pz+dz)` 获取，**无需任何边界条件分支或越界检查**；
- **两阶段预解析加速**：在进入循环前构建 `PaletteMeshingData`，将各 BlockState 的模型面、材质槽、图集 UV 及染色规则预先解析至 Palette 级（单区块仅执行数十次），热循环内部为纯连续内存读取。
- **6 向分桶与跳过**：模型面划分为 6 个主方向分桶与免剔除分桶。若邻居方块不透明，1 次判定即可直接跳过该方向的所有面。

### 4.2 4 顶点平滑 AO 与各向异性对角线折痕消除

对指定朝向的面，通过采样邻近 3 个方块（2 个正交邻居与 1 个对角邻居）计算 4 个顶点的离散遮挡级别 $0..=3$：

$$\text{AO} = \begin{cases} 0 & \text{若 side1 与 side2 均不透明（角被遮挡）} \\ 3 - (\text{side1} + \text{side2} + \text{corner}) & \text{否则} \end{cases}$$

遮挡等级线性映射为亮度乘子 $[0.2, 0.467, 0.733, 1.0]$。

#### 各向异性折痕消除（内部算子）
四边形剖分为 2 个三角形时，若对角线剖分方向与光照梯度不匹配，会产生明显的斜向三角面光照撕裂。`mtk-voxel` 内部通过比较对角顶点 AO 之和进行动态拓扑翻转：
```rust
#[inline]
pub(crate) fn should_flip_quad_diagonal(ao: [f32; 4]) -> bool {
    (ao[0] + ao[2]) > (ao[1] + ao[3])
}
```

---

### 4.3 物理流体表面与流向 UV 重建

- **流体高度插值**：`calculate_fluid_corner_heights` 采样中心与 8 邻域流体高度，结合 Minecraft 原版物理规则（静止水源表面张力 $8/9$ 高度、下落水流高度为 $1.0$）计算 4 角高度 $(c_{\text{NW}}, c_{\text{NE}}, c_{\text{SE}}, c_{\text{SW}})$。
- **流向矢量与 UV 旋转**：`calculate_fluid_flow_vector` 依据周围 4 邻域及下层方块高度差计算流动矢量 $(v_x, v_z)$ 与旋转角 $\theta = \text{atan2}(v_z, v_x) - \frac{\pi}{2}$，并在生成顶面网格时对 UV 应用旋转变换矩阵。
- **高度感知面剔除**：六个面统一调用 `FaceCuller::should_render_fluid_face`。顶面以四角最小值、侧面以该侧两角最大值作为 `face_height`，复刻原版 `FluidRenderer`：非满高流体（水源 $8/9$、斜向流动）位于方块下方时顶面可见；仅当表面到达 $y = 1.0$ 时才被上方满面方块遮挡。

---

### 4.4 3D Quart Biome 存储、5×5 (R=2) 均匀盒式平滑过渡与边界防渗色

对齐 Minecraft 1.18+ (Caves & Cliffs) 三维生物群系架构与原版客户端 `LevelRenderer` 5×5 ($R=2$) 均匀盒式滤波核 (`BIOME_KERNEL_R2`)：

```rust
pub const BIOME_KERNEL_R2: &[(i32, i32, f32)] = &[
    (-2, -2, 1.0), (-2, -1, 1.0), ... (0, 0, 1.0), ... (2, 2, 1.0),
];
```

- **3D 4×4×4 Quart 调色板存储 (`SectionBiomes`)**：每个 `SectionStorage` 原生携带 64 格（$4\times 4\times 4$ Quarts）调色板生物群系数据，实现 $O(1)$ 三维生物群系查询。
- **自适应 256 / 1024 查表 (`PaddedVoxelArray`)**：当区块内生物群系无垂直变化时仅预计算 256 列（$16\times 16$）；当存在三维垂直群系分层（如地表 `dark_forest`、地下 `lush_caves`）时自动扩展为 1024 项（$4\times 16\times 16$）预计算 `SmoothedBiomeColumn`。
- **沼泽 2D Perlin 噪声与特异群系路由**：内置沼泽 (`swamp` / `mangrove_swamp`) 2D Perlin 噪声（缩放 `0.0225`，阈值 `-0.1`）双色草方块斑驳采样；当邻域包含特异色彩群系（恶地、沼泽、黑森林、樱花树林、苍白之园）时自动标记 `has_custom_*` 并将面 `tint_type` 路由至 `TINT_TYPE_HARDCODED (4.0)`。
- **选区边缘钳位防护 (Edge Clamping)**：针对选区外部未加载区域，采样坐标被严格钳位至选区有效边界，彻底杜绝边界处因外推默认值产生 Plains 平原默认色彩渗色。

---

### 4.5 增量网格重构 (`DeltaMesher`)

在运行时发生方块放置或破坏时，调用 `DeltaMesher::rebuild_dirty_sections`：
- 仅提取 `world.dirty_sections` 标记的区块进行邻域重建；
- 支持 `find_affected_sections`：对于普通方块变动，自动扩散并脏化相邻的 6 向邻域区块；对于流体变动，自动将脏标记扩展至 $3\times 3\times 3$ 邻域。

---

### 4.6 预烘焙交替模型随机旋转与植物位置确定性抖动 (Alternate Blocks & Plant Offsets)

1:1 逆向并严格对齐 Minecraft Java 原版客户端渲染行为，完全基于纯算术空间坐标哈希，网络包无需传输任何多余状态数据：

1. **交替方块模型 (Alternate Blocks)**：
   - 自然方块（如 `dirt`, `stone`, `sand`, `grass_block`, `lily_pad` 等）预先由 `mtk-model` 离线预烘焙为 4 种独立正交旋转分桶（0°、90°、180°、270°），存储于 `BakedVariantGroup`；
   - 网格化热循环内以 `Mth.getSeed(x, y, z)`（包含 Y 轴，确保垂直堆叠方块朝向错开）配合 48 位 LCG `JavaRandom` 采样，直接检索预烘焙分桶，**彻底杜绝热循环内的实时矩阵旋转与 JSON 计算**。
2. **植物三维空间确定性抖动 (Plant Offsets)**：
   - 原版植物（花草、蕨类、枯竹等）在世界生成时沿 X/Z 轴微调 $\pm 0.25$（竹子等 `XYZ` 沿 Y 轴额外向下偏移 $\approx -0.2$）；
   - 严格遵循原版规范以 `Mth.getSeed(x, 0, z)`（**Y 轴恒为 0**）计算种子，确保高草丛、向日葵等两格高双层植物的上下层植物茎秆水平位移绝对对齐；
   - 在网格顶点发射阶段以零拷贝连续向量偏移 `(wx + dx, wy + dy, wz + dz)` 注入，开箱即用。

---

## 5. 快速上手示例 (Quick Start)

### 示例 1：构建体素世界并生成标准化网格

```rust
use glam::IVec3;
use mtk_cull::FaceCuller;
use mtk_voxel::mesher::shading::ModelSource;
use mtk_voxel::mesher::SectionMesher;
use mtk_voxel::storage::VoxelStorage;
use mtk_voxel::types::{CoordinateSystem, MesherConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化 3D 稀疏世界容器
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);

    // 2. 放置方块
    world.set_block(0, 0, 0, "minecraft:stone", Some("minecraft:plains"));
    world.set_block(1, 0, 0, "minecraft:dirt", Some("minecraft:plains"));
    world.set_block(0, 1, 0, "minecraft:water[level=0]", Some("minecraft:plains"));

    // 3. 配置网格化器
    let config = MesherConfig {
        coordinate_system: CoordinateSystem::ZUpRightHanded, // 转换为 Blender / Unreal Z-Up
        origin_centered: true,                               // 底部中心对齐
        weld_vertices: true,                                 // 空间顶点焊接
        enable_ao: true,                                     // 开启平滑 AO
        mesh_fluids: true,                                   // 生成流体曲面
        ..Default::default()
    };

    let culler = FaceCuller::default();

    // 4. 提取垫片数据并执行网格化
    let padded = world.get_section_padded_array(IVec3::new(0, 0, 0));
    let mesh = SectionMesher::mesh_section(&padded, &culler, |_| ModelSource::None, &config);

    println!("生成的顶点数: {}", mesh.positions.len());
    println!("生成的三角形索引数: {}", mesh.indices.len());
    assert!(!mesh.is_empty());

    Ok(())
}
```

---

### 示例 2：增量修改与局部网格更新 (`DeltaMesher`)

```rust
use mtk_cull::FaceCuller;
use mtk_voxel::mesher::DeltaMesher;
use mtk_voxel::storage::VoxelStorage;
use mtk_voxel::types::MesherConfig;

fn main() {
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 32, 16, 32);
    let culler = FaceCuller::default();
    let config = MesherConfig::default();

    // 初始设置
    world.set_block(5, 5, 5, "minecraft:diamond_block", None);
    let _initial_meshes = DeltaMesher::rebuild_dirty_sections(&mut world, &culler, |_| None, &config);

    // 模拟运行时修改方块
    let changes = vec![
        (5, 5, 5, "minecraft:air"),
        (6, 5, 5, "minecraft:gold_block"),
    ];
    world.apply_delta_update(0, 0, 0, &changes);

    // 仅重新构建受影响的区块
    let updated_meshes = DeltaMesher::rebuild_dirty_sections(&mut world, &culler, |_| None, &config);
    println!("受影响并重构的区块数: {}", updated_meshes.len());
}
```

---

### 示例 3：实现自定义 `VoxelSource` 外部数据源

```rust
use glam::IVec3;
use mtk_voxel::source::{ingest_from_source, VoxelSource};
use mtk_voxel::storage::{SectionStorage, VoxelStorage};
use mtk_voxel::types::VoxelError;

struct ProceduralSphereSource {
    radius: f32,
}

impl VoxelSource for ProceduralSphereSource {
    fn source_name(&self) -> &str {
        "ProceduralSphere"
    }

    fn has_section(&self, sx: i32, sy: i32, sz: i32) -> bool {
        sx.abs() <= 1 && sy.abs() <= 1 && sz.abs() <= 1
    }

    fn load_section(&mut self, sx: i32, sy: i32, sz: i32) -> Result<Option<SectionStorage>, VoxelError> {
        let coord = IVec3::new(sx, sy, sz);
        let mut section = SectionStorage::new(coord);
        let base_x = sx * 16;
        let base_y = sy * 16;
        let base_z = sz * 16;

        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    let wx = (base_x + x) as f32;
                    let wy = (base_y + y) as f32;
                    let wz = (base_z + z) as f32;
                    if (wx * wx + wy * wy + wz * wz).sqrt() <= self.radius {
                        section.set_local(x as usize, y as usize, z as usize, "minecraft:stone");
                    }
                }
            }
        }

        Ok(if section.is_empty() { None } else { Some(section) })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = ProceduralSphereSource { radius: 12.0 };
    let mut storage = VoxelStorage::new();

    let loaded = ingest_from_source(&mut source, &mut storage, Some(&[IVec3::new(0, 0, 0)]))?;
    println!("成功灌入 {} 个区块", loaded);
    assert_eq!(storage.get_block(0, 0, 0), "minecraft:stone");

    Ok(())
}
```

---

## 6. Feature 开关与依赖

`mtk-voxel` 在 `Cargo.toml` 中提供精细化的编译特性开关：

```toml
[features]
default = ["std", "parallel", "serde"]
std = ["mtk-core/std", "mtk-cull/std", "mtk-model/std", "mtk-resource/std", "mtk-texture/std"]
parallel = ["dep:rayon", "mtk-model/parallel", "mtk-resource/parallel", "mtk-texture/parallel"]
serde = ["dep:serde", "dep:serde_json", "mtk-core/serde", "mtk-cull/serde"]
```

- **`parallel`** (默认开启)：引入 `rayon` 启用多区块切片并行网格化 (`SectionMesher::mesh_sections_parallel_with_source_and_progress`)。
- **`serde`** (默认开启)：为 `SectionStorage`、`VoxelStorage`、`MesherConfig` 等结构体提供序列化与反序列化支持。

---

## 7. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。

