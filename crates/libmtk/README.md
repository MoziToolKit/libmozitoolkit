# libmtk

[![Crate](https://img.shields.io/badge/crate-libmtk-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Rayon](https://img.shields.io/badge/Rayon-Parallel-red.svg)](https://github.com/rayon-rs/rayon)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`libmtk`** 是 libmozitoolkit 的**应用编排层（Application Layer）**，只负责两件事：端到端资产预编译（`prebake`）与统一网格处理管线（`pipeline`）。纯 Rust 实现，严格遵循 **Host-Agnostic（宿主无关）** 架构设计。

> **边界约定（2026-10）**：本 crate **不再重导出**任何领域 crate 的符号。
> `MeshData`、`BakedModel`、`FaceCuller`、`SectionMesher` 等类型请直接从对应的 `mtk-*` crate 引入——
> 绑定层与工具链按"用哪层依赖哪层"直接依赖领域 crate，避免出现第二套入口。

---

## 目录 (Table of Contents)

- [1. 架构拓扑 (Sub-Crates Topology)](#1-架构拓扑-sub-crates-topology)
- [2. 核心特性与顶层功能 (Top-Level Features)](#2-核心特性与顶层功能-top-level-features)
  - [2.1 端到端无头资产预编译 (`libmtk::prebake`)](#21-端到端无头资产预编译-libmtkprebake)
  - [2.2 高级网格处理管线 (`libmtk::pipeline`)](#22-高级网格处理管线-libmtkpipeline)
  - [2.3 统一错误系统 (`MtkError`)](#23-统一错误系统-mtkerror)
- [3. 核心 API 清单 (Core API Reference)](#3-核心-api-清单-core-api-reference)
- [4. 快速上手示例 (Quick Start)](#4-快速上手示例-quick-start)
  - [示例 1：执行全量资源包预编译](#示例-1执行全量资源包预编译)
  - [示例 2：高级网格处理流水线](#示例-2高级网格处理流水线)
- [5. Feature 开关与依赖](#5-feature-开关与依赖)
- [6. 开源协议 (License)](#6-开源协议-license)

---

## 1. 架构拓扑 (Sub-Crates Topology)

`libmtk` 依赖以下领域 Crate 来完成编排，但**不对外重导出**它们：

| 依赖 Crate | 核心职责 |
| :--- | :--- | :--- |
| [`mtk-core`](../mtk-core/) | 基础几何基元、连续网格缓冲 (`MeshData`)、6 向拓扑与自适应像素细分/智能挤出算子。 |
| [`mtk-cull`](../mtk-cull/) | 原版 1.21+ 邻域面遮挡判定器 (`FaceCuller`)、2D 矩形差集切分与外部网格叠面剔除 (`cull_mesh_faces`)。 |
| [`mtk-model`](../mtk-model/) | BlockState 解析、1.21+ Block Model JSON 递归继承展开、模型烘焙数据库 (`BakedModelDatabase`)。 |
| [`mtk-voxel`](../mtk-voxel/) | 16x16x16 Section 体素存储、平滑 AO 计算、5x5 生物群系平滑核与无头区块网格化器 (`SectionMesher`)。 |
| [`mtk-resource`](../mtk-resource/) | 虚拟文件系统 (VFS, `ResourcePackStack`)、.mcmeta 动图解析与 CTM 17/47 连接纹理求解器。 |
| [`mtk-texture`](../mtk-texture/) | 2D 矩形装箱 (`Stitcher`)、多类别 PBR 图集烘焙与 Companion Overlay 贴图合成。 |
| [`mtk-material`](../mtk-material/) | 66 种原版生物群系调色板引擎 (SSOT)、`BiomeResolver` 映射提取与 Rayon 并行 UV 重映射。 |
| [`mtk-sync`](../mtk-sync/) | 原生 WebSocket 实时协同客户端 (`LiveSyncSession`) 与 Yefira 二进制小端序协议编解码器。 |
| [`mtk-save`](../mtk-save/) | 现代 Minecraft 存档加载核心：基于内置零拷贝 NBT、Anvil `.mca` 寻址与按需空间切片流。 |

---

## 2. 核心特性与顶层功能 (Top-Level Features)

### 2.1 端到端无头资产预编译 (`libmtk::prebake`)

一键将离线资源包栈（Vanilla + 材质包）全量烘焙为运行时持久化高速缓存（受 `ASSET_CACHE_FORMAT_VERSION` 版本约束）：
- **多类别图集烘焙**：自动装箱 Blocks、Items、ArmorTrims、Beds 等图集大图，并行编码 PNG 并落盘；
- **伴随贴图与 Overlay 提取**：自动合成 `_overlay.png` 并在图集地址映射表 (`AtlasAddressMap`) 中建立别名对齐；
- **模型图集 UV 预解算**：多线程全量烘焙模型，将图集 UV 坐标与 Chunk/Texture ID 预先注入 `BakedModelDatabase` 并序列化为二进制快照 (`models.bin`)；
- **调色板与生物群系映射**：提取 `biome_mapping.json` 供运行时极速着色。

## 2.3 统一错误系统 (`MtkError`)

使用 `thiserror` 统一封装模型、体素、贴图、资源、材质、I/O 与并发线程池错误。

---

## 3. 核心 API 清单 (Core API Reference)

```rust
// 1. 统一错误类型
pub enum MtkError {
    Model(mtk_model::ModelError),
    Voxel(mtk_voxel::VoxelError),
    Texture(mtk_texture::error::TextureError),
    Resource(mtk_resource::ResourceError),
    Material(mtk_material::MaterialError),
    Io(std::io::Error),
    Json(serde_json::Error),
    Postcard(postcard::Error),
    ThreadPool(String),
}

// 2. 端到端资产预编译
pub fn precompile_all_assets(
    stack: &ResourcePackStack,
    cache_dir: impl AsRef<Path>,
    config: &PrecompileConfig,
) -> Result<PrecompileResult, MtkError>;

pub fn precompile_all_assets_with_progress(
    stack: &ResourcePackStack,
    cache_dir: impl AsRef<Path>,
    config: &PrecompileConfig,
    progress_callback: Option<ProgressCallback>,
) -> Result<PrecompileResult, MtkError>;

pub fn prebake_all_models(
    stack: &ResourcePackStack,
    atlas_map: Option<&AtlasAddressMap>,
) -> Result<BakedModelDatabase, MtkError>;

pub fn prebake_all_models_with_progress(
    stack: &ResourcePackStack,
    atlas_map: Option<&AtlasAddressMap>,
    progress_callback: Option<ProgressCallback>,
) -> Result<BakedModelDatabase, MtkError>;

```

---

### 3.1 缓存包解析

`libmtk::prebake::resolve_active_cache_package(cache_dir, fingerprint)` 提供宿主无关的
`.mtkcache` 选择策略：direct-name 优先、package header fingerprint 校验、mtime 最新回退。
Blender 与 C4D 均应通过该函数定位 active cache。

## 4. 快速上手示例 (Quick Start)

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
libmtk = { version = "0.1.0" }
```

### 示例 1：执行全量资源包预编译

```rust
use std::path::Path;
use libmtk::{precompile_all_assets, CacheManifest, PrecompileConfig};
use mtk_resource::{DirectoryPack, ResourcePackStack};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stack = ResourcePackStack::new();

    // 载入原版资源目录或材质包 ZIP
    stack.push_pack(Box::new(DirectoryPack::new(
        "Vanilla",
        Path::new("/path/to/minecraft/resourcepack"),
    )));

    let output_cache_dir = Path::new("/path/to/cache_dir");

    let config = PrecompileConfig {
        max_atlas_width: 4096,
        max_atlas_height: 4096,
        atlas_category: "all".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: None, // 使用全部可用 CPU 核心
    };

    // 执行预编译流水线
    let result = precompile_all_assets(&stack, output_cache_dir, &config)?;

    println!(
        "Precompilation complete: {} baked models, {} atlas chunks in {:?}",
        result.baked_models,
        result.atlas_chunks,
        output_cache_dir
    );

    // 读取并校验元数据清单
    if let Some(manifest) = CacheManifest::read_from_dir(output_cache_dir) {
        assert!(manifest.is_valid_for(&result.fingerprint));
    }

    Ok(())
}
```
