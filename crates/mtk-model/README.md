# mtk-model

[![Crate](https://img.shields.io/badge/crate-mtk--model-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Rayon](https://img.shields.io/badge/Rayon-Parallel-red.svg)](https://github.com/rayon-rs/rayon)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-model`** 是 `libmozitoolkit` (`libmtk`) 体系中的无头方块状态机 (BlockState) 解析器、1.21+ Block Model JSON 递归继承展开与几何烘焙核心引擎。纯 Rust 实现，严格遵循 **Host-Agnostic（宿主无关）** 原则，为上层体素网格化 (`mtk-voxel`)、端到端预编译 (`libmtk`) 与 DCC 宿主提供精确的原版模型烘焙、变体多条件判定 (Variants/Multipart)、实体方块参数化回退以及 Wavefront OBJ 加载能力。

---

## 目录 (Table of Contents)

- [1. 架构定位与职责边界 (Architecture & Responsibilities)](#1-架构定位与职责边界-architecture--responsibilities)
- [2. 核心数据结构与枚举 (Core Structures & Enums)](#2-核心数据结构与枚举-core-structures--enums)
  - [2.1 BlockState 状态表示 (`parser/blockstate/state.rs`)](#21-blockstate-状态表示-parserblockstatestaters)
  - [2.2 状态机定义与变体匹配 (`parser/blockstate/definition.rs`)](#22-状态机定义与变体匹配-parserblockstatedefinitionrs)
  - [2.3 原版 Model JSON 结构 (`parser/model_json.rs`)](#23-原版-model-json-结构-parsermodel_jsonrs)
  - [2.4 烘焙几何结构体 (`baker/baked_model/`)](#24-烘焙几何结构体-bakerbaked_model)
  - [2.5 烘焙模型数据库 (`baker/baked_model/database.rs`)](#25-烘焙模型数据库-bakerbaked_modeldatabasers)
  - [2.6 预烘焙模型变体组 (`baker/baked_model/variant_group.rs`)](#26-预烘焙模型变体组-bakerbaked_modelvariant_grouprs)
- [3. 核心公共 API 清单 (Public APIs)](#3-核心公共-api-清单-public-apis)
  - [3.1 模型烘焙与状态机工具](#31-模型烘焙与状态机工具)
- [4. 快速上手示例 (Quick Start)](#4-快速上手示例-quick-start)
  - [示例 1：解析 BlockState 字符串并规范化](#示例-1解析-blockstate-字符串并规范化)
  - [示例 2：使用 `ModelBaker` 烘焙楼梯方块几何模型](#示例-2使用-modelbaker-烘焙楼梯方块几何模型)
  - [示例 3：构建 `BakedModelDatabase` 并利用多级回退智能寻址](#示例-3构建-bakedmodeldatabase-并利用多级回退智能寻址)
  - [示例 4：烘焙内置红石引线并提取发光与染色属性](#示例-4烘焙内置红石引线并提取发光与染色属性)
- [5. Feature 开关与依赖](#5-feature-开关与依赖)
- [6. 开源协议 (License)](#6-开源协议-license)

---

## 1. 架构定位与职责边界 (Architecture & Responsibilities)

`mtk-model` 是 `libmozitoolkit` 体系中方块状态机 (BlockState) 解析、原版 1.21+ 模型 JSON 展开与几何面烘焙的核心引擎，具备以下核心职责与边界：

- **BlockState 状态机与变体解算**：解析带属性方块标识符（如 `minecraft:oak_stairs[facing=east,half=bottom,shape=straight]`），支持原版 `variants` 规则匹配与 `multipart` 组合条件树评估（`OR`, `AND`, 属性正则匹配）。
- **递归模型继承树展开 (Hierarchy Resolution)**：支持多达 32 层的父模型继承（`parent`），合并子父级纹理字典，精准展开 `#texture` 变量引用并将纹理绑定写入三维要素 (Element)。
- **混合式实体模型架构 (Hybrid Parametric & Vanilla Model Architecture)**：
  - **普通方块 (Vanilla-First Pipeline)**：100% 遵循原版 BlockState 状态机与 BlockModel JSON 规范，无头烘焙红石线缆、连接墙、楼梯以及各类第三方材质包自定义模型；
  - **实体方块 (BER Parametric Fallback Layer)**：针对 Java 原版通过 `BlockEntityRenderer` 动态绘制且在 assets 中缺失 elements 的实体方块（箱子、床、潜影盒、头颅、告示牌、悬挂告示牌、末地传送门等），内置轻量 MiEx 表达式参数化模型求值器与 Minecraft 实体 Box UV 计算内核，优雅消解状态组合爆炸与手调 UV 倒置问题；
  - **外部材质包优先**：若用户材质包中提供了包含真实 elements 的实体方块模型，管线自动优先消费外部规范 JSON，实现与资源包生态的完美兼容。

- **紧凑状态烘焙与防膨胀架构 (Compact State Enumeration & Anti-Bloat)**：
  - 摒弃盲目的多部件笛卡尔积组合暴增，严格剔除世界运行时非几何属性（如 `waterlogged`, `distance`, `persistent`, `occupied`）；
  - `BakedModel` 对 `culled_faces` 与 `unculled_faces` 采用序列化剥离（`#[serde(skip)]`）与加载后延迟重建机制，使 `models.bin` 缓存体积缩减 60%~80%；
  - 多级智能模型数据库支持动态三级兼容度打分与属性清洗回退。

---

## 2. 核心数据结构与枚举 (Core Structures & Enums)

### 2.1 BlockState 状态表示 (`parser/blockstate/state.rs`)
```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockState {
    pub namespace: String,
    pub name: String,
    pub properties: BTreeMap<String, String>,
}
```
- `BlockState::parse(s: &str) -> Result<Self, ModelError>`: 解析方块状态字符串。
- `state.block_id() -> String`: 返回 `namespace:name`（无属性）。
- `state.to_canonical_string() -> String`: 返回属性确定性升序排列的标准字符串。
- `state.matches_properties(req_props) -> bool`: 检查是否包含指定的属性子集。

---

### 2.2 状态机定义与变体匹配 (`parser/blockstate/definition.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantMatch {
    pub model_id: String,
    pub rot_x: f32,
    pub rot_y: f32,
    pub uvlock: bool,
    pub weight: u32,
    pub variant_props: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockStateDefinition {
    pub variants: Option<HashMap<String, VariantEntry>>,
    pub multipart: Option<Vec<MultipartRule>>,
}
```
- `def.enumerate_all_states(base_id: &str) -> Vec<String>`: 枚举该定义下所有合法的规范状态字符串。
- `BlockStateResolver::resolve(def, state) -> Vec<VariantMatch>`: 评估匹配当前方块状态的所有激活模型变体。

---

### 2.3 原版 Model JSON 结构 (`parser/model_json.rs`)
```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockModelJson {
    pub parent: Option<String>,
    pub ambientocclusion: Option<bool>,
    pub textures: Option<HashMap<String, TextureValue>>,
    pub elements: Option<Vec<ElementJson>>,
}
```
- `model.resolve_hierarchy(model_id, parent_loader) -> Result<ResolvedBlockModel, ModelError>`: 递归解析父模型并展开所有 `#var` 纹理变量。

---

### 2.4 烘焙几何结构体 (`baker/baked_model/`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedFace {
    pub direction: Direction,
    pub texture: String,
    pub uv_rot: f32,
    pub uv_bounds: [f32; 4],
    pub tint_index: i16,
    pub cullface: Option<Direction>,
    pub vertices: [Vec3; 4],
    pub uvs: [Vec2; 4],
    pub normal: Vec3,
    pub atlas_uvs: Option<[Vec2; 4]>,
    pub atlas_chunk_id: Option<u16>,
    pub atlas_texture_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedModel {
    pub block_state: String,
    pub elements: Vec<BakedElement>,
    pub obj_faces: Vec<BakedObjFace>,
    pub faces: [BakedFace; 6],
    pub is_cube: bool,
    pub is_opaque: bool,
    pub is_emissive: bool,
    pub cull_meta: Option<mtk_cull::BlockCullMeta>,
    pub culled_faces: [Vec<BakedFace>; 6],
    pub unculled_faces: Vec<BakedFace>,
}
```
- `baked.deduplicate_faces() -> usize`: 消除自身内部同向叠面（含 `_emissive` 伴生发光层叠面消除）与反向接触面，并重构 6 向分桶。
- `baked.to_mesh_with_options(opts: &ModelMeshOptions) -> MeshData`: 带叠面消除与隐藏体裁剪导出为 `mtk_core::MeshData`。
- `face.remap_to_atlas_bounds(bounds, chunk_id, texture_id)`: 离线注入图集 UV 坐标。

---

### 2.5 烘焙模型数据库 (`baker/baked_model/database.rs`)
```rust
#[derive(Debug, Clone, Default)]
pub struct BakedModelDatabase {
    pub models: HashMap<String, BakedModel>,
    pub variant_groups: HashMap<String, BakedVariantGroup>,
}
```
- `db.get(state: &str) -> Option<&BakedModel>`: 多级智能回退寻址查询单体模型。
- `db.get_variant_group(state: &str) -> Option<&BakedVariantGroup>`: 解析 BlockState 对应的预烘焙离散变体模型组（支持 1:1 随机旋转采样）。
- `db.get_with_pos(state: &str, x: i32, y: i32, z: i32) -> Option<&BakedModel>`: 基于三维世界坐标确定性采样变体模型。
- `db.deduplicate_all() -> usize`: 批量对库中所有模型及变体组执行面消重。
- `db.remap_to_atlas_with(lookup_fn)`: 离线对库中所有模型及变体组注入图集坐标。
- `db.to_binary() -> Result<Vec<u8>, ModelError>`: 序列化为紧凑 Postcard 二进制，采用单变体去重并透明应用 zstd 压缩。
- `BakedModelDatabase::from_binary(bytes) -> Result<Self, ModelError>`: 从 zstd 压缩的 Postcard 二进制流反序列化；未压缩或旧 Bincode 缓存会被明确拒绝，需重建资产缓存。

---

### 2.6 预烘焙模型变体组 (`baker/baked_model/variant_group.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakedVariantGroup {
    pub base_state: String,
    pub models: Vec<BakedModel>,
    pub weights: Vec<u32>,
    pub total_weight: u32,
}
```
- `BakedVariantGroup::new(base_state, models, weights) -> Self`: 创建带权重的离散多变体模型组。
- `BakedVariantGroup::single(base_state, model) -> Self`: 创建单模型平凡变体组（权重 = 1）。
- `group.select_by_pos(x: i32, y: i32, z: i32) -> &BakedModel`: 1:1 对齐 Java 版 `Mth.getSeed` + `JavaRandom` 空间坐标确定性采样变体。
- `group.select_index_by_pos(x: i32, y: i32, z: i32) -> usize`: 空间坐标采样变体索引。
- `group.select_by_seed(seed: i64) -> &BakedModel`: 依据 64 位整型种子采样变体。
- `group.select_index_by_seed(seed: i64) -> usize`: 种子采样变体索引。
- `group.select_primary() -> &BakedModel`: 获取最高权重/默认基准模型（当关闭 Alternate Blocks 时回退）。

---

## 3. 核心公共 API 清单 (Public APIs)

### 3.1 模型烘焙与状态机工具
| API 签名 | 描述 |
| :--- | :--- |
| `BlockState::parse(s: &str) -> Result<BlockState, ModelError>` | 解析方块状态字符串。 |
| `BlockStateResolver::resolve(def, state) -> Vec<VariantMatch>` | 评估匹配方块状态对应的模型变体。 |
| `BlockStateResolver::resolve_variants(def, state) -> Vec<(Vec<VariantMatch>, u32)>` | 提取离散变体分支及其相对权重（用于 Alternate Blocks 预烘焙）。 |
| `get_builtin_blockstate_def(name) -> Option<BlockStateDefinition>` | 获取内置原版 Java BER 实体方块 BlockState 定义（箱子、潜影盒、钟、头颅、饰纹陶罐、末地传送门等无 JSON 几何之方块）。 |
| `get_builtin_model_by_id(model_id) -> Option<BlockModelJson>` | 获取内置原版标准 Blockbench 兼容模型 JSON（箱子各形态、潜影盒、钟各悬挂态、头颅/龙首/猪灵首、陶罐、传送门）。 |
| `get_builtin_model_for_state(blockstate) -> Option<BlockModelJson>` | 依据方块状态获取对应的内置实体回退模型并自动映射材质。 |
| `ModelBaker::new() -> Self` | 创建通用模型烘焙器。 |
| `baker.bake_blockstate(state_str, def, model_loader) -> Result<BakedModel, ModelError>` | 端到端烘焙指定方块状态为 `BakedModel`。 |
| `baker.bake_blockstate_variants(state_str, def, model_loader) -> Result<BakedVariantGroup, ModelError>` | 端到端烘焙离散变体分支组为 `BakedVariantGroup`。 |
| `is_block_emissive(state: &BlockState) -> bool` | 判断方块是否为自发光方块。 |
| `WavefrontObjParser::parse_str(text, filter) -> Vec<ObjRawFace>` | 解析通用 Wavefront OBJ 文本。 |

---

## 4. 快速上手示例 (Quick Start)

### 示例 1：解析 BlockState 字符串并规范化
```rust
use mtk_model::BlockState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = BlockState::parse("minecraft:oak_stairs[shape=straight,facing=east,half=bottom]")?;
    assert_eq!(state.namespace, "minecraft");
    assert_eq!(state.name, "oak_stairs");
    assert_eq!(state.properties.get("facing").unwrap(), "east");

    // 输出确定性升序规范键
    println!("Canonical string: {}", state.to_canonical_string());
    // -> "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]"

    Ok(())
}
```

### 示例 2：使用 `ModelBaker` 烘焙楼梯方块几何模型
```rust
use mtk_model::{BlockModelJson, BlockStateDefinition, ModelBaker};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut baker = ModelBaker::new();

    let blockstate_json = r#"{
        "variants": {
            "facing=east,half=bottom,shape=straight": {
                "model": "minecraft:block/oak_stairs",
                "y": 90,
                "uvlock": true
            }
        }
    }"#;

    let model_json = r#"{
        "textures": {
            "bottom": "minecraft:block/oak_planks",
            "top": "minecraft:block/oak_planks",
            "side": "minecraft:block/oak_planks"
        },
        "elements": [
            {
                "from": [0, 0, 0],
                "to": [16, 8, 16],
                "faces": {
                    "down":  { "texture": "#bottom", "cullface": "down" },
                    "up":    { "texture": "#top" },
                    "north": { "texture": "#side", "cullface": "north" },
                    "south": { "texture": "#side", "cullface": "south" },
                    "west":  { "texture": "#side", "cullface": "west" },
                    "east":  { "texture": "#side", "cullface": "east" }
                }
            },
            {
                "from": [8, 8, 0],
                "to": [16, 16, 16],
                "faces": {
                    "down":  { "texture": "#bottom" },
                    "up":    { "texture": "#top", "cullface": "up" },
                    "north": { "texture": "#side", "cullface": "north" },
                    "south": { "texture": "#side", "cullface": "south" },
                    "west":  { "texture": "#side" },
                    "east":  { "texture": "#side", "cullface": "east" }
                }
            }
        ]
    }"#;

    let def: BlockStateDefinition = serde_json::from_str(blockstate_json)?;
    let model: BlockModelJson = serde_json::from_str(model_json)?;

    let baked = baker.bake_blockstate(
        "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]",
        Some(&def),
        |_model_id| Some(model.clone()),
    )?;

    println!("Baked model elements: {}", baked.elements.len());
    println!("Culled faces count: {:?}", baked.culled_faces.iter().map(|v| v.len()).collect::<Vec<_>>());
    println!("Unculled faces count: {}", baked.unculled_faces.len());

    Ok(())
}
```

### 示例 3：构建 `BakedModelDatabase` 并利用多级回退智能寻址
```rust
use mtk_model::{BakedModel, BakedModelDatabase};

fn main() {
    let mut db = BakedModelDatabase::new();

    let mut model = BakedModel::default();
    model.block_state = "minecraft:chest[facing=north]".to_string();
    db.insert("minecraft:chest[facing=north]".to_string(), model);

    // 1. 精确查询
    assert!(db.get("minecraft:chest[facing=north]").is_some());

    // 2. 剥离运行时非几何属性 waterlogged=true 查询成功
    assert!(db.get("minecraft:chest[facing=north,waterlogged=true]").is_some());

    // 3. 批量消重
    let removed = db.deduplicate_all();
    println!("Removed {} overlapping faces from database", removed);
}
```

### 示例 4：烘焙内置红石引线并提取发光与染色属性
```rust
use mtk_model::baker::ModelBaker;
use mtk_core::attribute_constants::{ATTR_BIOME_TINT_COLOR, ATTR_UV_TRANSFORM};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut baker = ModelBaker::new();

    // 烘焙带信号强度的红石引线（自动使用内置 1.21+ fallback 模型与状态机）
    let baked = baker.bake_blockstate(
        "minecraft:redstone_wire[power=15,north=up,south=side]",
        None,
        |_| None,
    )?;

    assert!(baked.is_emissive);

    let (mesh, _) = baked.to_mesh_with_textures(false);
    assert!(mesh.has_custom_attribute(ATTR_UV_TRANSFORM));
    assert!(mesh.has_custom_attribute(ATTR_BIOME_TINT_COLOR));

    Ok(())
}
```

---

## 5. Feature 开关与依赖

`mtk-model` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `parallel` | 是 | 启用 `rayon` 多线程并发加速（如批量方块状态展开与模型烘焙）。 |

---

## 6. 开源协议与第三方署名 (License & Attribution)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。

### 第三方代码署名 (Third-Party Notices)
本 Crate 包含部分派生与移植自 [MiEx](https://github.com/BramStout/MiEx) (Copyright (c) 2023 Bram Stout) 的模型表达式求值、OBJ 解析与内置模型结构代码，遵循 **BSD 3-Clause License**。
BSD 3-Clause 许可证全文请参阅仓库 [`assets/builtins/LICENSE-MIEX`](../../assets/builtins/LICENSE-MIEX) 与根目录 [`NOTICE`](../../NOTICE)。根据开源许可规范，BSD-3 与 GPL-3 保持完全兼容，在此明确注明原始作者版权归属。
