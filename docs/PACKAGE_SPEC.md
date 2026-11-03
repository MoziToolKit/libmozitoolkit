# MTK 二进制中间存储包与场景交换容器规范 (Package Specification)

本文档是 `libmozitoolkit` (`libmtk`) 与 `MoziToolKit` 生态的通用二进制存储规范，定义了统一的 **MTK 二进制容器结构 (`.mtkcache` / `.mtkscene`)**，涵盖**全量资产预编译缓存**与**轻量级场景中间交换**两大业务场景。

---

## 1. 设计动机与核心哲学

### 1.1 现实痛点
1. **散文件膨胀与 I/O 碎片 (Loose Files Overhead)**：
   全量资产预编译缓存若直接展开在文件系统目录中（包含数千至数万个小尺寸 PNG 贴图、模型 JSON、映射表等），会导致磁盘 inode 激增、文件系统元数据查找延迟、跨磁盘/跨机复制极慢、以及缓存完整性校验成本极高。
2. **传统 3D 交换格式的“几何爆炸” (Geometry Explosion in 3D Formats)**：
   若将 Minecraft 建筑/世界选区直接导出为传统网格格式（OBJ、USD、FBX、glTF），数十万个方块会膨胀为数百万个多边形面、海量法线与 UV 缓冲，导致文件体积迅速膨胀至数百 MB 甚至数 GB。而且一旦烘焙为网格，面剔除规则、环境光遮蔽 (AO) 样式、图集分辨率即被永久固化，接收方无法再根据 DCC 宿主或材质管线灵活调整。

### 1.2 核心哲学
- **统一底层容器 (Unified Container Architecture)**：
  缓存包与交换包共享完全一致的二进制底层物理结构（统一文件头、分块目录表 TOC、Zstd 块压缩、64 字节对齐）。
- **Profile 业务分化 (Specialized Profiles)**：
  - **`AssetCache Profile`（全量预编译缓存包，`.mtkcache`）**：打包全量预烘焙图集、Standalone PBR 贴图库、全局模型数据库与生物群系映射表，用于本地毫秒级即时加载。
  - **`SceneInterchange Profile`（轻量场景交换包，`.mtkscene`）**：**不存全量网格**（仅存选区稀疏体素网格），**不存预烘焙图集**（仅按需打包场景用到的独立材质贴图），**按需剪枝模型**。在接收方端现场根据宿主配置实时重构网格并按需构建材质节点或现场图集。

---

## 2. 二进制物理布局 (Physical Layout)

文件整体采用 **Header + Chunk Payloads + Table of Contents (TOC)** 的分块组织形式，支持内存映射 (`mmap`) 零内存开销快速寻址。

```
┌────────────────────────────────────────────────────────┐
│ Fixed 64-Byte Header (固定文件头，含 Magic、版本、TOC 偏移)  │
├────────────────────────────────────────────────────────┤
│ Chunk 0: META  (元数据描述，对齐至 64 字节边界)           │
├────────────────────────────────────────────────────────┤
│ Chunk 1: VOXL  (体素 Section 数据块，SceneInterchange 专属)│
├────────────────────────────────────────────────────────┤
│ Chunk 2: MODL  (方块模型定义 / 烘焙模型缓存)             │
├────────────────────────────────────────────────────────┤
│ Chunk 3: TXTR  (独立贴图 / 纹理数据块流)                 │
├────────────────────────────────────────────────────────┤
│ ... (其余数据分块: ATLS / BIOM 等)                      │
├────────────────────────────────────────────────────────┤
│ Table of Contents (TOC，分块索引表，可独立 Zstd 压缩)    │
└────────────────────────────────────────────────────────┘
```

---

## 3. 文件头规范 (Header Specification)

固定占用 **64 字节**，所有数值字段严格遵循**小端序 (Little-Endian)**。

| 偏移 (Offset) | 类型 (Type) | 字段名称 (Name) | 详细描述 (Description) |
| :--- | :--- | :--- | :--- |
| `0x00` | `[u8; 4]` | `magic` | 固定魔数，ASCII `b"MTKP"` (`0x4D, 0x54, 0x4B, 0x50`) |
| `0x04` | `u16` | `version_major` | 主版本号（格式发生非兼容性结构调整时递增，当前为 `1`） |
| `0x06` | `u16` | `version_minor` | 次版本号（新增可选 Chunk 或向前兼容字段时递增，当前为 `0`） |
| `0x08` | `u16` | `profile` | 包类型：`0 = AssetCache`, `1 = SceneInterchange` |
| `0x0A` | `u16` | `flags` | 特性位标志：<br>• Bit 0: LittleEndian (固定为 1)<br>• Bit 1: TOC 经 Zstd 压缩<br>• Bit 2: 完全自包含 (Self-Contained)<br>• Bit 3..15: 保留 |
| `0x0C` | `u8` | `compression` | 默认分块压缩算法：`0 = None`, `1 = Zstd` |
| `0x0D` | `u8` | `checksum_type` | 校验和算法：仅 `0 = CRC32` |
| `0x0E` | `[u8; 2]` | `reserved` | 预留对齐填充，填 `0x00` |
| `0x10` | `u64` | `created_at` | 创建时间 Unix Epoch 时间戳（秒） |
| `0x18` | `[u8; 16]` | `fingerprint` | 内容唯一指纹 / 资源栈哈希 / UUID |
| `0x28` | `u64` | `toc_offset` | 分块索引目录表 (TOC) 在文件中的绝对字节偏移 |
| `0x30` | `u64` | `toc_length` | 分块索引目录表 (TOC) 占用的字节长度 |
| `0x38` | `u32` | `chunk_count` | 包含的数据分块条目数量 |
| `0x3C` | `u32` | `header_crc32` | 前 60 字节 (`0x00..0x3C`) 的 CRC32 校验值，防止文件头损坏 |

---

## 4. 分块目录表 (Table of Contents - TOC)

TOC 位于文件末尾（或由 `toc_offset` 明确寻址）。当 `flags` 中的 Bit 1 置位时，整个 TOC 缓冲区通过 Zstd 解压后解析。

TOC 由 `chunk_count` 个连续的条目组成：

```rust
pub struct ChunkEntry {
    /// 分块类型标识 (如 b"META", b"VOXL", b"MODL", b"TXTR", b"BIOM", b"ATLS")
    pub chunk_type: [u8; 4],
    /// 分块在文件中的绝对字节起始偏移 (必须 64 字节对齐)
    pub offset: u64,
    /// 压缩后占用字节长度 (若未压缩则等于 decompressed_size)
    pub compressed_size: u64,
    /// 原始解压后字节长度
    pub decompressed_size: u64,
    /// 专用分块压缩算法 (0 = 无压缩, 1 = Zstd)
    pub compression: u8,
    /// 预留字段对齐
    pub reserved: [u8; 7],
    /// 分块原始数据的 CRC32 校验码
    pub checksum: u64,
    /// 资源唯一路径或逻辑键名 (UTF-8 字符串，前缀 u16 长度)
    pub identifier: String,
}
```

---

## 5. 核心分块类型 (Chunk Protocols)

### 5.1 `META` - 全局元数据块
* **格式**：JSON 或 Postcard 序列化。
* **内容**：
  * 生成工具标识与版本（如 `libmozitoolkit v0.2.0`, `Blender 4.2`）。
  * 场景坐标 AABB 选区 `(min_x, min_y, min_z) -> (max_x, max_y, max_z)`。
  * 依赖资源包列表与哈希指纹。
  * 场景统计信息（方块总数、不重复方块种类、依赖纹理数）。

### 5.2 `VOXL` - 稀疏体素列存块 (`SceneInterchange` 专属)
* **存储原则**：
  * 专为 `.mtkscene` 设计，存放选区内有效表面体素与伴生点云数据（`SparseVoxelPayload`）。
  * 采用列存（Struct-of-Arrays, SoA）布局（`SparseVoxelColumns`）：扁平一维连续坐标数组 `(x, y, z)` 与紧凑调色板索引流（`state_indices`, `biome_indices`, `entity_indices`）。
  * **正交离散调色板 (Orthogonal Palettes)**：
    * `state_palette: Vec<String>`：不重复方块状态标识符（如 `minecraft:stone`）。
    * `biome_palette: Vec<String>`：不重复生物群系标识符（如 `minecraft:plains`）。
    * `entity_palette: Vec<BlockEntityData>`：附着方块实体（箱子、告示牌、画、展示框等），内含 `entity_type` 与保留原生 Visual NBT JSON 字符串，实现前瞻兼容。
  * 与 DCC 宿主的伴生体素点云（`VoxelPointCloud`）支持 1:1 零拷贝直接映射与极速互转。

### 5.3 `MODL` - 模型定义与几何块
* **存储内容**：
  * `AssetCache`：使用全局 **`BakedModelDatabase`**（已烘焙的完整方块模型库）。
  * `SceneInterchange`：**Tree-shaken 按需剪枝模型库**，只保留场景体素中实际引用的方块模型定义，且所有网格面 UV 已预先重映射至伴生的 Scene Mini-Atlas。

### 5.4 `TXTR` - 独立贴图与纹理块
* **存储原则**：
  * 存储 Standalone PBR 独立纹理条目与原始 PNG 字节流。
  * 相同纹理严格去重。
  * 可选携带伴生 PBR 贴图（`_n.png` OpenGL 法线 + 高度、`_orm.png` AO/粗糙度/金属度/发光、`_aux.png` IOR/SSS/孔隙度/透射）。

### 5.5 `ATLS` - 图集与映射块
* **存储原则**：
  * `AssetCache`：存储全量预烘焙出的全局多通道图集切片与 `atlas_mapping.json`。
  * `SceneInterchange`：存储在内存中动态拼接的 **Scene Mini-Atlas**（自包含微型多通道 PBR 图集与局部 `mapping.json`），仅按需裁切并装箱当前场景方块实际引用的纹理，模型 UV 已对其对齐。

### 5.6 `BIOM` - 生物群系调色板映射块
* 存储原版及自定义生物群系色阶映射表与 Colormap 贴图（`grass.png`, `foliage.png`）。
* 用于重构网格时在 Python / DCC 端即时计算双线性插值顶点色与 Shader 调色。

---

## 6. 两大业务 Profile 规范对照

| 特性维度 | Profile A: `AssetCache` (`.mtkcache`) | Profile B: `SceneInterchange` (`.mtkscene`) |
| :--- | :--- | :--- |
| **应用场景** | 全量材质包预编译缓存、本地资产库 | 用户工程分享、跨机器/跨 DCC 场景交换 |
| **体素数据 (`VOXL`)** | ❌ 无 | ✅ 稀疏列存体素与方块实体 (`SparseVoxelPayload`) |
| **贴图策略 (`ATLS`)** | ✅ 包含完整多通道全量 Atlas 与全局映射 | ✅ **自包含 Scene Mini-Atlas**（仅包含场景引用的裁剪图集） |
| **模型策略 (`MODL`)** | ✅ 包含全局所有方块的完整模型数据库 | ✅ **Tree-Shaken 模型闭包**（模型 UV 已预映射至 Mini-Atlas） |
| **网格模型数据** | ❌ 不存烘焙的世界大网格 | ❌ **不存全量网格**，现场根据体素+剪枝模型极速重构 |
| **典型文件体积** | 50 MB ~ 500 MB (取决于材质包分辨率) | **500 KB ~ 15 MB** (体积小 1~2 个数量级) |
| **独立脱机能力** | 依赖本地全量资源环境 | **100% 自包含**（无需原版客户端或材质包即可直接渲染） |

---

## 7. 现场重构管线 (Import & Reconstruct Pipeline)

当接收方导入 `.mtkscene` 文件时，`libmtk` 与 `MoziToolKit` 执行以下无损重构流程：

```
                    ┌─────────────────────────┐
                    │  Read .mtkscene (mmap)  │
                    └────────────┬────────────┘
                                 │
           ┌─────────────────────┴─────────────────────┐
           ▼                                           ▼
┌─────────────────────────┐                 ┌─────────────────────────┐
│ Sparse Voxel Ingestion  │                 │  Tree-Shaken Models &   │
│  (SparseVoxelPayload)   │                 │   Self-Contained Atlas  │
└──────────┬──────────────┘                 └────────────┬────────────┘
           │                                             │
           ▼                                             │
┌─────────────────────────┐                              │
│ Reconstruct Storage &   │                              │
│ Companion Point Cloud   │                              │
└──────────┬──────────────┘                              │
           │                                             │
           └─────────────────────┬───────────────────────┘
                                 ▼
                    ┌─────────────────────────┐
                    │     SectionMesher       │
                    │  (Greedy / AO / Tint)   │
                    └────────────┬────────────┘
                                 ▼
                    ┌─────────────────────────┐
                    │   Emit MeshData into    │
                    │  Blender Scene (Zero-C) │
                    └─────────────────────────┘
```

1. **零拷贝秒级解析**：通过 `ScenePackageReader` 快速读取文件头与 TOC，解析 `META` 元数据。
2. **提取自包含模型与微图集**：直接从 `MODL` 与 `ATLS` 读取 Tree-Shaken 几何与规范 PBR 图集，无需耗时的在线装箱。
3. **体素世界网格化**：
   - 将 `SparseVoxelPayload` 转换为 `VoxelPointCloud` 与 `VoxelStorage`；
   - 根据用户导入参数（AO 开关、流体曲面、坐标原点对齐）执行 `SectionMesher` 极速并行网格化。
4. **批量灌入场景与伴生挂载**：
   - 通过 DNA 直写或 Buffer Protocol 快速灌入网格；
   - 挂载 `VoxelPointCloud` 伴生点云（标识属性 `mtk:is_voxel_cloud = True`）；
   - 自动生成并赋予规范 Principled BSDF 材质。

---

## 8. 实施规范与约束

1. **宿主无关原则**：容器编解码器（Reader/Writer）必须是纯 Rust 实现，严禁依赖任何 DCC API。
2. **对齐与零拷贝**：所有 Chunk 数据起始偏移必须 64 字节对齐，以支持无损 Direct I/O 与 `mmap`。
3. **容错与前瞻兼容性**：读写器遇到未知非关键 Chunk 类型时，必须平滑跳过而不是 Panic 崩溃。
