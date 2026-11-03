# GEMINI.md - libmozitoolkit (Rust Core) 架构与开发规范

本文档是 `libmozitoolkit` 工作区的核心架构准则与开发铁律。全局通用开发规范与 CLI 工具偏好遵循用户全局规则。

---

## 1. 项目定位与协同拓扑

- **本工作区职责**：纯 Rust 实现的通用 3D/体素几何处理核心引擎，纯数据输入输出 (Data-in, Data-out)、宿主无关 (Host-Agnostic)、高性能无头计算。
- **协同边界**：
  - 为相对路径 `../MoziToolKit` (Blender Addon) 提供 `libmtk_py` (`bindings/mtk-py`) 与 `libmtk_blender_dna` (`adapters/blender/mtk-blender-dna-py`)；
  - 与相对路径 `../yefira` (Minecraft 服务端模组) 共享 RFC v3.1 协议二进制体素编解码标准。

---

## 2. 核心架构规约与铁律 (Golden Rules)

### 规则 1：严格的宿主无关与无头设计 (Host-Agnostic & Headless)
- Rust 核心层（`crates/mtk-*` 与 `crates/libmtk`）**严禁引入任何特定 DCC 宿主的专有库或 API**。
- 采用通用几何流与 POD 数据输入输出，严格使用标准空间坐标系命名（如 `z_up_coordinates`、`mc_world_to_z_up`），严禁出现 DCC 专属命名（如 `blender_coordinates`）。

### 规则 2：胶水层轻量化与零调用方清除 (Bindings & Zero-Copy Policy)
- `mtk-py` 只负责跨语言数据类型转换与内存桥接，严禁编写复杂几何算法。
- 几何与贴图像素优先采用 Buffer Protocol / memoryview 或 DNA 指针直写零拷贝。
- 严格遵循极简暴露面原则，**严禁保留前端未消费的冗余 API 或废弃 shim**；死代码与废弃接口必须坚决物理剔除。

### 规则 3：生物群系与调色板单一事实源 (Biome Palette SSOT)
- 所有 Biome Tint 与颜色映射必须以 `mtk-material::biome` 为唯一权威事实源（内置 66 种原版调色板与线性色彩数学），严禁在其他 Crate 或前端分散、硬编码重复调色板。

### 规则 4：资产缓存 (.mtkcache) 与场景交换包 (.mtkscene) 规范
- **全量资产预烘焙**：Atlas 拼接、PBR 对齐与 Model Baking 由 `libmtk::prebake` 统一无头处理（`ASSET_CACHE_FORMAT_VERSION = 5`）。
- **场景交换中间格式**：`.mtkscene` 由 `libmtk::scene` 统一编解码（`SCENE_PACKAGE_FORMAT_VERSION = 1`），包含：
  1. 稀疏体素紧凑流 (`crates/mtk-voxel/src/storage/sparse.rs`)；
  2. 动态裁剪重装箱的自包含 Mini-Atlas（多通道 PBR 裁剪与 UV 并行重映射）；
  3. Tree-Shaken 方块模型库，实现脱离原版权威资产的便携场景独立分发与网格重构。

### 规则 5：双轮子体系与构建特性门禁
- **日常开发态直连**：使用 Cargo 标准别名 `cargo build-all-py`（已内联附带 `--features extension-module`），构建产物软链接至相对路径 `../MoziToolKit/dev/lib/`。
- **构建门禁红线**：**严禁在 `Cargo.toml` 的 `default` 中开启 `extension-module`**（避免破坏独立测试二进制导致 `cargo test` 缺失 `libpython` 符号失败）。
- **发布态构建**：使用 `maturin build --release` 产出跨平台 wheels；严禁在全局 Python 环境中执行 `pip install`。

### 规则 6：内存安全、Panic 防御与模块防膨胀
- **严禁滥用 Panic**：胶水层与适配层非测试代码严禁 `.unwrap()` / `.expect()`，统一使用 `Result` / `PyResult` 冒泡。
- **最小可见性**：严格限制结构体字段与内部辅助函数的 `pub` 可见性。
- **模块防膨胀与文档同步**：源码单文件常规控制在 500 行以内，杜绝无界追加巨石文件；新增/调整 Crate 必须同步更新根目录 `README.md` 与该 Crate 的 `README.md`。

### 规则 7：CI 完整门禁与本地验证命令
提交前必须保证以下本地门禁全绿（与 GitHub Actions CI 完全对齐，零 Warning）：

```bash
# 1. 格式化规范检查
cargo fmt --all -- --check

# 2. 静态分析 (-D warnings 严格拦截)
cargo clippy --workspace --all-targets -- -D warnings

# 3. WebAssembly 交叉编译检查 (条件编译验证)
cargo check -p mtk-wasm --target wasm32-unknown-unknown

# 4. DNA 独立性门禁 (mtk-blender-dna 严禁反向引入核心 crate)
cargo tree -p mtk-blender-dna | grep -Eq 'mtk-(core|voxel|material|model|texture|resource)' && exit 1 || true

# 5. 全工作区单元与集成测试
cargo test --workspace

# 6. Python 绑定测试 (复用 MoziToolKit 的 uv 虚拟环境与 dev/lib)
cargo build-all-py
PYTHONPATH=../MoziToolKit/dev/lib uv run --directory ../MoziToolKit pytest ../libmozitoolkit/bindings/mtk-py/tests/ -q
PYTHONPATH=../MoziToolKit/dev/lib uv run --directory ../MoziToolKit pytest ../libmozitoolkit/adapters/blender/mtk-blender-dna-py/tests/ -q
```

- 测试资产统一走 `mtk-testkit` 动态解析，缺失时必须优雅跳过并退出，严禁 panic，严禁硬编码绝对路径。

### 规则 8：三层数据交换与宿主无关批量规范 (Tiered Ingestion & Host-Agnostic Model)
- **三层数据交换架构**：
  1. **Tier 1 (DNA 极速零拷贝 - Fast-Path)**：`libmtk_blender_dna` 原生指针直写，仅作为可热插拔的极限加速插件，严禁作为不可替代的硬性前提；
  2. **Tier 2 (DCC 官方批量通道 - Official Safe-Batch)**：以 Blender `foreach_set` / 官方 CPython 稳定 API 为代表的安全通道，作为 DCC 宿主内的绝对安全定海神针；
  3. **Tier 3 (宿主无关标准连续缓冲模型 - Host-Agnostic Buffers)**：纯 Python 标准 Buffer Protocol / Typed MemoryView / NumPy 连续阵列。彻底脱离任何 DCC 运行时，供纯 Python CLI、无头数据管道、AI 训练或第三方 3D 引擎等零成本高效消费。
- **Buffer-First 与强类型规范**：
  - 所有数值型数组（位置、法线、UV、索引）必须通过 `_memoryview` 提供，且必须自带标准类型字符（`'f'`, `'i'`, `'I'`, `'H'`），严禁暴露未类型化的裸字节流，严禁向 Python 堆展开深拷贝列表；
  - 核心几何容器必须提供宿主无关的批处理字典导出（如 `to_buffers()`），使脱离 DCC 的纯 Python 脚本可开箱即用。
- **调色板与索引格式 (Palette + Indices)**：
  - 离散字符串属性（材质键、方块状态、Biome 名等）强制遵循 `(palette: List[str], indices: memoryview)` 紧凑格式，严禁逐顶点/逐面生成独立 Python 字符串。
- **测试与底层算子隔离**：
  - 单方块细粒度烘焙、协议 Mock 编码器、底层几何切分等测试专属或底层细节算子，统一收敛至 `unstable.testing` 子模块，严禁污染顶层生产命名空间。

---

## 3. 核心领域架构与模块划分

```
libmozitoolkit/
├── crates/
│   ├── mtk-core/       -> 基础几何基元、Quad、MeshData、像素切分与挤出算子、跨平台 Profile
│   ├── mtk-cull/       -> 6 向邻域遮挡状态机、2D 矩形差集切分
│   ├── mtk-model/      -> BlockState 状态解析、1.21+ Block Model JSON 烘焙
│   ├── mtk-voxel/      -> 16x16x16 体素存储、稀疏体素 (SparseVoxelStorage)、流体曲面、平滑 AO
│   ├── mtk-sync/       -> RFC v3.1 原生 WebSocket 同步客户端与状态机
│   ├── mtk-resource/   -> VFS、.mcmeta 动图元数据、原版 atlases/*.json 解析
│   ├── mtk-texture/    -> 矩形装箱 Stitcher、多类别 PBR 图集与 Overlay 烘焙
│   ├── mtk-material/   -> 66 生物群系调色板引擎、规范 PBR 预解码、并行 UV
│   ├── mtk-package/    -> .mtkcache 资产包与 .mtkscene 场景包编解码
│   ├── mtk-profile/    -> 全链路微秒级性能探针
│   ├── libmtk/         -> 极简应用编排层 (Prebake, Scene Interchange, Mesh Pipeline)
│   └── mtk-testkit/    -> 测试用资产解析器与内置 fixtures
├── bindings/
│   ├── mtk-py/         -> 宿主无关 PyO3 Python 绑定 (libmtk_py)
│   └── mtk-wasm/       -> 宿主无关 WebAssembly 绑定
└── adapters/
    ├── mtk-host-memory/-> 宿主连续内存切片安全检查底座
    └── blender/        -> Blender C++ 连续内存直写适配器 (libmtk_blender_dna)
```

---

## 4. 文档索引

- **系统架构设计**：[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- **公共 API 参考**：[`docs/API_REFERENCE.md`](docs/API_REFERENCE.md)
- **跨语言绑定设计**：[`docs/BINDINGS_DESIGN.md`](docs/BINDINGS_DESIGN.md)
- **路线图**：[`TODO.md`](./TODO.md)
