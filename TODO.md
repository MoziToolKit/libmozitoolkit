# 📋 libmozitoolkit (`libmtk`) 开发计划与版本路线图 (Roadmap)

本文档为 `libmtk` 底层核心库的功能演进规划，严格遵循**宿主无关（Host-Agnostic）**与**纯数据输入输出（Data-in, Data-out）**设计原则。前端 Blender 插件规划见 [`MoziToolKit/TODO.md`](../MoziToolKit/TODO.md)。实时协同服务端见 [`yefira/GEMINI.md`](../yefira/GEMINI.md)。

---

## 📌 版本演进总览 (Milestones Overview)

| 版本 | 阶段定位 | 核心主题与边界 | 状态 |
| :--- | :--- | :--- | :--- |
| **v2.0 基础发版** | **核心计算内核** | **通用 3D/体素世界几何、双图集烘焙与材质渲染基石 (Production Core)**<br>Anvil 存档解析、BlockState 烘焙、真实 3D Quart Biome 平滑采样、CTM 求解、静态+动态双图集 (Dual-Atlas) 与直接指针直写。 | ✅ **完成交付 (2026-10-18)** |
| **v2.0 架构淬炼** | **纯净化里程碑** | **B0–B7 架构大清理、预解码 PBR 材质与 6 平台 Wheels 矩阵 (Hardened Milestone)**<br>完成系统级 B0–B7 架构收敛；`libmtk` 收敛为极简应用编排层；拆分独立 `bindings/mtk-blender-dna` 宿主内存适配器；预解码标准 PBR 通道全面接管，物理清除旧 LabPBR 运行时与 `mtk_material_props` 物理属性表；新增 `mtk-profile` 性能探针；原生像素切片三角形转 Quads；RFC v3.1 握手与进度裁决；6 平台 Native Wheels 矩阵验证通过。 | ✅ **全面达成 (2026-10-21)** |
| **v2.1** | **次级大版本** | **高级实体、特殊渲染器与动作重定向 (Advanced Entities & Animation)**<br>支持 1.21.4+ 铜像 Special Renderers 模型、骨骼动画与曲线重定向数学库 (`mtk-anim`)、实体 MCA 文件解析、填充地图解码与自包含场景交换包 (`mtk-package::scene`)。 | 📋 **启动推进中** |
| **v2.2** | **通用面部与格式** | **面部捕捉网络流、平滑滤波与蓝图全覆盖 (Facial Stream & Blueprint Expansion)**<br>Rust 原生高性能 LiveLinkFace UDP (11111) 与 WebSocket 异步接收、One Euro Filter 平滑滤波、ARKit 52 向量化解算；`.litematic` 蓝图解析；2D 俯视小地图切片渲染。 | 📋 **规划排期中** |
| **v2.3+** | **万能生态扩展** | **全格式与全场景覆盖 (Multi-format & Ecosystem Expansion)**<br>基岩版 LevelDB 存储与模组世界扩展、体素反向逆向推算器 (Voxel Guesser)。 | 🔮 **远景规划** |

---

## 🏆 近期重大架构跃迁：v2.0 架构淬炼战役 (2026-10-18 ~ 2026-10-21)

### 1. B0–B7 架构大清理战役 (✅ 全部通过验证)
- [x] **`libmtk` 极简化应用编排**：删除 42 个 `pub use` 块（门面从 175 符号收敛至 9 个），只保留 `prebake` + `pipeline` + `MtkError`；领域 crate 直连，杜绝臃肿门面转发。
- [x] **零调用方绝对清除**：删除 `mtk-ffi`；删除 47 个前端未指名消费的 PyO3 绑定方法；删除 `AnvilWorldSource`、`MeshSanitizer`、`mtk-model::culling` shim、重复贴图属性等死代码。
- [x] **物理剔除 `unified_mesh` 胶水与废弃格式**。
- [x] **数据模型统一为 Palette 调色板表达**：点云与扁平多边形几何流全面调色板化，大幅降低内存占用并提速重映射。
- [x] **拆分宿主内存适配器 (`bindings/mtk-blender-dna`)**：将直连 Blender C++ 连续内存的指针直写逻辑从通用 `mtk-py` 剥离为独立的 `libmtk_blender_dna` 轮子，核心保持 100% 宿主无关。
- [x] **消除 `no_std` 假象**：移除全部 crate 的无用 `std` feature 与条件编译，明确声明为 std-only（wasm32 目标同样使用 std）。

### 2. 预解码 PBR 材质现代化与旧实现物理清除 (✅ 全部完成)
- [x] **预解码 PBR 图集通道 (`ASSET_CACHE_FORMAT_VERSION = 5`)**：
  - 资产预编译期直接将 LabPBR 解码为规范图集通道（Albedo, Normal_GL, Roughness, Metallic, Aux/Emission），丢弃原始 `_n` / `_s`；
  - 提供 `lookup_static(loc)` 与 `lookup_animated(loc)` 权威双通道寻址。
- [x] **物理清除旧材质属性注册表与逐面字段**：
  - 彻底删除 `mtk-material/src/properties/` 与 `material_properties.json`；
  - 彻底移除 `mtk-core` 中的 `material_props` 字段与常量，每面减少一份 Float4 存储；
  - 网格化阶段停止一切逐面物理属性查询，全链统一采用标准 Principled PBR。

### 3. 全链路性能探针与分析体系 (`mtk-profile` - ✅ 全部完成)
- [x] 新增轻量级 `mtk-profile` crate。
- [x] 在 `subdivide`、`extrude`、`VoxelWorld` meshing 及 `mtk-sync` 事件派发全链路植入轻量级微秒性能探针。
- [x] 配合前端精准输出 Rust 核心执行耗时。

### 4. 原生几何算子与拓扑修复 (✅ 全部完成)
- [x] **切片三角形转 Quads**：像素切分后的网格三角面自动重构为规范四边形拓扑。
- [x] **拓扑与 UV 修复**：解决任意旋转网格像素切片时的 UV 扭曲问题；统一动画长条贴图单帧截断规则。
- [x] 确保 PerLoop buffer 拓扑布局一致，杜绝 UV 越界。

### 5. RFC v3.1 Live Sync 服务端协同闭环 (✅ 全部完成)
- [x] 对接 `yefira` 伴生模组，全面支持 Protocol v3、XXHash64 体素校验与增量分片推送。
- [x] 握手决策码 (`decision_code`)、进度归一化 (`normalize_stream_progress`)、增量事件合并 (`poll_events_coalesced`) 由 Rust 权威控制。
- [x] 支持携带 Auth Token 进行安全鉴权，并处理本地 Loopback 直连流程。

### 6. 跨平台 6 平台 Wheels 构建与验证 (✅ 全部完成)
- [x] Windows x64、macOS (arm64/x64)、Linux (glibc/musl) CI 矩阵验证通过，提供 rolling release 自动化流水线。

### 7. DNA 双向零拷贝适配与状态自愈跃迁 (2026-11-01 ~ 2026-11-03 ✅ 全部完成)
- [x] **DNA 直写与直读双向适配 (`mtk-blender-dna-py`)**：通过 Python Buffer Protocol 扩展 `read_mesh_geometry` 与 `read_point_cloud_attributes`，实现 Blender 网格拓扑、面材质、循环 UV 与点云属性的零拷贝直接读取，彻底消除 Python 包装对象中转开销。
- [x] **RFC v3.1 稀疏状态哈希与 0ms 重连 (`mtk-sync`)**：打通 `compute_canonical_state_hash` 与客户端握手状态自愈，实现基于体素哈希指纹的 0ms 零几何重建秒级恢复。
- [x] **自包含场景交换容器 (`.mtkscene` / `libmtk::scene`)**：实现 `SparseVoxelPayload` 列存体素格式、Tree-shaken 模型闭包裁剪、内存微型图集 (Mini-Atlas) 烘焙与 UV 自动重映射。

---

## 🎯 v2.0 正式版：核心计算与材质渲染基石 (✅ 已交付并淬炼)

### 1. 双图集 (Dual-Atlas: Static & Animated) 烘焙与寻址映射 (✅ 已完成)
- [x] 全量材质保底静态图集与长条切片动态图集双通道寻址；
- [x] 动态图集 `.png.mcmeta` 垂直长条打包与帧时间线解析。

### 2. 核心几何与 UV 算子库 (`mtk-core` - ✅ 已完成)
- [x] 自适应像素网格切分系统 (`mtk-core::subdivide`)：四边形自适应细分、非均匀像素吸附与属性双线性插值；
- [x] 智能挤出与 UV 修复系统 (`mtk-core::extrude`)：SMART / INWARD / OUTWARD 模式几何重构、Perlin/Voronoi 噪声与锐边标记；
- [x] 空间邻域遮挡剔除系统 (`mtk-cull`)：6 向邻域状态机与 2D 矩形投影差集微观剔除。

### 3. Minecraft 存档与体素源抽象 (`mtk-save`, `mtk-voxel` - ✅ 已完成)
- [x] 4KiB 扇区直接寻址 Anvil / MCA 文件读取；
- [x] 零拷贝 NBT 反序列化（Chunk, BlockStates, Biomes, LevelData）；
- [x] 统一体素源特征 (`VoxelSource`, `VoxelReader`, `VoxelWriter`)。

### 4. BlockState 烘焙与世界重构 (`mtk-model`, `mtk-voxel` - ✅ 已完成)
- [x] 1.21+ BlockState 条件规则求值、UV Lock、TintIndex、混合式实体模型架构；
- [x] 真实 3D Quart Biome 染色采样与 5×5 原版均匀盒式平滑滤波；
- [x] OptiFine / Continuity CTM 连接纹理 47-tile 求解器。

### 5. 现代 Blender 5.0+ 零拷贝内存直写管道 (`bindings/mtk-blender-dna` - ✅ 已完成)
- [x] C++ 连续内存指针直接写入（顶点、法线、循环 UV、索引、材质槽），消除 Python 对象开销。

### 6. 单文件预编译资产缓存 (`crates/mtk-package` - ✅ 已完成)
- [x] 64 字节 Header + 块级 Zstd 压缩 TOC + 64 字节内存对齐；
- [x] 输出单一 `<fingerprint>.mtkcache`。

---

## 🚀 v2.1 版本：高级实体、特殊渲染器、动作重定向与场景交换 (📋 启动推进中)

> **2.1 版本主题**：拓展特殊渲染模型、实体数据流、动作重定向数学库与自包含场景交换包。

### 1. 1.21.4+ 客户端特殊模型攻坚 (Special Renderers & Copper Golem Statue) (P1)
- [ ] **解析客户端模型新语法 (`mtk-model::special`)**
  - [ ] 扩展模型反序列化器，识别 `"type": "minecraft:special"` 与 `"type": "minecraft:select"`；
- [ ] **铜像 / 铜傀儡像 (Copper Golem Statue) 几何模型生成**
  - [ ] 缺少 `"elements"` 字段时路由至特殊渲染器模型构建器；
  - [ ] 内置铜傀儡像 4 种氧化态与各姿态盒模型几何与贴图映射。

### 2. 动画曲线与骨骼重定向数学库 (`mtk-anim::retarget` - P1)
- [ ] **Mine-imator 关键帧与曲线解算**
  - [ ] 实现 Linear、Instant 与 Bézier 贝塞尔缓动曲线的离散数值解算器；
- [ ] **骨骼变换与坐标轴重定向矩阵流 (Bone Retargeting Math)**
  - [ ] 针对 Y/Z 轴向翻转、四元数局部与世界旋转重定向的高性能批量批处理计算；
  - [ ] 提供从 MI 临时参考骨骼快速重定向至 Rig2 / Ice Cube / BPS / Thomas 的向量化通道。

### 3. 原版静态与悬挂实体解析 (`mtk-save::entity`) (P1)
- [ ] **`entities/r.X.Z.mca` 独立实体区域文件解析**
  - [ ] 扩展 MCA 读取器，支持 Minecraft 1.17+ 独立实体区域文件；
  - [ ] 基于选区 AABB 过滤空间相交实体。
- [ ] **艺术画 (`PaintingEntity`) 解析**
  - [ ] 提取 `variant`, `facing`, `pos`，内置原版 40+ 种画作尺寸表，生成带木质背板的三维画框网格。
- [ ] **物品展示框 (`ItemFrameEntity`) 解析**
  - [ ] 提取展示框旋转角度、发光属性与内容物信息。
- [ ] **原版填充地图数据解码器 (`mtk-save::map`)**
  - [ ] 解析 `data/map_<id>.dat` Gzip NBT 文件；
  - [ ] 内置 60+ 种 `MapColor` 基础色彩表与 4 级明暗倍率，解码 128×128 RGBA8 连续像素流。

### 4. 高级方块实体 NBT 图元生成器 (`mtk-model::ber_nbt`) (P2)
- [ ] **告示牌文字生成器 (Sign Text)**：解析 `front_text` / `back_text`，按原版点阵排版生成面文字图元与发光材质标记；
- [ ] **旗帜多层花纹合成 (Banner Patterns)**：依照原版公式按顺序进行 16 种颜色的多层 Alpha 混合合成；
- [ ] **双箱 (Double Chest) 几何缝合与铰链预留**：依据 BlockState 自动缝合为宽箱子网格并预留铰链轴心。

### 5. 自包含轻量场景交换包与 Mini-Atlas 彻底重构 (`libmtk::scene` / `mtk-package` - ✅ 全部完成 🌟)
- [x] **Mini-Atlas 动静双卷装箱与数学公式修复 (`crates/libmtk/src/scene/exporter.rs`)**：
  - [x] 修复 UV 坐标系垂直颠倒数学公式：`v_min = 1.0 - (y+h)/H`, `v_max = 1.0 - y/H`；
  - [x] 实现静态卷 (Frame 0 + 100% 覆盖) 与动态长条卷 (Strip + 动图元数据) 正交装箱；
  - [x] 扩展 `SceneAtlasSource` 与 `SpriteCrop` 支持 Overlay 覆盖层（`_overlay.png`）；
  - [x] 接入动态实体图集内存缓冲（`dynamic_entity_atlases`），提取 64x64 玩家皮肤与合成旗帜直接缝合进 Mini-Atlas；
  - [x] 增加水/熔岩等流体方块贴图自动防御收集；
- [x] **场景包多卷 ATLS 与极速读取 (`ScenePackageReader`)**：
  - [x] 确保支持读取和解压静态与动态多卷 Mini-Atlas，元数据暴露 `has_animated_atlas`；
- [x] **跨语言 Python 绑定扩充 (`bindings/mtk-py/src/scene.rs`)**：
  - [x] `export_scene_package` 增加接收可选 `dynamic_entity_atlases` 缓冲字典；
  - [x] `SceneMetadata` 暴露 `has_animated_atlas`。

---

## 🛠️ v2.2 版本：通用面部网络流、平滑滤波与蓝图全覆盖 (📋 规划排期中)

### 1. 原生高性能面部捕捉网络流与平滑滤波 (`mtk-face` / `mtk-net` - P1 核心 🌟)
- [ ] 原生 Rust 实现 LiveLinkFace (UDP 11111) 与 WebSocket 异步数据包监听；
- [ ] 零拷贝快速解析 ARKit 52 种 Blendshape 浮点数组与头部姿态四元数；
- [ ] One Euro Filter 自适应低通滤波与丢包插值补偿；
- [ ] 矩阵乘加通用骨骼偏移向量计算与零延迟桥接。

### 2. 建筑蓝图文件直接解析器 (`mtk-schematic` - P1)
- [ ] `.litematic` / `.schematic` 纯 Rust 解析，直接对接 `VoxelSource` 一键生成完整建筑几何。

### 3. 2D 俯视微型小地图切片渲染器 (`mtk-voxel::minimap` - P1)
- [ ] 快速光线表面高度与群系色彩投影，极速输出连续 RGBA8 贴图内存。

---

## 🔮 v2.3+ 远景：万能格式与生态扩展 (Future Vision)

- [ ] 基岩版（Bedrock Edition）存储读取 (`mtk-voxel::storage::leveldb`)；
- [ ] 体素反向逆向推算器 (Voxel Guesser / Mesh Inferrer)。
