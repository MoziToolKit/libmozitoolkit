# mtk-cli (`mtk`)

[![Crate](https://img.shields.io/badge/crate-mtk--cli-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![CLI](https://img.shields.io/badge/CLI-Clap%20v4-blue.svg)](https://docs.rs/clap)
[![Status](https://img.shields.io/badge/Roadmap%20Priority-P1%20Tooling-yellow.svg)]()
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-cli`** 是 MoziToolKit (`libmtk`) 的独立跨平台统一命令行工具（二进制名称为 `mtk`），专为 Minecraft 3D 资产无头预编译、CTM 纹理规则求解、模型烘焙排查、OBJ 导出与面遮挡状态机验证而设计。

---

## 目录 (Table of Contents)

- [1. 架构定位与演进优先级](#1-架构定位与演进优先级)
- [2. 命令行子命令与参数详解](#2-命令行子命令与参数详解)
  - [2.1 `mtk atlas` (图集与资产烘焙)](#21-mtk-atlas-图集与资产烘焙)
  - [2.2 `mtk model` (模型烘焙与数据导出)](#22-mtk-model-模型烘焙与数据导出)
  - [2.3 `mtk export` (Wavefront OBJ / MTL 导出)](#23-mtk-export-wavefront-obj--mtl-导出)
  - [2.4 `mtk ctm` (OptiFine / Continuity 连接纹理测试)](#24-mtk-ctm-optifine--continuity-连接纹理测试)
  - [2.5 `mtk verify` (规则校验)](#25-mtk-verify-规则校验)
  - [2.6 `mtk bench` (性能基准测试)](#26-mtk-bench-性能基准测试)
- [3. 构建与安装指南](#3-构建与安装指南)
- [4. 终端实战命令示例](#4-终端实战命令示例)
- [5. 开源协议 (License)](#5-开源协议-license)

---

## 1. 架构定位与演进优先级

- **演进优先级**：`P1`（**核心工具链与无头资产验证**）。
- **核心定位**：
  1. **离线资产预编译与图集装箱**：在无 Blender 或图形界面环境下，极速解析 Minecraft 原版 JAR 与第三方面资源包（如 SPBR、LabPBR），输出 `atlas_mapping.json`、`biome_mapping.json` 与 `<fingerprint>.mtkcache` 单一缓存包。
  2. **模型烘焙与几何排查**：验证 1.21+ BlockState 状态机与复杂模型几何结构，一键导出为标准 Wavefront OBJ/MTL 文件。
  3. **自动化测试与基准校验**：批量执行 CTM 连接纹理求解测试、面遮挡剔除测试用例校验以及模型烘焙吞吐量压测。

---

## 2. 命令行子命令与参数详解

```
mtk <COMMAND>

Commands:
  atlas    Texture atlas baking and sprite address map generation
  model    Model baking inspection and batch data serialization
  export   Export baked models and textures to Wavefront OBJ / MTL
  ctm      Continuity and OptiFine Connected Textures (CTM) rule parsing and atlas testing
  verify   Verification utilities (e.g. face culling rules against reference test cases)
  bench    Benchmarks: real jar model baking, and the real world-meshing pipeline
  help     Print this message or the help of the given subcommand(s)
```

---

### 2.1 `mtk atlas` (图集与资产烘焙)

| 子命令 | 参数 | 说明 |
| :--- | :--- | :--- |
| `bake-vanilla` | `--mc-dir <DIR>` (默认 `mc`)<br>`--max-size <SIZE>` (默认 4096) | 扫描指定 Minecraft assets 目录并烘焙原版方块基础图集 |
| `bake-all` | `-j, --jar <JAR>` (基础原版 JAR)<br>`-p, --pack <ZIP>` (可选材质包)<br>`-o, --output <DIR>` (输出目录)<br>`--max-size <SIZE>` | 烘焙包含 14 大类 Minecraft 资产的静态与动态双图集及映射 JSON |

---

### 2.2 `mtk model` (模型烘焙与数据导出)

| 子命令 | 参数 | 说明 |
| :--- | :--- | :--- |
| `dump` | `<ASSETS>` (JAR 或资源目录)<br>`<STATE>` (例如 `"minecraft:furnace[facing=north,lit=false]"`)<br>`-o, --output <FILE>` (可选输出路径，默认 stdout)<br>`-p, --pretty` (格式化 JSON 输出) | 烘焙单个方块状态，输出包含顶点数、面数、透明度、发光及贴图槽位的 JSON 数据 |
| `dump-batch` | `<ASSETS>`<br>`-i, --input <JSON_FILE>` (方块状态数组文件)<br>`-o, --output <JSON_FILE>` | 批量烘焙指定列表中的所有方块状态并输出聚合结果 |

---

### 2.3 `mtk export` (Wavefront OBJ / MTL 导出)

| 子命令 | 参数 | 说明 |
| :--- | :--- | :--- |
| `samples` | `-j, --jar <JAR>`<br>`-o, --output <DIR>` (默认 `manual_test/samples`) | 批量导出台阶、楼梯、墙壁、栅栏、铁栏杆、灯笼等代表性方块模型为 `.obj` 与 `.mtl` |
| `builtin-objs` | `-a, --assets <ASSETS>`<br>`-o, --output <DIR>` (默认 `manual_test/models`)<br>`-t, --textures-dir <DIR>` (默认 `manual_test/textures`) | 导出箱子、潜影盒、床、旗帜、钟、头颅等原版硬编码特殊实体方块网格与提取贴图 |

---

### 2.4 `mtk ctm` (OptiFine / Continuity 连接纹理测试)

| 参数 | 说明 |
| :--- | :--- |
| `-p, --packs-dir <DIR>` (默认 `.`) | 扫描包含 CTM / Continuity 资源包的目录 |
| `-j, --jar <JAR>` (默认 `26.2-Fabric.jar`) | 基础原版 JAR 文件路径 |

---

### 2.5 `mtk verify` (规则校验)

| 子命令 | 参数 | 说明 |
| :--- | :--- | :--- |
| `cull` | `-i, --input <FILE>` (测试用例 JSON，未提供则读取 stdin)<br>`-o, --output <FILE>` (输出文件，未提供则写入 stdout)<br>`--leaves <MODE>` (`SINGLE_FACE`, `FANCY`, `FAST`, `NONE`)<br>`--glass <MODE>` (`GROUP`, `SAME_BLOCK`, `NONE`) | 读取方块邻接测试用例，校验面遮挡状态机判定结果的一致性 |

---

### 2.6 `mtk bench` (性能基准测试)

| 子命令 | 参数 | 说明 |
| :--- | :--- | :--- |
| `model` | `-j, --jar <JAR>` (原版 JAR 文件) | 扫描并对全量原版 BlockState 进行多线程并发模型烘焙压测，测算 TPS 与耗时 |
| `world` | `--cache <MTKCACHE>`<br>`--save <DIR>`<br>`--dimension <ID>` (默认 `overworld`)<br>`--bounds <MIN_MAX>` (默认 `-64,-64,-64,512,320,512`)<br>`--iterations <N>` (默认 3)<br>`--threads <N>`<br>`--no-ctm`<br>`--json` | **真实场景基准**：打开真实 `.mtkcache`，从真实存档加载选区，跑完整管线（`extract_padded` → 模型/图集/CTM/biome 网格化 → weld 合并），逐阶段输出毫秒与 quads/s |

`bench world` 拒绝在"没有模型库"的缓存上运行（那测的是回退立方体路径，不是用户看到的世界）。
同一份缓存 + 存档 + bounds 才能比较两次结果；`--no-ctm` 用于把 CTM 的逐面成本单独量出来。
宿主侧（Blender 注入 + 材质绑定）用 `tools/blender/scenario_bench.py`。

---

## 3. 构建与安装指南

### 3.1 本地编译
```bash
# 编译 Release 二进制 (产物位于 target/release/mtk)
cargo build --release -p mtk-cli
```

### 3.2 安装至本地 Cargo 二进制路径
```bash
cargo install --path crates/mtk-cli
```

---

## 4. 终端实战命令示例

### 示例 1：全量预编译原版与第三方 PBR 材质包
```bash
mtk atlas bake-all \
    --jar 26.2-Fabric.jar \
    --pack SPBR-21.zip \
    --output ./baked_assets/atlases \
    --max-size 4096
```

### 示例 2：检查单个方块状态的模型烘焙几何
```bash
mtk model dump \
    26.2-Fabric.jar \
    "minecraft:oak_stairs[facing=east,half=top,shape=inner_left]" \
    --pretty
```

### 示例 3：导出代表性方块 OBJ 用于 DCC 视觉审查
```bash
mtk export samples \
    --jar 26.2-Fabric.jar \
    --output ./debug_models
```

### 示例 4：验证面遮挡状态机与测试用例
```bash
mtk verify cull \
    --input tests/cases/cull_cases.json \
    --leaves SINGLE_FACE \
    --glass GROUP
```

### 示例 5：运行模型烘焙吞吐量压测
```bash
mtk bench model --jar 26.2-Fabric.jar
```

### 示例 6：真实场景体素管线基准（缓存 + 存档）
```bash
cargo build --release -p mtk-cli
./target/release/mtk bench world \
    --cache ../MoziToolKit/.ci-cache/mtk_fp_58d0c7fee81ad3bf.mtkcache \
    --save ../mtk-save-world \
    --bounds -64,-64,-64,512,320,512 \
    --iterations 3
```

---

## 5. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
