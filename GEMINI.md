# GEMINI.md - libmozitoolkit (Rust Core) 架构与开发规范

本文档是 `libmozitoolkit` 工作区的核心架构准则与开发铁律。全局通用开发规范与 CLI 工具偏好遵循用户全局规则。

---

## 0. 第一原则与消费者边界

- **唯一消费者**：`../MoziToolKit` (Blender Addon) 与 `../yefira` (Minecraft 服务端模组)。**不存在任何第三方下游、旧版 wheel、旧版 `.blend` 或旧版配置的兼容义务。**
- **Data In, Data Out**：核心只接受连续 POD 缓冲、显式配置与纯数据 DTO，返回新的纯数据结果。禁止把宿主对象、UI 状态、进程级全局单例混入核心 API。
- **一个能力，一个规范入口**：同一能力只允许一个公开入口。禁止为了“兼容 / 方便 / 未来扩展”并列暴露 `foo`、`foo_flat`、`foo_paletted`、`foo_with_options`、`foo_v2` 等多套 API。
- **零消费者即删除**：没有真实调用方的 API、类型、字段、配置项、测试专用导出，必须在同一提交中物理删除；禁止 `#[deprecated]`、兼容 shim、别名转发、条件编译旧路径与“保留一个版本观察”。

---

## 1. 项目定位与协同拓扑

- **本工作区职责**：纯 Rust 实现的通用 3D/体素几何处理核心引擎，纯数据输入输出 (Data-in, Data-out)、宿主无关 (Host-Agnostic)、高性能无头计算。
- **协同边界**：
  - 为相对路径 `../MoziToolKit` (Blender Addon) 提供 `libmtk_py` (`bindings/mtk-py`) 与 `libmtk_blender_dna` (`adapters/blender/mtk-blender-dna-py`)；
  - 与相对路径 `../yefira` (Minecraft 服务端模组) 共享 RFC v3.1 协议二进制体素编解码标准。
- **无兼容承诺**：本仓库的 Python API、Rust API、二进制格式与配置结构均按“源码级同步升级”处理。接口变更时直接修改消费者，不保留旧入口。

---

## 2. 核心架构规约与铁律 (Golden Rules)

### 规则 1：零兼容、零废弃、零 shim (No Compatibility, No Deprecation, No Shim)

- **禁止兼容层**：严禁 `legacy_*` / `compat_*` / `*_old` / `*_v2` / `*_deprecated` 命名，严禁 `#[deprecated]`，严禁旧参数名转发，严禁“先保留、下版本再删”。
- **禁止运行时能力探测**：绑定层与适配层严禁用 `hasattr()`、`try/except TypeError`、版本号分支去兼容不同原生模块版本。模块版本不匹配就抛错，要求重新构建/安装匹配产物。
- **禁止历史格式双读**：资产缓存、场景包、协议、配置只支持当前版本。旧格式直接拒绝并给出明确错误，不写迁移读取分支。
- **API 变更即一次性破坏**：新增/修改 API 时，同一提交内完成：核心实现、绑定、`../MoziToolKit` 调用方、测试、文档。旧符号物理删除，不保留迁移期。
- **允许的运行时防御**：外部资源缺失、文件损坏、网络中断等运行期错误可以优雅返回 `Result` / 空结果 / 明确异常；但不得把它伪装成旧 API/旧格式兼容层。

### 规则 2：单一规范入口与最小暴露面 (One Canonical Entry, Minimal Surface)

- **默认私有**：`pub(crate)` / 私有模块是默认值。只有被其他 crate、CLI 或绑定明确消费的符号才允许 `pub`；测试不能作为公开 API 的消费者，只能消费 `pub(crate)` 或 `#[cfg(test)]` 入口。
- **模块不裸奔**：禁止把内部模块写成 `pub mod` 后依赖 `pub use` 转发全部内容。内部模块使用私有 `mod`，crate 根只 `pub use` 真正对外契约。
- **禁止巨型 re-export**：crate 根不得出现几十个符号的 `pub use` 清单。新增根导出必须能指出跨 crate 消费者；否则留在原模块内部或降为 `pub(crate)`。
- **一个能力一个名字**：如果批处理/零拷贝/调色板等实现不同，使用一个公开函数 + 内部派发，或一个显式配置 DTO；禁止把实现变体全部提升为公开 API。
- **禁止万能参数**：绑定函数禁止 `options: dict = None, **kwargs` 这类透传式签名。使用显式类型参数或一个强类型配置对象；参数超过约 8 个业务字段时必须收敛为配置 DTO。
- **禁止测试 API 进入稳定面**：测试专用、Mock 专用、底层单步算子只能存在于 Rust `#[cfg(test)]` 或 `unstable.testing`。稳定命名空间只放生产消费者真正调用的入口。
- **禁止包装器污染**：纯数据结果优先返回字典 / 连续缓冲 / 明确 DTO；禁止为 `dict` 再包一层 `PyXxxCapabilities` / `PyXxxResult`，除非该对象承担生命周期或句柄语义。
- **Rust 绑定侧最小导出**：`bindings/mtk-py` 的 `pub use` 只服务于 `#[pymodule]` 注册；PyO3 类与方法不需要对 Rust 其他 crate 公开时，一律 `pub(crate)`。

### 规则 3：Data In, Data Out 与无全局状态 (Pure Data Flow)

- **输入输出为纯数据**：位置、法线、UV、索引、调色板、状态串、配置均以连续缓冲 / POD / 明确 DTO 传递。禁止宿主句柄、Blender RNA、UI 回调状态进入核心 crate。
- **禁止进程级可变全局状态作为 API**：禁止把全局缓存（如 colormap 缓存、资源发现缓存、debug world 缓存）暴露为公开的 `set_*` / `clear_*` / `install_*` 接口。缓存必须是实例内部或函数内部实现细节，对外不可见。
- **禁止隐式回退策略**：绑定层不得在缺资源时偷偷使用内置资产、旧路径或 host fallback。缺什么就返回错误；回退策略由调用方显式决定并显式传参。
- **批量优先**：生产路径不得逐方块 / 逐面调用 Python 再回到 Rust。一次性传入扁平缓冲，Rust 内完成批处理，一次性返回结果。
- **禁止单实现抽象**：只有一种实现的 trait、只有一种变体的 enum、只为“未来可能扩展”而存在的泛型层，一律删除。

### 规则 4：胶水层轻量化与零拷贝 (Bindings & Zero-Copy Policy)

- `mtk-py` 只负责跨语言数据类型转换与内存桥接，严禁编写复杂几何算法、资源发现策略、材质回退策略。
- 几何与贴图像素优先采用 Buffer Protocol / memoryview / typed buffer 零拷贝；memoryview 必须带标准类型字符（`'f'`, `'i'`, `'I'`, `'H'`, `'B'`），禁止未类型化裸字节流与逐元素深拷贝。
- 零拷贝路径必须有真实消费者与性能证据；没有消费者的 DNA/并行/特殊路径直接删除。
- 绑定层不得复制核心逻辑。发现绑定里出现几何/材质/格式判断时，把逻辑下沉到核心 crate 或直接删除。

### 规则 5：生物群系与调色板单一事实源 (Biome Palette SSOT)

- 所有 Biome Tint 与颜色映射必须以 `mtk-material::biome` 为唯一权威事实源（内置 66 种原版调色板与线性色彩数学），严禁在其他 Crate 或前端分散、硬编码重复调色板。
- 颜色输入输出统一使用明确色彩空间字段（`srgb` / `linear`），禁止在 API 名字或参数上含糊其辞。

### 规则 6：资产缓存 (.mtkcache) 与场景交换包 (.mtkscene) 规范

- **全量资产预烘焙**：Atlas 拼接、PBR 对齐与 Model Baking 由 `libmtk::prebake` 统一无头处理（`ASSET_CACHE_FORMAT_VERSION = 6`）。
- **场景交换中间格式**：`.mtkscene` 由 `libmtk::scene` 统一编解码（`SCENE_PACKAGE_FORMAT_VERSION = 1`），包含：
  1. 稀疏体素紧凑流 (`crates/mtk-voxel/src/storage/sparse.rs`)；
  2. 动态裁剪重装箱的自包含 Mini-Atlas（多通道 PBR 裁剪与 UV 并行重映射）；
  3. Tree-Shaken 方块模型库，实现脱离原版权威资产的便携场景独立分发与网格重构。
- 格式版本只增不改。旧版本包直接拒绝；不写旧字段兼容读取、不写字段回退默认值、不写“读到旧包自动迁移”。

### 规则 7：双轮子体系与构建特性门禁

- **日常开发态直连**：使用 Cargo 标准别名 `cargo build-all-py`（已内联附带 `--features extension-module`），构建产物软链接至相对路径 `../MoziToolKit/dev/lib/`。
- **构建门禁红线**：**严禁在 `Cargo.toml` 的 `default` 中开启 `extension-module`**（避免破坏独立测试二进制导致 `cargo test` 缺失 `libpython` 符号失败）。
- **发布态构建**：使用 `maturin build --release` 产出跨平台 wheels；严禁在全局 Python 环境中执行 `pip install`。

### 规则 8：内存安全、Panic 防御与模块防膨胀

- **严禁滥用 Panic**：胶水层与适配层非测试代码严禁 `.unwrap()` / `.expect()`，统一使用 `Result` / `PyResult` 冒泡。
- **最小可见性**：严格限制结构体字段与内部辅助函数的 `pub` 可见性；结构体字段默认私有，提供明确构造器/访问器。
- **模块防膨胀**：源码单文件常规控制在 500 行以内，杜绝无界追加巨石文件。
- **禁止注释掉的旧实现**：删除即删除，不保留注释代码、`#[allow(dead_code)]` 死代码、TODO 兼容项。

### 规则 9：API 准入与删除审计 (API Admission & Deletion Audit)

新增或修改公开 API 前，提交说明 / PR 描述必须逐项回答：

1. **消费者**：`../MoziToolKit`、`../yefira` 或本仓库 CLI/绑定中的具体文件与调用点是什么？没有真实调用方直接拒绝。
2. **规范入口**：这是该能力的唯一入口吗？是否与已有 API 重复？重复则合并或删除旧入口。
3. **数据流**：输入输出是否都是纯数据/缓冲/DTO？是否引入全局状态或隐式回退？
4. **删除清单**：本次是否删除了被替代的旧入口、旧参数、旧测试与旧文档？没有删除清单的“新增”视为未完成。
5. **文档与测试**：文档是否同步更新？测试是否覆盖新的唯一入口？

删除审计必须执行：

```bash
# 1. Python 稳定面快照（构建后）
PYTHONPATH=../MoziToolKit/dev/lib python3 - <<'PY'
import inspect, libmtk_py
for name in sorted(n for n in dir(libmtk_py) if not n.startswith("_")):
    obj = getattr(libmtk_py, name)
    print(name, "CLASS" if inspect.isclass(obj) else "")
PY

# 2. 消费者检索：0 命中即删除候选
rg -n '\bSymbolName\b' ../MoziToolKit ../yefira

# 3. Rust 根导出与公开模块审计
rg -n '^pub (use|mod)' crates/*/src/lib.rs bindings/*/src/lib.rs adapters/*/src/lib.rs
```

删除规则：

- Rust 侧：`pub use` 无跨 crate 消费者 → 删除根导出；模块仅内部使用 → `mod`；item 仅本 crate 使用 → `pub(crate)`；完全无引用 → 删除。
- Python 侧：稳定顶层无生产消费者 → 删除；仅测试/工具使用 → 下沉 `unstable.testing` 或删除；仅 Rust 内部使用 → 不注册到 `#[pymodule]`。
- 文档侧：删除符号的同时删除 README / API 文档 / 示例 / 测试中的引用。禁止留下“已废弃但可用”的说明。

### 规则 10：文档同步与 CI 完整门禁

**文档同步是 API 定义的一部分**。任何 API / 格式 / 配置变更必须在同一提交更新：

- 根目录 `README.md`；
- 对应 crate 的 `README.md`；
- `docs/API_REFERENCE.md`、`docs/BINDINGS_DESIGN.md`、`docs/PACKAGE_SPEC.md`（如涉及）；
- `bindings/mtk-py/README.md`（Python 导出面唯一手册）；
- `../MoziToolKit` 的 bridge 文档、调用方与测试（若接口改变）。

文档中不得出现已删除符号、旧参数名、旧版本兼容说明。CI 前用 `rg` 检查变更符号是否仍残留在文档/示例中。

提交前必须保证以下本地门禁全绿（与 GitHub Actions CI 完全对齐，零 Warning）：

```bash
# 1. 格式化规范检查
cargo fmt --all -- --check

# 2. 静态分析 (-D warnings 严格拦截)
cargo clippy --workspace --all-targets -- -D warnings

# 3. WebAssembly 交叉编译检查（仅在保留 mtk-wasm 时执行）
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

### 规则 11：三层数据交换与宿主无关批量规范 (Tiered Ingestion & Host-Agnostic Model)

- **三层数据交换架构**：
  1. **Tier 1 (DNA 极速零拷贝 - Fast-Path)**：`libmtk_blender_dna` 原生指针直写，仅作为可热插拔的极限加速插件，严禁作为不可替代的硬性前提；
  2. **Tier 2 (DCC 官方批量通道 - Official Safe-Batch)**：以 Blender `foreach_set` / 官方 CPython 稳定 API 为代表的安全通道，作为 DCC 宿主内的绝对安全定海神针；
  3. **Tier 3 (宿主无关标准连续缓冲模型 - Host-Agnostic Buffers)**：纯 Python 标准 Buffer Protocol / Typed MemoryView / NumPy 连续阵列。彻底脱离任何 DCC 运行时，供纯 Python CLI、无头数据管道或第三方 3D 引擎等零成本高效消费。
- **Buffer-First 与强类型规范**：
  - 所有数值型数组（位置、法线、UV、索引）必须通过 `_memoryview` 提供，且必须自带标准类型字符（`'f'`, `'i'`, `'I'`, `'H'`），严禁暴露未类型化的裸字节流，严禁向 Python 堆展开深拷贝列表；
  - 只有在存在真实宿主无关消费者时，才提供唯一的批处理字典导出（如 `to_buffers()`）；没有消费者时不得为假想场景保留；
  - `to_buffers()` 只能有一份规范实现，禁止再派生 `to_numpy()` / `to_dict()` / `to_flat_*` 等重复出口。
- **调色板与索引格式 (Palette + Indices)**：
  - 离散字符串属性（材质键、方块状态、Biome 名等）强制遵循 `(palette: List[str], indices: memoryview)` 紧凑格式，严禁逐顶点/逐面生成独立 Python 字符串。
- **测试与底层算子隔离**：
  - 单方块细粒度烘焙、协议 Mock 编码器、底层几何切分等测试专属或底层细节算子，统一收敛至 `unstable.testing` 子模块，严禁污染顶层生产命名空间；
  - 仅被测试使用的稳定顶层 API 视为违规，必须下沉或删除。

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
│   ├── mtk-save/       -> Anvil MCA、NBT、存档空间切片
│   ├── mtk-package/    -> .mtkcache 资产包与 .mtkscene 场景包编解码
│   ├── libmtk/         -> 极简应用编排层 (Prebake, Scene Interchange)
│   └── mtk-testkit/    -> 测试用资产解析器与内置 fixtures
├── bindings/
│   ├── mtk-py/         -> 宿主无关 PyO3 Python 绑定 (libmtk_py)
│   └── mtk-wasm/       -> 宿主无关 WebAssembly 绑定（无消费者时按规则 1 删除）
└── adapters/
    ├── mtk-host-memory/-> 宿主连续内存切片安全检查底座
    └── blender/        -> Blender C++ 连续内存直写适配器 (libmtk_blender_dna)
```

---

## 4. 文档索引

- **系统架构设计**：[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- **公共 API 参考**：[`docs/API_REFERENCE.md`](docs/API_REFERENCE.md)
- **跨语言绑定设计**：[`docs/BINDINGS_DESIGN.md`](docs/BINDINGS_DESIGN.md)
- **API 暴露面审计**：[`docs/API_SURFACE_AUDIT.md`](docs/API_SURFACE_AUDIT.md)
- **路线图**：[`TODO.md`](./TODO.md)
