# mtk-resource

[![Crate](https://img.shields.io/badge/crate-mtk--resource-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Rayon](https://img.shields.io/badge/Rayon-Parallel-red.svg)](https://github.com/rayon-rs/rayon)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-resource`** 是 `libmozitoolkit` (`libmtk`) 体系中的无头资源包虚拟文件系统 (VFS)、原版资产标识符规范、`.png.mcmeta` 动图元数据解析、原版 `atlases/*.json` 数据模型与 CTM 连接纹理规则引擎。遵循 **Host-Agnostic（宿主无关）** 原则，为上层图集装箱 (`mtk-texture`)、模型烘焙 (`mtk-model`) 与预编译管线提供统一的多源资产读取与覆盖层叠抽象。

---

## 目录 (Table of Contents)

- [1. 架构定位与职责边界 (Architecture & Responsibilities)](#1-架构定位与职责边界-architecture--responsibilities)
- [2. 核心数据结构与枚举 (Core Structures & Enums)](#2-核心数据结构与枚举-core-structures--enums)
  - [2.1 标识符系统 (`identifier.rs`)](#21-标识符系统-identifierrs)
  - [2.2 虚拟文件系统与多层资源栈 (`pack/`)](#22-虚拟文件系统与多层资源栈-pack)
  - [2.3 纹理动图元数据 (`meta.rs`)](#23-纹理动图元数据-metars)
  - [2.4 原版图集配置规范 (`atlas.rs`)](#24-原版图集配置规范-atlasrs)
  - [2.5 CTM 规则与解算器 (`ctm/`)](#25-ctm-规则与解算器-ctm)
  - [2.6 玩家皮肤多级解析与哈希索引 (`skin/`)](#26-玩家皮肤多级解析与哈希索引-skin)
- [3. 核心公共 API 清单 (Public APIs)](#3-核心公共-api-清单-public-apis)
  - [3.1 `ResourcePackStack` 常用方法](#31-resourcepackstack-常用方法)
  - [3.2 CTM 解算方法](#32-ctm-解算方法)
- [4. 快速上手示例 (Quick Start)](#4-快速上手示例-quick-start)
  - [示例 1：构建多层资源栈并解算 PBR 伴随贴图](#示例-1构建多层资源栈并解算-pbr-伴随贴图)
  - [示例 2：加载图集定义与收集待烘焙精灵](#示例-2加载图集定义与收集待烘焙精灵)
  - [示例 3：解析 CTM 规则并执行邻域连接解算](#示例-3解析-ctm-规则并执行邻域连接解算)
- [5. Feature 开关与依赖](#5-feature-开关与依赖)
- [6. 开源协议 (License)](#6-开源协议-license)

---

## 1. 架构定位与职责边界 (Architecture & Responsibilities)

`mtk-resource` 是 `libmozitoolkit` 体系中的资产发现与虚拟文件系统（VFS）底座，具备以下核心职责与边界：

- **多源 VFS 抽象**：统一抽象物理目录 (`DirectoryPack`)、内存文件表 (`MemoryPack`) 以及 ZIP/JAR 压缩包 (`ZipPack`)，支持多层资源包自顶向下按优先级覆盖叠加 (`ResourcePackStack`)。
- **细粒度 PBR 伴随贴图解算**：支持按通道独立探测并合并法线贴图 (`_n.png`, `_N.png`)、高光粗糙度贴图 (`_s.png`, `_S.png`)、叠加层 (`_overlay.png`, `grass_block_side_overlay.png`) 与动画元数据 (`.png.mcmeta`)。
- **原版图集配置解析**：完整支持 Minecraft 1.20+ 数据驱动的 `atlases/*.json` 规范（`directory`, `single`, `filter`, `unstitch`, `paletted_permutations`）以及内置默认图集模板。
- **CTM 连接纹理规则引擎**：解析 OptiFine 与 Continuity 规范的 `.properties` 文件，提供 CTM 47、CTM 17、水平/垂直连接、重复模式与加权伪随机变体解算。
- **纯数据无头设计**：零宿主依赖，纯数据输入输出，全面支持 Rayon 多线程与 `serde` 序列化。

---

## 2. 核心数据结构与枚举 (Core Structures & Enums)

### 2.1 标识符系统 (`identifier.rs`)
```rust
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceLocation {
    pub namespace: String,
    pub path: String,
}
```
- `ResourceLocation::new(namespace, path) -> Self`: 构造命名空间与路径。
- `ResourceLocation::vanilla(path) -> Self`: 构造 `minecraft` 默认命名空间资源。
- `ResourceLocation::parse(s: &str) -> Result<Self, ResourceError>`: 解析 `"minecraft:block/stone"` 或 `"block/stone"`。
- `ResourceLocation::parse_texture_path(input: &str) -> Result<Self, ResourceError>`: 容错解析任意物理/相对贴图路径。
- `loc.to_asset_path(category_dir, extension) -> String`: 转换为物理资产相对路径（如 `"assets/minecraft/textures/block/stone.png"`）。
- `ResourceLocation::from_asset_path(path, category_dir, extension) -> Option<Self>`: 从物理资产路径逆向提取标识符。
- `ResourceLocation::model_candidate_asset_paths(input: &str) -> Vec<String>`: 生成模型寻址候选路径列表。

---

### 2.2 虚拟文件系统与多层资源栈 (`pack/`)
```rust
pub trait ResourcePack: Send + Sync {
    fn name(&self) -> &str;
    fn open(&self, relative_path: &str) -> Option<Vec<u8>>;
    fn list_files(&self, prefix: &str) -> Vec<String>;
    fn has_file(&self, relative_path: &str) -> bool;
}

pub struct DirectoryPack { /* ... */ }
pub struct MemoryPack { /* ... */ }
#[cfg(feature = "zip")]
pub struct ZipPack { /* ... */ }

pub struct ResourcePackStack {
    packs: Vec<Box<dyn ResourcePack>>,
}
```
- `PbrCompanions`: 单一纹理对应的复合伴随数据（Albedo, Normal, Specular, Overlay, Emissive, MCMETA）。
- `DiscoveredSprite`: 图集构建前收集的完整待解码精灵图描述。

---

### 2.3 纹理动图元数据 (`meta.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnimationFrame {
    Index(u32),
    Detailed { index: u32, time: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationMetadata {
    pub frametime: u32,
    pub interpolate: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frames: Option<Vec<AnimationFrame>>,
}
```

---

### 2.4 原版图集配置规范 (`atlas.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AtlasSource {
    Directory { source: String, prefix: String },
    Single { resource: ResourceLocation, sprite: Option<ResourceLocation> },
    Filter { pattern: AtlasFilterPattern },
    Unstitch { resource: ResourceLocation, divisor_x: f64, divisor_y: f64, regions: Vec<UnstitchRegion> },
    PalettedPermutations {
        palette_key: ResourceLocation,
        permutations: HashMap<String, ResourceLocation>,
        textures: Vec<ResourceLocation>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AtlasCategory {
    Blocks, Items, Particles, Chests, ShulkerBoxes,
    BannerPatterns, ShieldPatterns, ArmorTrims, DecoratedPot,
    Paintings, Celestials, Gui, MapDecorations, Entities, Misc,
    Custom(ResourceLocation),
}
```

---

### 2.5 CTM 规则与解算器 (`ctm/`)
```rust
pub enum CtmMethod {
    Full { inner_seams: bool },
    Compact { inner_seams: bool },
    Horizontal,
    Vertical,
    HorizontalVertical,
    VerticalHorizontal,
    Top,
    Repeat { width: u32, height: u32 },
    Random { weights: Vec<f32>, total_weight: f32, symmetry: CtmSymmetry, linked: bool },
    Fixed,
    Overlay,
}

pub struct CtmRule {
    pub name: String,
    pub priority: i32,
    pub method: CtmMethod,
    pub match_blocks: Vec<BlockMatch>,
    pub match_tiles: Vec<ResourceLocation>,
    pub tiles: Vec<Option<ResourceLocation>>,
    pub connect_logic: ConnectLogic,
    /* ... */
}

pub struct CtmSolver {
    pub rules: Vec<CtmRule>,
    /* ... */
}
```

---

### 2.6 玩家皮肤多级解析与哈希索引 (`skin/`)
- **内容哈希唯一命名规约**：所有皮肤文件物理存储严格统一为 `<sha256>.png`，彻底消除按用户名与 UUID 重复存储同份图像的冗余文件。
- **0 秒极速本地索引 (`index.json` / `SkinIndex`)**：
  - 维护内存与磁盘高速映射表，支持 `UUID <-> 用户名 <-> 64 位 SHA256 哈希 <-> 原始 URL` 毫秒级双向检索；
  - 无论 Live Sync 或存档导入传入的是离线 UUID、带/不带前缀的 UUID、还是玩家名，均能 0 秒极速直接命中本地缓存，首帧免等待；
  - 自动发现并无缝迁移历史残留的 `player_*.png` 与 `uuid_*.png` 遗留文件至规范哈希文件。
- **在线皮肤验证与静默增量同步**：
  - 在保持首帧 0 秒呈现本地缓存的同时，根据可配置 TTL（默认 1 小时）在后台守护线程异步校验 Mojang / CDN 最新皮肤指纹；
  - 当检测到玩家在线更新了皮肤时，自动拉取新哈希图像、更新 `index.json` 并派发热更新事件驱动网格重建。
- **第三方 CDN 镜像站优先与防假降级校验 (Mirror-First & Anti-Fake-Fallback)**：
  - 针对纯玩家名或 UUID 查找，优先通过高可用 Cloudflare 边缘缓存镜像（Crafthead / Minotar）单次 HTTP 请求直取 64x64 PNG，有效规避 Mojang 官方 Session Server 的严格速率限制（429）与三跳瀑布流；
  - **严格响应头门禁与假数据防御**：镜像站针对未知或离线 UUID 常返回 HTTP 200 及默认 Alex/Steve 回退皮肤。模块内严格校验响应标头（Crafthead 的 `x-crafthead-profile-cache-hit != "invalid-profile"` 与 64 位 `x-crafthead-texture-id`、Minotar 的 64 位 `etag`）并执行内容哈希双重比对，坚决拒收镜像伪降级假数据；
  - **离线 UUID 智能感知与用户名优先**：针对 Minecraft 离线/合成 UUID（RFC 4122 Version 3），自动感知其不在 Mojang 官方认证库，若附带玩家名则优先以玩家名向镜像站与官方库发起查询，杜绝无效 HTTP 往返；
  - **历史污染自愈清洗 (Cache Poison Eviction)**：`load_and_migrate` 初始化时自动识别并物理清洗历史错误缓存的镜像兜底 Alex/Steve 文件与映射记录，保障皮肤索引与本地呈现的绝对真实性；
  - 镜像未命中时优雅回退至 Mojang 官方 API；
- **长连接与会话池单例复用 (Keep-Alive Connection Pool)**：
  - 核心 `Arc<ureq::Agent>` 贯穿生命周期单例复用，保留 TCP Keep-Alive 与 TLS 会话缓存，彻底消除每次请求重复握手开销；
- **零 C 依赖安全拉取与代理感知**：
  - 基于 `ureq` + `rustls` 跨平台拉取；
  - 自动识别读取系统环境代理变量（`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY`）；
  - 具备负缓存机制（Negative Caching），针对无效用户或 404 限制重试开销。

---

## 3. 核心公共 API 清单 (Public APIs)

### 3.1 `ResourcePackStack` 常用方法
| 方法签名 | 描述 |
| :--- | :--- |
| `push_pack(pack: Box<dyn ResourcePack>)` | 推入高优先级资源包（栈顶）。 |
| `append_pack(pack: Box<dyn ResourcePack>)` | 追加低优先级兜底资源包（栈底）。 |
| `open_asset_raw(asset_path: &str) -> Option<Vec<u8>>` | 按优先级自顶向下读取原始资产字节。 |
| `open_texture_raw(loc: &ResourceLocation) -> Option<Vec<u8>>` | 读取指定纹理的原始 PNG 字节。 |
| `load_atlas_category(cat: &AtlasCategory) -> AtlasDefinition` | 加载或自动回退至默认类别图集定义。 |
| `resolve_pbr_companions(loc: &ResourceLocation) -> PbrCompanions` | 跨包细粒度聚合 Albedo/Normal/Specular/Overlay/MCMETA。 |
| `collect_sprites_for_atlas(def: &AtlasDefinition) -> Result<Vec<DiscoveredSprite>, ResourceError>` | 展开图集定义并提取所有待缝合精灵数据。 |
| `collect_sprites_including_ctm(def: &AtlasDefinition) -> Result<Vec<DiscoveredSprite>, ResourceError>` | 展开图集定义并自动注入所有关联 CTM 子贴图精灵数据。 |
| `load_ctm_rules() -> Vec<CtmRule>` | 扫描并解析活跃包中的所有 OptiFine/Continuity CTM 规则。 |
| `build_ctm_solver() -> CtmSolver` | 自动从活跃包解析规则构建 CTM 解算器。 |
| `compute_stack_fingerprint() -> String` | 计算当前资源包栈拓扑与顺序的唯一哈希指纹。 |

### 3.2 CTM 解算方法
| 方法签名 | 描述 |
| :--- | :--- |
| `CtmRule::parse_properties(path, ns, content) -> Option<CtmRule>` | 解析 `.properties` 配置文件。 |
| `CtmRule::applies_to_scene(&self, state_palette, needed_textures) -> bool` | 场景剪枝判定：检测规则是否被当前场景引用的方块状态或纹理命中。 |
| `rule.to_json() -> Result<String, serde_json::Error>` | 将 CTM 规则序列化为紧凑 JSON 格式（供 `.mtkscene` 包嵌入）。 |
| `CtmRule::from_json(json: &str) -> Result<Self, serde_json::Error>` | 从 JSON 反序列化 CTM 规则。 |
| `CtmSolver::new(rules: Vec<CtmRule>) -> Self` | 构建带优先级倒序索引的高速 CTM 解算器。 |
| `solver.to_json() -> Result<String, serde_json::Error>` | 将解算器全量规则序列化为 JSON 字符串。 |
| `CtmSolver::from_json(json: &str) -> Result<Self, serde_json::Error>` | 从 JSON 规则字符串现场重建倒序解算器。 |
| `solver.resolve_face(state, face, pos, base_tile, biome, get_block) -> Option<ResourceLocation>` | 根据邻域方块状态与基础贴图解算目标面替换贴图（使用标准静态切线）。 |
| `solver.resolve_face_with_tangents(state, face, tangents, pos, base_tile, biome, get_block) -> Option<ResourceLocation>` | 支持显式传入动态 2D 切线基底 `(up, left)` 的解算接口，自适应感知旋转方块与 UV 旋转。 |

### 3.3 资源包嗅探与原版权威校验 (`probe.rs`)
| 方法签名 | 描述 |
| :--- | :--- |
| `probe_pack(path: &Path) -> PackProbe` | 快速探测资源包物理形态、包格式、`pack.mcmeta` 元数据、原版方块状态/模型/纹理存在性、资产指标统计 (`AssetSummary`) 与匹配评分。 |
| `path_provides_vanilla_client(path: &Path) -> bool` | 严格门禁检测：验证给定目录或 ZIP/JAR 文件是否包含核心原版客户端资产（石头、泥土、橡木木板等核心模型、状态和纹理）。 |

`PackProbe.asset_summary` (`AssetSummary`) 结构提供精细资产统计：
- `has_renderable_assets: bool`：是否包含可渲染资产（方块状态、模型、方块贴图或 CTM 配置）；
- `has_block_assets: bool`：是否包含方块级几何/纹理资产（用于过滤纯代码/功能模组）；
- `blockstate_count`、`block_model_count`、`item_model_count`、`block_texture_count`、`ctm_property_count`；
- `namespaces: Vec<String>`：资源包内涉及的命名空间清单。


---

## 4. 快速上手示例 (Quick Start)

### 示例 1：构建多层资源栈并解算 PBR 伴随贴图
```rust
use mtk_resource::{
    DirectoryPack, MemoryPack, ResourceLocation, ResourcePackStack,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stack = ResourcePackStack::new();

    // 1. 底部载入基础材质包
    let base_pack = DirectoryPack::new("vanilla", "/path/to/minecraft/assets");
    stack.append_pack(Box::new(base_pack));

    // 2. 顶部覆盖高清 LabPBR 扩展包（仅包含 _n.png 与 _s.png）
    let mut pbr_pack = MemoryPack::new("lab_pbr_patch");
    pbr_pack.insert(
        "assets/minecraft/textures/block/diamond_ore_n.png",
        vec![/* normal map bytes */],
    );
    pbr_pack.insert(
        "assets/minecraft/textures/block/diamond_ore_s.png",
        vec![/* specular map bytes */],
    );
    stack.push_pack(Box::new(pbr_pack));

    // 3. 跨层解析 diamond_ore 的复合伴随数据
    let loc = ResourceLocation::parse("minecraft:block/diamond_ore")?;
    let companions = stack.resolve_pbr_companions(&loc);

    assert!(companions.albedo.is_some());   // 来自 base_pack
    assert!(companions.normal.is_some());   // 来自 pbr_pack
    assert!(companions.specular.is_some()); // 来自 pbr_pack

    Ok(())
}
```

### 示例 2：加载图集定义与收集待烘焙精灵
```rust
use mtk_resource::{AtlasCategory, ResourcePackStack};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stack = ResourcePackStack::new();
    // 载入标准 blocks 类别定义（若无 atlases/blocks.json 则回退内置模板）
    let atlas_def = stack.load_atlas_category(&AtlasCategory::Blocks);

    // 展开并提取所有独立精灵及其伴随层
    let sprites = stack.collect_sprites_for_atlas(&atlas_def)?;
    println!("Discovered {} sprites for blocks atlas", sprites.len());

    Ok(())
}
```

### 示例 3：解析 CTM 规则并执行邻域连接解算
```rust
use glam::IVec3;
use mtk_core::Direction;
use mtk_resource::{CtmRule, CtmSolver};

fn main() {
    let properties = r#"
matchBlocks=minecraft:bookshelf
method=horizontal
tiles=0 1 2 3
"#;

    let rule = CtmRule::parse_properties(
        "optifine/ctm/bookshelf.properties",
        "minecraft",
        properties,
    ).expect("Failed to parse CTM rule");

    let solver = CtmSolver::new(vec![rule]);

    // 模拟邻域方块查询闭包
    let get_block = |pos: IVec3| -> Option<&'static str> {
        if pos == IVec3::new(1, 0, 0) {
            Some("minecraft:bookshelf")
        } else {
            None
        }
    };

    let target_tile = solver.resolve_face(
        "minecraft:bookshelf",
        Direction::North,
        IVec3::ZERO,
        None,
        None,
        get_block,
    );

    if let Some(tile) = target_tile {
        println!("Connected CTM sub-tile: {}", tile);
    }
}
```

---

## 5. Feature 开关与依赖

`mtk-resource` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `std` | 是 | 启用标准库与 `mtk-core/std`、`serde/std`、`zip`。 |
| `zip` | 是 | 启用 `zip` crate 支持直接读取 `.zip` / `.jar` 资源包文件。 |
| `parallel` | 是 | 启用 `rayon` 多线程并发加速（如大批量文件扫描与 CTM 规则预加载）。 |

---

## 6. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
