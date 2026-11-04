# libmozitoolkit (`libmtk`)

[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![DCC Frontend: MoziToolKit](https://img.shields.io/badge/DCC%20Frontend-MoziToolKit-orange.svg)](https://github.com/mozi1924/MoziToolKit)
[![Live Sync: Yefira](https://img.shields.io/badge/Live%20Sync-Yefira%20Mod-brightgreen.svg)](https://github.com/mozi1924/yefira)

**`libmozitoolkit`** 是专为高性能 Minecraft 几何数据解析、微观遮挡剔除、体素流网格化与高保真资产烘焙设计的核心跨平台通用 Rust 库套件。

作为 **[MoziToolKit 2.0](https://github.com/mozi1924/MoziToolKit)** 体系的高性能无头核心引擎，本项目遵循 **零宿主依赖（Host-Agnostic）**、**纯数据输入输出（Data-in, Data-out）** 与 **WASM 原生兼容** 原则，既可直接嵌入 Blender 插件，也能无缝运行于浏览器前端（WebGPU / Three.js）、独立 GUI 软件或现代游戏引擎（Bevy / Godot / Unreal）。

---

## 🌐 三位一体协同生态体系 (Three-Pillar Architecture)

`libmozitoolkit` 在 MoziToolKit 生态拓扑中扮演纯数据与高性能计算中枢：

| 工程与角色 | 职责定位 | 技术栈与边界 | 仓库链接 |
| :--- | :--- | :--- | :---: |
| **[`MoziToolKit`](https://github.com/mozi1924/MoziToolKit)**<br>*(DCC 插件前端)* | **Blender 宿主界面与场景装配**<br>提供 3D 视图侧边栏面板、操作符、规范 PBR 材质节点树生成、点云伴生管理及视口实时渲染调度。 | Python 3.11+<br>Blender 5.0+ Extension | [🔗 GitHub](https://github.com/mozi1924/MoziToolKit) |
| **`libmozitoolkit`**<br>*(Rust 核心引擎 `libmtk`，本仓库)* | **无头高性能几何与体素计算底座**<br>负责体素流解析、1.21+ 方块模型烘焙、邻域面遮挡剔除、2D 矩形装箱、66 种原版 Biome 调色板与 DNA 内存直灌。 | Rust 2021<br>PyO3 / ABI3 / WASM | **当前仓库** |
| **[`yefira`](https://github.com/mozi1924/yefira)**<br>*(Minecraft 伴生模组)* | **服务端权威体素扫描与实时协同**<br>通过 WebSocket 全双工流推送三维选区与增量体素数据，支持 XXHash64 校验自愈与幽灵漫游模式。 | Java 17/21/25<br>Fabric / Forge / NeoForge | [🔗 GitHub](https://github.com/mozi1924/yefira) |

---

## 模块结构与技术文档 (Workspace Crates)

本项目采用分层内聚治理架构，每个 Crate/Binding 均配有独立的 `README.md` 作为该模块的权威 API 与技术规范事实源：

| Crate / Binding | 优先级 | 职责与功能 | 专属技术文档与 API |
| :--- | :---: | :--- | :--- |
| **[`mtk-core`](crates/mtk-core)** | 核心 | 基础几何基元、紧凑网格缓冲容器 (`MeshData`)、Minecraft 标准 6 向拓扑、自适应像素网格切分 (`subdivide`) 与智能挤出/UV 修复 (`extrude`) | [📖 `crates/mtk-core/README.md`](crates/mtk-core/README.md) |
| **[`mtk-cull`](crates/mtk-cull)** | 核心 | 6 向邻域遮挡状态机、原版剔除规则、2D 矩形差集切分与外部网格叠面剔除 (`cull_mesh_faces`) | [📖 `crates/mtk-cull/README.md`](crates/mtk-cull/README.md) |
| **[`mtk-model`](crates/mtk-model)** | 核心 | 无头 BlockState 状态机解析与对齐 1.21+ 规范的 Model JSON 模型烘焙、OBJ 导出/加载 | [📖 `crates/mtk-model/README.md`](crates/mtk-model/README.md) |
| **[`mtk-voxel`](crates/mtk-voxel)** | 核心 | 纯体素核心：16x16x16 Chunk Section 体素存储、流体曲面计算、平滑环境光遮蔽 (AO)、网格化器 (Mesher)、差量网格构建与统一体素源抽象接口 (`VoxelSource`) | [📖 `crates/mtk-voxel/README.md`](crates/mtk-voxel/README.md) |
| **[`mtk-sync`](crates/mtk-sync)** | 核心 | 实时网络协同：原生 WebSocket 客户端、小端序二进制协议编解码、对接模组 [`Yefira`](https://github.com/mozi1924/yefira) 与 Live Sync 会话管理（遵循 [RFC v3.1 协议规范](https://github.com/mozi1924/MoziToolKit/blob/main/docs/live_sync/live_sync_protocol.md)） | [📖 `crates/mtk-sync/README.md`](crates/mtk-sync/README.md) |
| **[`mtk-resource`](crates/mtk-resource)** | 核心 | 无头资源包虚拟文件系统 (VFS)、.mcmeta 动图元数据、原版 atlases/*.json 规范解析与 CTM 47/17 连接纹理求解 | [📖 `crates/mtk-resource/README.md`](crates/mtk-resource/README.md) |
| **[`mtk-texture`](crates/mtk-texture)** | 核心 | 矩形空间装箱 Stitcher、多类别 PBR 图集与 Companion Overlay 贴图烘焙、透明度采样与边缘外推填充 | [📖 `crates/mtk-texture/README.md`](crates/mtk-texture/README.md) |
| **[`mtk-material`](crates/mtk-material)** | 核心 | 66 种原版生物群系调色板与线性色彩数学引擎 (SSOT)、`BiomeResolver` 模型扫描与预编译映射、多线程并行 UV 重映射与外部别名解算 | [📖 `crates/mtk-material/README.md`](crates/mtk-material/README.md) |
| **[`mtk-save`](crates/mtk-save)** | 核心 | 现代 Minecraft 存档加载核心：基于内置零拷贝 NBT 解码、Anvil `.mca` 区域文件直接寻址、384 高度世界 3D 生物群系与方块状态解包、按需空间切片流式数据源 (`VoxelSource`) | [📖 `crates/mtk-save/README.md`](crates/mtk-save/README.md) |
| **[`mtk-package`](crates/mtk-package)** | 核心 | 统一二进制资产缓存容器编解码器：64 字节头部校验、分块 TOC 索引、`memmap2` 零拷贝解析与分块 Zstd 压缩 | [📖 `crates/mtk-package/README.md`](crates/mtk-package/README.md) |
| **[`libmtk`](crates/libmtk)** | 应用层 | 应用编排 Crate：端到端资产预编译管线 (`prebake`)。**不再重导出领域类型**，下游按需直连 `mtk-*` | [📖 `crates/libmtk/README.md`](crates/libmtk/README.md) |
| **[`mtk-py`](bindings/mtk-py)** | **P0 (当前重点)** | 基于 PyO3 的统一 Python 扩展模块 (`libmtk_py`)，提供通用 Buffer Protocol 内存视图，直供 [`MoziToolKit`](https://github.com/mozi1924/MoziToolKit) 消费 | [📖 `bindings/mtk-py/README.md`](bindings/mtk-py/README.md) |
| **[`mtk-cli`](crates/mtk-cli)** | **P1 (工具链)** | 独立命令行工具 (`mtk`)，支持图集烘焙 (`atlas`)、模型烘焙/导出 (`model` / `export`)、CTM 测试 (`ctm`)、规则校验 (`verify`) 与性能基准 (`bench`) | [📖 `crates/mtk-cli/README.md`](crates/mtk-cli/README.md) |
| **[`mtk-bench`](crates/mtk-bench)** | **P1 (基准压测)** | 4000 区块与网格面剔除基准性能压测套件 | [📖 `crates/mtk-bench/README.md`](crates/mtk-bench/README.md) |
| **[`mtk-testkit`](crates/mtk-testkit)** | **Dev (测试基建)** | 仅用于测试的资产解析器与内置 JSON fixtures（`publish = false`），统一真包位置发现与缺省跳过策略 | [📖 `crates/mtk-testkit/README.md`](crates/mtk-testkit/README.md) |
| **[`mtk-wasm`](bindings/mtk-wasm)** | **P2 (远期预备)** | 基于 wasm-bindgen 的 WebAssembly 绑定，支持浏览器/Node.js/WebGPU 零拷贝 TypedArray 视图 | [📖 `bindings/mtk-wasm/README.md`](bindings/mtk-wasm/README.md) |
| **[`mtk-host-memory`](adapters/mtk-host-memory)** | **P0 (宿主内存)** | 宿主无关 raw slice → pointer 复制原语，无线程/核心/宿主 SDK 依赖 | [📖 `adapters/mtk-host-memory/README.md`](adapters/mtk-host-memory/README.md) |
| **[`mtk-blender-dna`](adapters/blender/mtk-blender-dna)** | **P0 (DNA 适配)** | Blender 语义 raw slice → pointer 包装；不依赖核心 crate | [📖 `adapters/blender/mtk-blender-dna/README.md`](adapters/blender/mtk-blender-dna/README.md) |
| **[`libmtk_blender_dna`](adapters/blender/mtk-blender-dna-py)** | **P0 (独立适配 wheel)** | 独立 ABI3 wheel，通过 Python Buffer Protocol 接收核心 `memoryview` 直写 Blender DNA 指针，并支持零拷贝直读与 memoryview 映射 | [📖 `adapters/blender/mtk-blender-dna-py/README.md`](adapters/blender/mtk-blender-dna-py/README.md) |

---

## 项目文档与设计规范

- 🤖 **[Agent 协作与开发规范 (`GEMINI.md`)](GEMINI.md)**：工作区协同、开发铁律、多工作区交互、Crate 新增/变更规约。
- 📋 **[开发路线与 TODO 清单 (`TODO.md`)](TODO.md)**：阶段演进规划、核心算法与数据管道里程碑。
- 🏛️ **[系统架构设计 (`docs/ARCHITECTURE.md`)](docs/ARCHITECTURE.md)**：分层拓扑、模块职责、数据流向。
- 📖 **[API 抽象与全局契约手册 (`docs/API_REFERENCE.md`)](docs/API_REFERENCE.md)**：全局通用契约、坐标系规范、连续内存协议、错误体系。
- 🔌 **[跨语言绑定与胶水层设计 (`docs/BINDINGS_DESIGN.md`)](docs/BINDINGS_DESIGN.md)**：Python Wheel、C-ABI FFI、WASM 规范与 Blender 对接范式。
- 🧹 **[API 暴露面审计 (`docs/API_SURFACE_AUDIT.md`)](docs/API_SURFACE_AUDIT.md)**：零消费者 Python/Rust API 清单、过度复杂接口诊断与下一轮清理批次。

---

## 构建与测试

### 0. 提交前门禁（与 CI 一致，必须全绿）
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

> 需要真实 Minecraft 资产的集成测试通过 [`crates/mtk-testkit`](crates/mtk-testkit) 自动定位资源，优先级为：环境变量 → 工作区相邻目录（`../mc`、`../26.2-Fabric.jar`、`~/Downloads/SPBR-21.zip`）→ 内置 fixtures。
> 可用环境变量：`MTK_TEST_ASSETS` / `MTK_TEST_JAR` / `MTK_TEST_RESOURCE_PACK` / `MTK_TEST_SAVE` / `MTK_TEST_MODELS_CACHE`。资产缺失时相关测试会优雅跳过，不会 panic。

### 1. 本地原生编译与测试
```bash
cargo test --workspace
```

### 2. 独立 CLI 工具构建
```bash
cargo build --release -p mtk-cli
```

### 3. WebAssembly 兼容性校验
```bash
cargo check -p mtk-wasm --target wasm32-unknown-unknown
```

### 4. Python 扩展构建

- **开发态直通构建（Dev Direct-Link，推荐日常开发）**：
  ```bash
  cargo build-all-py      # 一键编译 mtk-py 与 mtk-blender-dna-py（自动注入 extension-module）
  cargo build-py          # 仅编译通用计算内核 mtk-py
  cargo build-dna         # 仅编译 Blender DNA 适配器 mtk-blender-dna-py
  ```
  产物输出至 `target/release/`，通过软链接由 `MoziToolKit/dev/loader.py` 即时载入 Blender。

- **发布态 Wheel 构建（Release Packaging，需专属虚拟环境）**：
  ```bash
  source .venv/bin/activate
  maturin build --release -m bindings/mtk-py/Cargo.toml
  maturin build --release -m adapters/blender/mtk-blender-dna-py/Cargo.toml
  ```

### 5. 持续集成 (GitHub Actions)
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) 在 push/PR 时执行：`fmt --check`、`clippy -D warnings`、`cargo test --workspace`（从临时 Release 注入真包）、`wasm32-unknown-unknown` 兼容性检查与 abi3 Python wheel 构建。

---

## 开源协议

本项目基于 **GNU General Public License v3.0 or later (GPL-3.0-or-later)** 开源。
