# mtk-testkit

**Dev-only** test asset resolver and vendored fixtures for the `libmozitoolkit` workspace. This crate is `publish = false` and is never shipped with any binding.

---

## 1. 定位 (Purpose)

真实 Minecraft 资源包 / 存档体积大且受版权约束，**绝不入库**。`mtk-testkit` 为所有集成测试提供统一的资产解析链：

| 优先级 | 来源 | 说明 |
| :---: | :--- | :--- |
| 1 | 环境变量 | CI 从临时 Release 下载真包后设置（`MTK_TEST_ASSETS` 等） |
| 2 | 工作区相邻目录 | `../mc`、`../26.2-Fabric.jar`、`../SPBR-21.zip` 等本地约定 |
| 3 | 内置 fixtures | `fixtures/mc` 下的最小 JSON 子集，保证测试在纯仓库环境下可运行 |

- 需要**完整原版资产**的测试（如 canonical debug world）应查询 [`real_assets_root`] 并在缺失时优雅跳过。
- 仅依赖少量模型 JSON 的**密闭测试**可直接使用 [`assets_root`]（自动回退到 fixtures）。

---

## 2. 公共 API

```rust
use mtk_testkit::{assets_root, real_assets_root, fabric_jar, save_world};
use mtk_testkit::{model_json_path, blockstate_json_path, require_assets};

let root = require_assets!();                       // 无资产则打印提示并 return
let loader = |id: &str| std::fs::read_to_string(model_json_path(&root, id)).ok();
```

| 函数 | 返回值 | 用途 |
| :--- | :--- | :--- |
| `workspace_root()` | `PathBuf` | 工作区根目录 |
| `fixtures_root()` | `PathBuf` | 内置 fixtures 根目录 |
| `assets_root()` | `Option<PathBuf>` | 真实资产或 fixtures（总是尽量可用） |
| `real_assets_root()` | `Option<PathBuf>` | 仅完整原版资产 |
| `has_real_assets()` | `bool` | 是否存在完整原版资产 |
| `fabric_jar()` | `Option<PathBuf>` | 客户端/整合包 jar |
| `resource_pack_zip()` | `Option<PathBuf>` | 外部资源包 zip |
| `save_world()` | `Option<PathBuf>` | 存档目录（含 `level.dat`） |
| `models_cache_bin()` | `Option<PathBuf>` | 预编译 `models.bin` |
| `model_json_path(root, id)` | `PathBuf` | `minecraft:block/cube_all` → `.../models/block/cube_all.json` |
| `blockstate_json_path(root, id)` | `PathBuf` | 去除属性谓词并定位 blockstate JSON |
| `read_asset(rel)` / `read_fixture(rel)` | `Option<String>` | 相对于资产根 / fixtures 根读取 UTF-8 |
| `require_assets!()` | `PathBuf` | 无资产时提前 `return` 的便捷宏 |
| `is_online_enabled()` | `bool` | 是否显式启用了外网在线测试 |
| `require_online!()` | `()` | 未显式开启 `MTK_TEST_ONLINE=1` 时提前 `return` |

### 环境变量

| 变量 | 用途 |
| :--- | :--- |
| `MTK_TEST_ASSETS` / `MC_ASSETS_DIR` / `MC_DIR` | 解包后的 Minecraft jar 根目录（含 `assets/minecraft`） |
| `MTK_TEST_JAR` / `MC_JAR` | 客户端 jar |
| `MTK_TEST_RESOURCE_PACK` | 外部资源包 `.zip` |
| `MTK_TEST_SAVE` | 存档目录 |
| `MTK_TEST_MODELS_CACHE` | 预编译模型缓存 `models.bin` |
| `MTK_TEST_ONLINE` / `MTK_ONLINE_TESTS` | 显式开启真实 HTTP 联网测试（如 Mojang 皮肤拉取） |

---

## 3. Fixtures 约定

`fixtures/mc/assets/minecraft/{blockstates,models}` 仅收纳**测试断言所必需**的最小 JSON 依赖闭包（含父模型继承链），例如 `scaffolding` / `spawner` / `vault` / `heavy_core`。

新增依赖原版模型的测试时，请：
1. 用脚本解析 `parent` 继承闭包；
2. 只复制闭包内文件到 `fixtures/mc/...`；
3. 在测试中用 `mtk_testkit::*_json_path` 定位，严禁硬编码绝对路径。

---

## 4. 构建与测试

```bash
cargo test -p mtk-testkit
```

无外部资产时，`mtk-testkit` 自身不执行真实磁盘 I/O 断言，仅验证路径拼接逻辑。
