# mtk-material

[![Crate](https://img.shields.io/badge/crate-mtk--material-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Rayon](https://img.shields.io/badge/Rayon-Parallel-red.svg)](https://github.com/rayon-rs/rayon)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-material`** 是 `libmozitoolkit` (`libmtk`) 体系中的材质名称解析、外部别名映射、Mineways 网格图集反混淆、原版规范 66 生物群系调色板与线性色彩数学引擎唯一权威事实源 (SSOT)、以及基于 Rayon 的多线程图集 UV 批量重映射核心。遵循 **Host-Agnostic（宿主无关）** 原则，为上层网格处理、预编译管线与 DCC 宿主提供精确的材质与群系色彩着色属性。

---

## 目录 (Table of Contents)

- [1. 架构定位与职责边界 (Architecture & Responsibilities)](#1-架构定位与职责边界-architecture--responsibilities)
- [2. 核心数据结构与枚举 (Core Structures & Enums)](#2-核心数据结构与枚举-core-structures--enums)
  - [2.1 生物群系与调色板 (`biome/palettes.rs` & `hardcoded.rs`)](#21-生物群系与调色板-biomepalettesrs--hardcodedrs)
  - [2.2 生物群系解析器与网格面属性生成 (`biome/resolver/` & `batch.rs`)](#22-生物群系解析器与网格面属性生成-biomeresolver--batchrs)
  - [2.3 材质解析器与网格图集规范 (`resolver/` & `types.rs`)](#23-材质解析器与网格图集规范-resolver--typesrs)
  - [2.4 多通道 UV 批量重映射结果 (`types.rs`)](#24-多通道-uv-批量重映射结果-typesrs)
- [3. 核心公共 API 清单 (Public APIs)](#3-核心公共-api-清单-public-apis)
  - [3.1 色彩数学与生物群系工具](#31-色彩数学与生物群系工具)
  - [3.2 材质与 UV 重映射](#32-材质与-uv-重映射)
- [4. 快速上手示例 (Quick Start)](#4-快速上手示例-quick-start)
  - [示例 1：查询权威生物群系调色板与色彩空间转换](#示例-1查询权威生物群系调色板与色彩空间转换)
  - [示例 2：Rayon 并行计算网格面生物群系着色属性](#示例-2rayon-并行计算网格面生物群系着色属性)
  - [示例 3：全网格多通道 UV 并行重映射 (Atlas UV + Local UV)](#示例-3全网格多通道-uv-并行重映射-atlas-uv--local-uv)
  - [示例 4：查询红石引线信号强度动态硬编码染色](#示例-4查询红石引线信号强度动态硬编码染色)
- [5. Feature 开关与依赖](#5-feature-开关与依赖)
- [6. 开源协议 (License)](#6-开源协议-license)

---

## 1. 架构定位与职责边界 (Architecture & Responsibilities)

`mtk-material` 是 `libmozitoolkit` 体系中材质解析、UV 空间重映射与生物群系色彩着色的**权威事实源 (Single Source of Truth, SSOT)**，具备以下核心职责与边界：

- **66 原版生物群系调色板唯一权威源 (Biome SSOT)**：内置 66 种 Minecraft 官方生物群系参数表（草方块、树叶、干枯植被、水体颜色、温度、湿度及 Colormap 三角形采样坐标），统一管理 sRGB 与 Linear RGBA 线性色彩转换数学。
- **方块分层染色注册表与分类器**：精准支持原版方块染色规则（如 `pink_petals` / `wildflowers` 的花瓣不染色与茎叶染色分层、`dead_bush` / `azalea_leaves` 永不染色保护、硬编码固定色方块如云杉/白桦树叶）。
- **多导出器材质与网格图集解混淆**：
  - 数据驱动支持 Mineways 等网格图集 (`GridAtlasSpec`，18x18 包含 1px 保护边框像素解码)；
  - 自动识别并纠正非正交旋转 UV（适配 `mtk_core` 旋转检测与扶正）；
  - 自动归一化超限平铺 UV 并提取仿射变换矩阵（适配 `MC_Atlas_UV_Tiling` 着色器节点）。
- **Rayon 并行多 UV 批量重映射**：同时输出图集 UV (`atlas_uvs`) 与局部标准 [0, 1] UV (`local_uvs`)，输出逐面 Chunk ID、Texture ID、UV 模式与 Overlay 标记。

---

## 2. 核心数据结构与枚举 (Core Structures & Enums)

### 2.1 生物群系与调色板 (`biome/palettes.rs` & `hardcoded.rs`)
```rust
pub static CANONICAL_BIOMES: &[BiomePalette];

#[derive(Debug, Clone, PartialEq)]
pub struct BiomePalette {
    pub id: &'static str,
    pub name: &'static str,
    pub grass_hex: &'static str,
    pub foliage_hex: &'static str,
    pub dry_foliage_hex: &'static str,
    pub water_hex: &'static str,
    pub temperature: f32,
    pub humidity: f32,
    pub has_custom_grass: bool,
    pub has_custom_foliage: bool,
    pub has_custom_dry_foliage: bool,
}
```
- `pal.grass_linear() -> [f32; 4]`: 返回 Linear RGBA 线性草方块颜色。
- `pal.foliage_linear() -> [f32; 4]`: 返回 Linear RGBA 树叶颜色。
- `pal.dry_foliage_linear() -> [f32; 4]`: 返回 Linear RGBA 干枯植被颜色。
- `pal.water_linear() -> [f32; 4]`: 返回 Linear RGBA 水体颜色。
- `pal.colormap_uv() -> [f32; 2]`: 返回原版 256x256 三角形 Colormap 采样坐标 `[1.0 - t, h * t]`。

**染色分类常量**：
- `TINT_TYPE_NONE = 0`: 不染色
- `TINT_TYPE_GRASS = 1`: 草本染色（取 grass colormap 或调色板）
- `TINT_TYPE_FOLIAGE = 2`: 树叶染色（取 foliage colormap 或调色板）
- `TINT_TYPE_WATER = 3`: 水体染色
- `TINT_TYPE_HARDCODED = 4`: 硬编码固定颜色（云杉树叶、睡莲、红石线等）
- `TINT_TYPE_DRY_FOLIAGE = 5`: 苍白/干枯植被染色

**沼泽草双色 Perlin 噪声计算**：
- `mtk_material::get_swamp_grass_color(x, z) -> [f32; 4]`: 严格按照原版算法（频率 0.0225、阈值 -0.1）在双色（`#4C763C` / `#6A7039`）间进行权威采样。

**常驻 Colormap 内存像素采样 (`biome/colormap.rs`)**：
- `mtk_material::get_default_colormaps() -> Arc<VanillaColormaps>`: 获取内置原版草地、树叶、干枯树叶 RGB 色图。
- `cms.sample_grass_linear(t, h) -> [f32; 4]`: 根据温度/湿度最近邻采样并转换为 Linear RGBA。
- `cms.sample_foliage_linear(t, h) -> [f32; 4]`: 树叶色图最近邻采样。
- `cms.sample_dry_foliage_linear(t, h) -> [f32; 4]`: 干枯树叶色图最近邻采样。

---

### 2.2 生物群系解析器与网格面属性生成 (`biome/resolver/` & `batch.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TintInfo {
    pub tint_type: u8,
    pub tint_category: String,
    pub tint_weight: f32,
    pub base_tint_weight: f32,
    pub overlay_tint_weight: f32,
    pub has_overlay: bool,
    pub overlay_texture: Option<String>,
    pub is_hardcoded: bool,
    pub hardcoded_color: Option<[f32; 4]>,
    pub hardcoded_hex: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MeshBiomeAttributesResult {
    /// mtk_biome_tint_data: (base_tint_weight, overlay_tint_weight, tint_weight, tint_type)
    pub packed_tint_data: Vec<[f32; 4]>,
    /// mtk_biome_tint_color: (r, g, b, a) Linear RGBA
    pub tint_colors: Vec<[f32; 4]>,
}
```

---

### 2.3 材质解析器与网格图集规范 (`resolver/` & `types.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridAtlasSpec {
    pub swatch_size: f32,
    pub tile_size: f32,
    pub border: f32,
    pub image_width: u32,
    pub image_height: u32,
    pub atlas_name_patterns: Vec<String>,
    pub atlas_suffix_patterns: Vec<String>,
    pub swatch_to_candidates: HashMap<usize, Vec<String>>,
}

pub struct MaterialResolver;
```

---

### 2.4 多通道 UV 批量重映射结果 (`types.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshMultiUvRemapResult {
    pub face_count: usize,
    pub loop_count: usize,
    pub atlas_uvs: Vec<[f32; 2]>,
    pub local_uvs: Vec<[f32; 2]>,
    pub face_chunk_ids: Vec<u16>,
    pub face_texture_ids: Vec<u32>,
    pub face_uv_transforms: Vec<[f32; 4]>,
    pub face_uv_rotations: Vec<f32>,
    pub unmapped_faces: usize,
}
```

---

## 3. 核心公共 API 清单 (Public APIs)

### 3.1 色彩数学与生物群系工具
| 函数签名 | 描述 |
| :--- | :--- |
| `hex_to_linear_rgba(hex: &str) -> [f32; 4]` | 十六进制色彩字符串（如 `"#91BD59"`）转 Linear RGBA。 |
| `get_colormap_uv(temp: f32, hum: f32) -> [f32; 2]` | 计算原版三角形 Colormap 采样 UV。 |
| `get_biome_palette(name: &str) -> &'static BiomePalette` | 获取权威生物群系调色板（默认回退 `plains`）。 |
| `default_biome_palette() -> &'static BiomePalette` | 获取默认生物群系调色板 (`plains`)。 |
| `get_swamp_grass_color(x: f32, z: f32) -> [f32; 4]` | 获取沼泽草基于原版 0.0225 频率 Perlin 噪声的权威双色插值色彩 (Linear RGBA)。 |
| `get_unit_cube_tint_indices(block_name: &str) -> [i16; 6]` | 获取单位立方体 6 个方向的面染色标记。 |
| `get_default_colormaps() -> Arc<VanillaColormaps>` | 获取内置原版草地、树叶、干枯树叶 RGB 色图。 |
| `compute_mesh_biome_attributes_per_face(texture_keys, face_biomes, resolver)` | Rayon 并行按逐面生物群系数组计算染色面属性与特异群系路由。 |
| `compute_mesh_biome_attributes_with_colormaps(...)` | 显式传入 colormaps 批量计算网格染色属性。 |
| `compute_mesh_biome_attributes_custom_with_colormaps(...)` | 自定义群系设置的染色属性计算。 |
| `compute_mesh_biome_attributes_paletted_flat(palette, indices, ...)` | palette-native 全量群系属性计算，避免逐面 `String`。 |
| `reapply_uniform_biome_tint_flat(base_tt, packed, old_colors, colors, flags)` | 即时统一群系重着色：并行输出扁平 tint data/color，不重新解析贴图键。 |
| `reapply_uniform_biome_tint(...)` | 同上，返回 array-of-structs 形式，便于非 Python 宿主。 |

### 3.2 材质与 UV 重映射
| 函数签名 | 描述 |
| :--- | :--- |
| `remap_local_to_atlas(u, v, sprite) -> [f32; 2]` | 将 [0, 1] 局部 UV 映射到图集 Sprite 的 Frame 0 区域。 |
| `MaterialResolver::resolve(mat_name, aliases, address_map)` | 多级匹配解析材质名称到 `AtlasSpriteLocation`（静态 Frame 0 优先）。 |
| `resolve_animated_sprite(mat_name, aliases, address_map)` | 仅查找专用动画 sprite（`anim_sprites`），支持原生别名/清理名/用户别名。 |
| `extract_animation_metrics(sprite) -> ([frames, frametime, interpolate], [w, h, 0])` | 统一提取动画调度器所需元数据；静态 sprite 返回安全默认值。 |
| `remap_mesh_multi_uvs_paletted_with_options(...)` | 单一规范批量 UV 重映射入口：palette + u32 indices 版本；返回 source-key palette/indices 与零拷贝 id/metric buffer。 |
| `resolve_mesh_material_routes(keys, aliases, address_map, enable_animated)` | 只解析逐面 chunk/texture/动画路由，不触碰 UV。 |
| `reroute_mesh_animation(uvs, ranges, keys, old_chunk_ids, atlas, aliases, enable_animated)` | 将已有 Atlas UV 从旧 sprite 反解并投影到新 sprite，支持静态↔动画幂等切换。 |
| `decode_baked_atlas(atlas: &mut BakedAtlas) -> Result<PbrDecodeSummary, MaterialError>` | 将烘焙图集中各分块的 LabPBR 伴生贴图解码为规范 MtkPackedV1 缓冲，并自动注入水体/玻璃/冰块等内置物理预设。 |
| `get_physical_preset(path: &str) -> Option<PhysicalMaterialPreset>` | 查询水体/玻璃/冰块/蜂蜜/史莱姆等材质的内置物理预设（透射、IOR、粗糙度）。 |



## 4. 快速上手示例 (Quick Start)

### 示例 1：查询权威生物群系调色板与色彩空间转换
```rust
use mtk_material::biome::{get_biome_palette, hex_to_linear_rgba, srgb_to_linear};

fn main() {
    // 1. 查询樱花树林 (Cherry Grove) 调色板
    let cherry = get_biome_palette("cherry_grove");
    println!("Biome: {}", cherry.name);
    println!("Grass Hex: {}", cherry.grass_hex);
    println!("Grass Linear RGBA: {:?}", cherry.grass_linear());
    println!("Colormap UV: {:?}", cherry.colormap_uv());

    // 2. 自定义十六进制色彩转 Linear RGBA (用于 Blender 着色器注入)
    let linear_color = hex_to_linear_rgba("#80A755");
    println!("Spruce leaves linear color: {:?}", linear_color);
}
```

### 示例 2：Rayon 并行计算网格面生物群系着色属性
```rust
use mtk_material::biome::{compute_mesh_biome_attributes, BiomeResolver};

fn main() {
    let resolver = BiomeResolver::new();

    // 模拟来自网格各面的材质/纹理标识符
    let face_texture_keys = vec![
        "minecraft:block/grass_block_top".to_string(),
        "minecraft:block/oak_leaves".to_string(),
        "minecraft:block/stone".to_string(),
        "minecraft:block/dead_bush".to_string(),
    ];

    // 计算平原生物群系下的面属性
    let attrs = compute_mesh_biome_attributes(
        &face_texture_keys,
        "plains",
        None,
        &resolver,
    );

    for i in 0..face_texture_keys.len() {
        println!(
            "Face {}: Tint Data: {:?}, Color: {:?}",
            face_texture_keys[i],
            attrs.packed_tint_data[i],
            attrs.tint_colors[i],
        );
    }
}
```

### 示例 3：全网格多通道 UV 并行重映射 (Atlas UV + Local UV)
```rust
use mtk_material::remap_mesh_multi_uvs_parallel;
use mtk_texture::AtlasAddressMap;

fn main() {
    let address_map = AtlasAddressMap::new(); // 从 atlas_mapping.json 加载...

    let mut source_uvs = vec![
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], // Face 0 (Quad)
    ];
    let face_materials = vec!["block/oak_planks".to_string()];
    let face_loop_ranges = vec![(0u32, 4u32)];

    let result = remap_mesh_multi_uvs_parallel(
        &source_uvs,
        &face_materials,
        &face_loop_ranges,
        &address_map,
        None,
        None,
    );

    println!("Remapped {} faces ({} loops)", result.face_count, result.loop_count);
    println!("Atlas UVs: {:?}", result.atlas_uvs);
    println!("Local UVs: {:?}", result.local_uvs);
}
```

### 示例 4：查询红石引线信号强度动态硬编码染色
```rust
use mtk_material::biome::hardcoded::{get_redstone_wire_color, get_redstone_wire_hex};
use mtk_material::BiomeResolver;

fn main() {
    // 1. 直接查询 0 与 15 级红石线性色彩
    println!("Off state hex: {}", get_redstone_wire_hex(0));  // -> "#4B0000"
    println!("Max signal hex: {}", get_redstone_wire_hex(15)); // -> "#FF2600"
    println!("Max signal Linear: {:?}", get_redstone_wire_color(15));

    // 2. 通过 BiomeResolver 动态解算方块状态中的信号等级
    let resolver = BiomeResolver::new();
    let tint = resolver.get_tint_info(
        "redstone_dust_line0",
        Some("minecraft:redstone_wire[power=15]"),
        Some(0),
    );
    assert_eq!(tint.tint_type, 4); // TINT_TYPE_HARDCODED
    assert_eq!(tint.hardcoded_hex.as_deref(), Some("#FF2600"));
}
```

---

## 5. Feature 开关与依赖

`mtk-material` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `std` | 是 | 启用标准库与 `mtk-core/std`、`mtk-resource/std`、`mtk-texture/std`、`zip`。 |
| `parallel` | 是 | 启用 `rayon` 多线程并发加速（如大批量面群系属性计算与 UV 重映射）。 |

---

## 6. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。

