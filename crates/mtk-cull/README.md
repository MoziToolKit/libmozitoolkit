# mtk-cull

[![Crate](https://img.shields.io/badge/crate-mtk--cull-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-cull`** 是 MoziToolKit (`libmtk`) 体系的微观遮挡剔除与几何消重引擎。专注于 Minecraft 邻域面遮挡判定、2D 矩形差集切分（Occlusion Boolean Operations）、隐藏体素体裁剪，以及对外部导入网格（如 Mineways、jmc2obj、Blockbench OBJ）进行高精度的共面叠面消除与不可见面剔除。

---

## 目录 (Table of Contents)

- [1. 核心特性与职责边界](#1-核心特性与职责边界)
- [2. 模块结构与核心 API](#2-模块结构与核心-api)
  - [2.1 邻域面剔除引擎 (`engine`, `rules`, `types`)](#21-邻域面剔除引擎-engine-rules-types)
  - [2.2 2D 矩形布尔与遮挡检测 (`geometry::rect_ops`)](#22-2d-矩形布尔与遮挡检测-geometryrect_ops)
  - [2.3 共面分析与几何消重 (`sanitizer`, `mesh_cull`, `geometry::coplanar`)](#23-共面分析与几何消重-sanitizer-mesh_cull-geometrycoplanar)
- [3. 快速上手示例 (Quick Start)](#3-快速上手示例-quick-start)
  - [示例 1：使用 `FaceCuller` 进行方块面可见性判定](#示例-1使用-faceculler-进行方块面可见性判定)
  - [示例 2：2D 矩形差集切分 (半砖/局部遮挡)](#示例-22d-矩形差集切分-半砖局部遮挡)
- [4. Feature 开关与依赖](#4-feature-开关与依赖)
- [5. 开源协议 (License)](#5-开源协议-license)

---

## 1. 核心特性与职责边界

- **邻域面遮挡状态机 (`FaceCuller`)**：原版 1.21+ 规范的 6 向邻域剔除判定，内置实心不透明体 (`SolidOpaque`)、半透明玻璃 (`GlassTranslucent`)、镂空树叶 (`CutoutLeaves`)、异形方块 (`PartialShape`)、流体 (`Fluid`) 与非遮挡装饰 (`NonOccluding`) 的全状态分类决策。
- **高度感知流体面剔除 (`should_render_fluid_face`)**：独立复刻原版 `FluidRenderer` 语义。流体顶面仅在表面到达 `y = 1.0` 时才被上方方块遮挡；非满高流体（水源 `8/9`、斜向水流）在方块下方仍保留顶面，水平面使用该侧最大角点高度。
- **2D 矩形差集切分算法 (`subtract_rect`)**：极速微观 2D 矩形布尔差集运算（返回 `Vec<Aabb2d>`），精准解算半砖 (Slabs)、楼梯 (Stairs)、活板门 (Trapdoors) 与地毯等异形方块的局部遮挡面。
- **共面关系分析 (`check_coplanar_overlap`)**：2D 投影下判定两多边形是否 `Exact` / `ContainedInA` / `ContainedInB` / `Partial` 重叠，供上层做叠面去重。
- **网格微观剔除与叠面消除 (`cull_mesh_faces`)**：基于空间哈希的高性能网格面剔除算子，用于烘焙期内部多元素模型消重（如 `mtk-model` 消除内部重叠体积），完整保留所有顶点/角点/面级自定义属性与 Quad 拓扑。

---

## 2. 模块结构与核心 API

### 2.1 邻域面剔除引擎 (`engine`, `rules`, `types`)

| 类型 / 结构体 | 说明 |
| :--- | :--- |
| [`FaceCuller`](src/engine/face_culler.rs) | 统一面剔除引擎，内部维护线程安全的 `BlockCullMeta` 元数据缓存（超过 8192 条后按任意键淘汰，非严格 LRU）。 |
| [`BlockCullMeta`](src/types.rs) | 预烘焙方块剔除元数据，包含 6 向遮挡矩形列表 `face_shapes: [Vec<Aabb2d>; 6]`、满面位掩码 `full_face_mask` 与不透明标记。 |
| [`CullCategory`](src/types.rs) | 方块剔除类别：`SolidOpaque`, `GlassTranslucent`, `CutoutLeaves`, `PartialShape`, `Fluid`, `NonOccluding`, `Air`。 |
| [`LeavesCullMode`](src/types.rs) | 树叶剔除模式：`Fancy`（原版双面双透）、`SingleFace`（接触面单面保留，防闪烁）、`Fast`（不透明壳体互剔）、`None`。 |
| [`GlassCullMode`](src/types.rs) | 玻璃剔除模式：`Group`（跨色玻璃全局互剔）、`SameBlock`（同色方块互剔）、`None`。 |
| [`is_inherently_submerged_block`](src/rules/categories/meta.rs) | `(name: &str) -> bool`：判定原版中无 `waterlogged` 属性但实质恒定处于水下的方块（水草 `seagrass`、海带 `kelp` 等）。在属性解析时自动注入 `waterlogged=true` 作为普通含水方块处理。 |
| `should_skip_rendering` | `(curr_cat, neighbor_cat, ...) -> bool`：基于类别与方块属性的快速渲染跳过规则。 |
| [`FaceCuller::should_render_fluid_face`](src/engine/face_culler.rs) | `(fluid_meta, self_meta, neighbor_meta, direction, face_height) -> bool`：原版 `FluidRenderer.isFaceOccludedByNeighbor` / `isFaceOccludedBySelf` 的高度感知流体面可见性判定。`face_height` 在顶面传四角最小值、侧面传该侧两角最大值。 |
| [`fluid_cull`](src/rules/fluid_cull.rs) | `fluid_group` / `is_same_fluid` / `is_face_occluded_by_state` / `should_render_fluid_face`：独立于方块满面状态的流体剔除原语。 |

### 2.2 2D 矩形布尔与遮挡检测 (`geometry::rect_ops`)

| 函数 | 签名 / 描述 |
| :--- | :--- |
| `subtract_rect` | `(subject: &Aabb2d, clip: &Aabb2d) -> Vec<Aabb2d>`：从目标矩形中裁剪单个遮挡矩形，返回剩余不相交子矩形碎片。 |
| `is_face_completely_occluded` | `(subjects: &[Aabb2d], occluders: &[Aabb2d]) -> bool`：判定一组目标矩形面是否被遮挡矩形集合完全覆盖。 |
| `extract_quad_face_occlusion_rect` | `(vertices: &[Vec3; 4], dir: Direction) -> Option<Aabb2d>`：从 3D 四边形中投影提取 2D 单位面上的轴对齐遮挡矩形。 |

### 2.3 共面分析与外部网格剔除 (`mesh_cull`, `geometry::coplanar`)

| 类型 / 函数 | 说明 |
| :--- | :--- |
| [`MeshCullConfig`](src/mesh_cull.rs) | 网格剔除配置：`tolerance`（几何容差）、`cull_duplicates`（同向重复面消除）、`cull_coplanar_opposite`（反向接触面剔除）。 |
| [`MeshCullResult`](src/mesh_cull.rs) | 剔除统计结果，包含 `initial_faces`, `culled_faces`, `remaining_faces`, 净化后的 `mesh: MeshData` 与 `profile: Option<ExecutionProfile>`。 |
| `cull_mesh_faces` | `(mesh: &MeshData, config: &MeshCullConfig) -> MeshCullResult`：外部网格微观剔除与叠面消除入口。 |
| `check_coplanar_overlap` | `(verts_a, norm_a, verts_b, norm_b, tol) -> Option<CoplanarRelation>`：计算两多边形的共面投影重叠关系（`Exact`, `ContainedInA`, `ContainedInB`, `Partial`）。 |

---

## 3. 快速上手示例 (Quick Start)

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
mtk-cull = { version = "0.1.0" }
mtk-core = { version = "0.1.0" }
glam = "0.29"
```

### 示例 1：使用 `FaceCuller` 进行方块面可见性判定

```rust
use mtk_cull::{FaceCuller, GlassCullMode, LeavesCullMode, CullCategory};
use mtk_core::Direction;

fn main() {
    let culler = FaceCuller::new(LeavesCullMode::SingleFace, GlassCullMode::Group);

    // 获取方块元数据（自动解析并缓存）
    let stone = culler.get_meta("minecraft:stone", None, None);
    let dirt = culler.get_meta("minecraft:dirt", None, None);
    let air = culler.get_meta("minecraft:air", None, None);

    assert_eq!(stone.category, CullCategory::SolidOpaque);

    // 石头在东侧 (+X) 与泥土紧贴：石头的东面应当被剔除
    let render_east = culler.should_render_face(
        &stone,
        Some(&dirt),
        Direction::East,
        None,
        None,
        None,
    );
    assert!(!render_east, "Touching solid face should be culled");

    // 石头在东侧接触空气：石头的东面应当被渲染
    let render_air = culler.should_render_face(
        &stone,
        Some(&air),
        Direction::East,
        None,
        None,
        None,
    );
    assert!(render_air, "Face touching air should be rendered");
}
```

### 示例 1.1：高度感知的流体顶面剔除

```rust
use mtk_cull::FaceCuller;
use mtk_core::Direction;

let culler = FaceCuller::default();
let water = culler.get_meta("minecraft:water[level=0]", None, None);
let stone = culler.get_meta("minecraft:stone", None, None);

// 水源表面在 8/9，方块底面在 1.0：顶面必须保留。
assert!(culler.should_render_fluid_face(
    &water, None, Some(&stone), Direction::Up, 8.0 / 9.0,
));

// 仅当流体表面真正到达 y = 1.0 时，上方满面方块才遮挡顶面。
assert!(!culler.should_render_fluid_face(
    &water, None, Some(&stone), Direction::Up, 1.0,
));
```

### 示例 2：2D 矩形差集切分 (半砖/局部遮挡)

```rust
use mtk_core::Aabb2d;
use mtk_cull::geometry::subtract_rect;

fn main() {
    // 完整的 1.0 x 1.0 单位方块面
    let full_face = Aabb2d::from_min_max(0.0, 0.0, 1.0, 1.0);
    // 下半砖遮挡区域 [0.0..1.0, 0.0..0.5]
    let bottom_slab = Aabb2d::from_min_max(0.0, 0.0, 1.0, 0.5);

    // 执行布尔差集切分
    let remaining_pieces = subtract_rect(&full_face, &bottom_slab);

    assert_eq!(remaining_pieces.len(), 1);
    let top_half = remaining_pieces[0];
    assert_eq!(top_half.min.y, 0.5);
    assert_eq!(top_half.max.y, 1.0);
    println!("Unoccluded remainder area: {}", top_half.area());
}
```

---

## 4. Feature 开关与依赖

`mtk-cull` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `serde` | 是 | 为 `BlockCullMeta`, `CullCategory` 等结构体实现序列化与反序列化。 |

> 本 crate 仅支持 `std`（`wasm32-unknown-unknown` 亦走 `std`），不提供 `no_std` 构建。

---

## 5. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
