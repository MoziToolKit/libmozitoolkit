# mtk-texture

[![Crate](https://img.shields.io/badge/crate-mtk--texture-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Rayon](https://img.shields.io/badge/Rayon-Parallel-red.svg)](https://github.com/rayon-rs/rayon)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-texture`** 是 `libmozitoolkit` (`libmtk`) 体系中的无头图像处理、2D 空间二叉装箱图集生成器 (Stitcher)、多通道 PBR 伴随贴图对齐与双图集（静态 / 动画）烘焙核心。遵循 **Host-Agnostic（宿主无关）** 原则，为上层材质映射 (`mtk-material`)、端到端预编译 (`libmtk`) 与 DCC 宿主提供极速像素处理、静态/动态双图集装箱与边缘抗渗色保护。

---

## 目录 (Table of Contents)

- [1. 架构定位与职责边界 (Architecture & Responsibilities)](#1-架构定位与职责边界-architecture--responsibilities)
- [2. 核心数据结构与枚举 (Core Structures & Enums)](#2-核心数据结构与枚举-core-structures--enums)
  - [2.1 像素图像缓冲与处理 (`image/`)](#21-像素图像缓冲与处理-image)
  - [2.2 图集地址映射表与元数据 (`atlas/address_map.rs`)](#22-图集地址映射表与元数据-atlasaddress_maprs)
  - [2.3 图集构建器与装箱器 (`atlas/builder/` & `stitcher/`)](#23-图集构建器与装箱器-atlasbuilder--stitcher)
- [3. 核心公共 API 清单 (Public APIs)](#3-核心公共-api-清单-public-apis)
  - [3.1 `AtlasBuilder` 与 `AtlasAddressMap`](#31-atlasbuilder-与-atlasaddressmap)
  - [3.2 图像与调色板工具](#32-图像与调色板工具)
- [4. 快速上手示例 (Quick Start)](#4-快速上手示例-quick-start)
  - [示例 1：烘焙带 PBR 伴随层的多图集并查询 UV](#示例-1烘焙带-pbr-伴随层的多图集并查询-uv)
  - [示例 2：实时烘焙盔甲纹饰调色板 (Armor Trim Permutation)](#示例-2实时烘焙盔甲纹饰调色板-armor-trim-permutation)
- [5. Feature 开关与依赖](#5-feature-开关与依赖)
- [6. 开源协议 (License)](#6-开源协议-license)

---

## 1. 架构定位与职责边界 (Architecture & Responsibilities)

`mtk-texture` 是 `libmozitoolkit` 体系中的图像处理、多通道 PBR 对齐与空间装箱图集生成核心，具备以下核心职责与边界：

- **2D 空间二叉装箱算法 (Stitcher)**：实现原版风格的 2D 矩形空间分割算法，自动按 2 的幂次动态扩容与多 Chunk 分片，支持 Mipmap 像素对齐。
- **PBR 多通道对齐与动图分流 (Dual-Mode Architecture)**：
  - **静态图集 (Static Atlas)**：包含 100% 纹理覆盖（动态纹理截取 Frame 0 方块），保障通用着色器极致性能与零采样溢出；
  - **动态图集 (Animated Atlas)**：包含长条竖直动画帧（Full Strip），自动将静态或短帧 Normal/Specular 通道按需循环垂向平铺 (`tile_vertical`)，实现 PBR 通道 1:1 帧对齐。
- **抗渗色边缘边缘填充 (Edge Clamping Padding)**：针对图集中相邻小图块进行边界外推复制填充，杜绝 Mipmap 降采样与双线性插值 (Bilinear Filtering) 导致黑边或相邻图块色彩渗漏。
- **调色板排列实时烘焙 (Paletted Permutations)**：支持盔甲纹饰 (Armor Trims) 与动态换色方块的调色板像素级实时无损替换。

---

## 2. 核心数据结构与枚举 (Core Structures & Enums)

### 2.1 像素图像缓冲与处理 (`image/`)
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA 4 字节/像素，行主序 (Row-Major)
}
```
- `RgbaBuffer::new(w, h) -> Self`: 创建透明黑色缓冲。
- `RgbaBuffer::solid(w, h, r, g, b, a) -> Self`: 创建纯色填充缓冲。
- `RgbaBuffer::from_png_bytes(bytes: &[u8]) -> Result<Self, TextureError>`: 解码 PNG 字节流。
- `buf.to_png_bytes() -> Result<Vec<u8>, TextureError>`: 编码为标准 PNG 字节流。
- `buf.blit(src, src_x, src_y, dst_x, dst_y, w, h)`: 高性能无损矩形像素拷贝。
- `buf.crop(x, y, w, h) -> Self`: 裁剪提取子区域。
- `buf.tile_vertical(target_height) -> Self`: 垂直重复平铺至目标高度。
- `buf.resize_nearest(new_w, new_h) -> Self`: 最近邻插值缩放，确保像素边缘锋利且 PBR 通道不失真。
- `buf.align_companion_to_albedo(albedo_fw, albedo_fh, albedo_fc) -> Self`: 伴随通道智能对齐。

---

### 2.2 图集地址映射表与元数据 (`atlas/address_map.rs`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpriteKind {
    StaticAtlas,
    AnimatedAtlas,
    StandaloneOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasSpriteLocation {
    pub chunk_id: u16,
    pub category: String,
    pub is_animated: bool,
    pub sprite_kind: SpriteKind,
    pub texture_id: u32,
    /// 归一化 UV 边界 [u_min, v_min, u_max, v_max]
    pub uv_bounds: [f32; 4],
    /// Frame 0 专属归一化 UV 边界 [u_min, v_min, u_max, v_max]
    pub frame_0_uv_bounds: [f32; 4],
    /// 局部单方块 [0, 1] UV 边界
    pub local_uv_bounds: [f32; 4],
    /// 动图单帧 UV 步进 [u_step, v_step]
    pub frame_uv_step: [f32; 2],
    /// Frame 0 在图集 Chunk 上的像素矩形 [x, y, width, height]
    pub pixel_rect: [u32; 4],
    /// 整个动画长条的像素矩形 [x, y, width, height]
    pub strip_pixel_rect: [u32; 4],
    pub frame_size: [u32; 2],
    pub frame_count: u32,
    pub animation: Option<AnimationMetadata>,
    pub has_normal: bool,
    pub has_specular: bool,
    pub has_overlay: bool,
    /// 材质纯色/平均颜色十六进制字符串 "#RRGGBB"（不含 Alpha）
    pub average_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtlasTextureSlot {
    pub role: String,
    pub file_name: String,
    pub colorspace: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasChunkMeta {
    pub chunk_id: u16,
    pub category: String,
    pub is_animated: bool,
    pub category_chunk_index: usize,
    pub width: u32,
    pub height: u32,
    pub has_normal: bool,
    pub has_specular: bool,
    pub has_overlay: bool,
    pub has_orm: bool,
    pub has_aux: bool,
    pub has_tint: bool,
    pub pbr: Option<PbrMeta>,
    pub textures: Vec<AtlasTextureSlot>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AtlasAddressMap {
    pub chunks: Vec<AtlasChunkMeta>,
    pub sprites: HashMap<ResourceLocation, AtlasSpriteLocation>,
    pub anim_sprites: HashMap<ResourceLocation, AtlasSpriteLocation>,
}

impl AtlasAddressMap {
    pub fn default_chunk_id(&self) -> u16;
}
```

---

### 2.3 图集构建器与配置 (`atlas/builder/`)
```rust
pub struct AtlasBuilderConfig {
    pub max_width: u32,
    pub max_height: u32,
    pub mip_level: u32,
    pub padding: u32,
}
```

---

### 2.4 动态皮肤图集 (`atlas/dynamic.rs`)
针对玩家头颅 (Player Skull) 与各类自定义第三方皮肤站动态装配打造的固定规格动态图集块：
- **规格**：固定 1024×1024 尺寸，16×16 = 256 个 64×64 槽位；
- **保留图集块 ID**：`DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID = 0xFFF0` (65520)；
- **去重与映射**：支持以皮肤键天然去重、直接通过 `register_to_address_map` 将槽位与别名无缝注入 `AtlasAddressMap`。

```rust
pub struct BakedAtlasChunk {
    pub chunk_id: u16,
    pub category: String,
    pub is_animated: bool,
    pub category_chunk_index: usize,
    pub width: u32,
    pub height: u32,
    pub albedo: RgbaBuffer,
    pub normal: Option<RgbaBuffer>,
    pub specular: Option<RgbaBuffer>,
    pub overlay: Option<RgbaBuffer>,
    pub normal_gl: Option<RgbaBuffer>,
    pub orm: Option<RgbaBuffer>,
    pub aux: Option<RgbaBuffer>,
}

pub struct BakedAtlas {
    pub chunks: Vec<BakedAtlasChunk>,
    pub address_map: AtlasAddressMap,
}
```

---

## 3. 核心公共 API 清单 (Public APIs)

### 3.1 `AtlasBuilder` 与 `AtlasAddressMap`
| 方法签名 | 描述 |
| :--- | :--- |
| `AtlasBuilder::new(config: AtlasBuilderConfig) -> Self` | 创建图集构建器。 |
| `builder.build(stack, definition) -> Result<BakedAtlas, TextureError>` | 构建默认 `"blocks"` 类别图集。 |
| `builder.build_category(stack, category) -> Result<BakedAtlas, TextureError>` | 构建指定分类图集（如 `Items`, `Chests`）。 |
| `builder.build_categories(stack, definitions) -> Result<BakedAtlas, TextureError>` | 批量烘焙多类别图集并输出全局唯一索引表。 |
| `address_map.lookup(loc: &ResourceLocation) -> Option<&AtlasSpriteLocation>` | O(1) 查询精灵（静态优先，次查动态）。 |
| `address_map.lookup_static(loc) / lookup_animated(loc)` | 分别精准查询静态 Frame 0 或长条动图槽位。 |
| `address_map.lookup_str(s: &str) -> Option<&AtlasSpriteLocation>` | 支持字符串路径（自动容错解析）。 |
| `address_map.to_json() / from_json(json_str)` | 序列化/反序列化 `atlas_mapping.json`。 |

### 3.2 图像与调色板工具
| 函数签名 | 描述 |
| :--- | :--- |
| `sample_alpha_f32(w, h, pixels, u, v, invert_y) -> f32` | 采样指定 UV 处的透明度 (0.0~1.0)。 |
| `batch_analyze_transparent_faces_f32(faces_uvs, w, h, pixels, mode, threshold, invert_y) -> Vec<bool>` | Rayon 并行批量分析网格面透明度。 |

---

## 4. 快速上手示例 (Quick Start)

### 示例 1：烘焙带 PBR 伴随层的多图集并查询 UV
```rust
use mtk_resource::{AtlasCategory, ResourcePackStack};
use mtk_texture::{AtlasBuilder, AtlasBuilderConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stack = ResourcePackStack::new();
    // 载入你的材质包...

    let builder = AtlasBuilder::new(AtlasBuilderConfig {
        max_width: 2048,
        max_height: 2048,
        mip_level: 0,
        padding: 1, // 启用 1px 边缘抗渗色填充
    });

    // 烘焙 Blocks 类别图集
    let baked = builder.build_category(&stack, &AtlasCategory::Blocks)?;

    println!("Generated {} atlas chunks", baked.chunks.len());
    for chunk in &baked.chunks {
        println!(
            "Chunk {}: {}x{} (Normal: {}, ORM: {}, Aux: {})",
            chunk.file_stem(),
            chunk.width,
            chunk.height,
            chunk.normal_gl.is_some(),
            chunk.orm.is_some(),
            chunk.aux.is_some(),
        );
    }

    // 查询草方块顶面在图集中的 UV 范围
    if let Some(loc) = baked.address_map.lookup_str("block/grass_block_top") {
        println!("Grass top UV bounds: {:?}", loc.frame_0_uv_bounds);
        println!("Chunk ID: {}", loc.chunk_id);
    }

    // 导出 atlas_mapping.json
    let mapping_json = baked.address_map.to_json()?;
    std::fs::write("atlas_mapping.json", mapping_json)?;

    Ok(())
}
```

---

## 5. Feature 开关与依赖

`mtk-texture` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `std` | 是 | 启用标准库与 `mtk-core/std`、`mtk-resource/std`、`serde/std`。 |
| `parallel` | 是 | 启用 `rayon` 多线程并发加速（如图像编解码与多图集并发渲染）。 |

---

## 6. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
