# mtk-core

[![Crate](https://img.shields.io/badge/crate-mtk--core-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](../../LICENSE)

**`mtk-core`** 是 MoziToolKit (`libmtk`) 体系的底层几何计算核心与标准数据契约底座。该 Crate 纯 Rust 实现，严格遵循 **Host-Agnostic（宿主无关）** 与 **Data-In / Data-Out（纯数据输入输出）** 原则，为上层体素化、模型烘焙、材质映射与 DCC 宿主（如 Blender 扩展 `MoziToolKit`）提供高性能网格缓冲、6 向拓扑、自适应像素网格切分以及智能挤出侧面 UV 修复算子。

---

## 目录 (Table of Contents)

- [1. 核心特性与职责边界](#1-核心特性与职责边界)
- [2. 模块结构与核心 API](#2-模块结构与核心-api)
  - [2.1 几何缓冲与基元 (`mesh`, `geometry`, `attributes`)](#21-几何缓冲与基元-mesh-geometry-attributes)
  - [2.2 6 向拓扑与坐标系 (`direction`, `geometry`)](#22-6-向拓扑与坐标系-direction-geometry)
  - [2.3 自适应像素切分 (`subdivide`)](#23-自适应像素切分-subdivide)
  - [2.4 智能挤出与 UV 修复 (`extrude`)](#24-智能挤出与-uv-修复-extrude)
  - [2.5 确定性随机与植被偏移 (`random`)](#25-确定性随机与植被偏移-random)
- [3. 快速上手示例 (Quick Start)](#3-快速上手示例-quick-start)
  - [示例 1：构建基础单位立方体网格](#示例-1构建基础单位立方体网格)
  - [示例 2：执行自适应像素网格细分](#示例-2执行自适应像素网格细分)
  - [示例 3：空间坐标系变换](#示例-3空间坐标系变换)
- [4. Feature 开关与依赖](#4-feature-开关与依赖)
- [5. 开源协议 (License)](#5-开源协议-license)

---

## 1. 核心特性与职责边界

- **扁平连续几何缓冲 (`MeshData`)**：零堆碎片化的连续顶点、法线、UV、四边形索引 (`quad_indices`) 与通用层级自定义属性 (`MeshAttribute`) 容器，天然适配 GPU 缓冲区与 NumPy / DCC 零拷贝映射。
- **高性能多边形拓扑容器 (`FlatPolygonMesh`)**：针对多边形网格（N-gon / Quad / Tri）提供展平的 1D 循环缓冲区（Loop Buffers），消除成千上万个嵌套 `Vec<Vec<T>>` 堆分配开销。
- **Minecraft 6 向拓扑与坐标系转换**：标准 6 方向枚举 (`Direction`)、位掩码 (`DirMask`)，以及 Minecraft 局部坐标到标准化右手 Z-Up 坐标（`mc_local_to_centered_z_up`, `mc_world_to_z_up`）的精准转换。
- **自适应像素网格细分 (`subdivide`)**：基于纹理分辨率与 UV 物理边长智能计算细分网格，自动适应非正方形图集与长条动图安全截断，支持顶点属性的双线性插值 (`interpolate_bilinear_*`) 与微距流形焊接。
- **智能挤出与侧面 UV 重构 (`extrude`)**：智能判断挤出法线朝向，自动修复因几何挤出产生的拉伸 UV，支持图集安全边距钳位（Atlas Safe Padding Clamping）以及 3D Perlin / Cellular 噪声随机挤出。

---

## 2. 模块结构与核心 API

### 2.1 几何缓冲与基元 (`mesh`, `geometry`, `attributes`)

| 类型 / 结构体 | 说明 |
| :--- | :--- |
| [`MeshData`](src/mesh/mod.rs) | 核心连续网格缓冲，包含 `positions`, `normals`, `indices`, `uvs`, `uv_layout`, `secondary_uvs`, `colors`, `face_materials`, `face_tint_indices`, `quad_indices`, `custom_attributes`。提供零拷贝引用合并 `merge_all_refs(&[&MeshData])`、单阶段流式跨区块缝合合并 `merge_welded_sections(&[&MeshData], tolerance)`、按空间谓词限定焊接范围的 `merge_welded_sections_filtered(&[&MeshData], tolerance, predicate)` 以及显式 UV 布局策略的 `weld_vertices(tolerance)`。 |
| [`Quad`](src/geometry.rs) | 4 顶点平面四边形基元。支持 `Quad::unit_cube_face(Direction)` 生成标准单位立方体面（对齐 Minecraft 1.21+ FaceInfo 逆时针环绕规范）。 |
| [`FlatPolygonMesh`](src/polygon.rs) | 连续 1D 循环拓扑网格表示，提供 `face_vertices(i)` 与 `face_uvs(i)` 零拷贝切片访问器。 |
| [`Aabb2d`](src/geometry.rs) | 2D 轴对齐包围盒，提供相交判定与矩形布尔运算。 |
| [`FaceAttributes`](src/attributes.rs) | 面属性包，包含 `material_slot: MaterialSlotId` 与 `tint_index: TintIndex`。 |
| [`MeshAttribute`](src/attributes.rs) | 通用动态属性层，支持 `Point`, `Corner`, `Face`, `Mesh` 作用域及多种数值类型（`AttributeData`）。 |
| `analyze_hard_edges` | `(positions, edge_verts, face_loop_starts, face_loop_totals, loop_verts, sharp_angle_rad, explicit_sharp_edges) -> Vec<u8>`：基于网格二面角与边界的高性能拓扑硬边分析算子。 |

### 2.2 6 向拓扑与坐标系 (`direction`, `geometry`)

| 函数 / 枚举 | 签名 / 描述 |
| :--- | :--- |
| [`Direction`](src/direction.rs) | `East` (0), `West` (1), `Up` (2), `Down` (3), `South` (4), `North` (5)。支持 `opposite()`, `normal()`, `offset()`, `mask()` 等。 |
| [`DirMask`](src/direction.rs) | 6 方向位掩码（基于 `bitflags`），支持高效的相邻面快速位运算。 |
| `mc_local_to_centered_z_up` | `(lx: f32, ly: f32, lz: f32) -> Vec3`：将 Minecraft 局部体素坐标转换为以方块中心为原点的右手 Z-Up 坐标。 |
| `mc_world_to_z_up` | `(wx: f32, wy: f32, wz: f32) -> Vec3`：将 Minecraft 世界坐标 (+X East, +Y Up, +Z South) 转换为标准右手 Z-Up 坐标 (+X East, +Y North, +Z Up)。 |
| `Direction::canonical_face_tangents` | `(self) -> (Direction, Direction)`：返回未旋转规范立方面在 Minecraft 空间中的 2D 贴图切线基底 `(up, left)` 作为 SSOT。 |
| `derive_quad_face_tangents` | `(positions: &[Vec3; 4], uvs: &[Vec2; 4], fallback_dir: Direction, is_minecraft_uv: bool) -> (Direction, Direction)`：根据四边形 3D 几何向量与 2D UV 偏导数动态推导物理切线基底，自适应感知旋转方块与 UV 旋转。 |


### 2.3 自适应像素切分 (`subdivide`)

| 函数 | 描述 |
| :--- | :--- |
| `calculate_face_target_grid` | `(&uvs, tex_w, tex_h, pixels_per_face, max_subdivisions) -> (u32, u32)`：根据 UV 坐标与贴图尺寸推算最优细分列数和行数。 |
| `calculate_pixel_grid_cut_factors` | `(&uvs, tex_w, tex_h, pixels_per_face, max_subdivisions) -> (Vec<f32>, Vec<f32>)`：计算四边形在纹理整数像素边界上的归一化切分参数因子。 |
| `adaptive_pixel_split_mesh` | `(mesh, face_resolutions, default_res, pixels_per_face, max_subdiv, weld_dist) -> MeshData`：全网格批量自适应像素细分，支持多边形属性双线性插值与顶点焊接。 |
| `MeshData::weld_vertices` | `(&mut MeshData, tolerance: f32)`：基于 `uv_layout` 选择逐 loop 或逐顶点属性策略的原地顶点焊接。 |

### 2.4 智能挤出与 UV 修复 (`extrude`)

| 函数 / 枚举 | 描述 |
| :--- | :--- |
| [`ExtrudeUvMode`](src/extrude.rs) | `Smart`（智能法线判定）、`Inward`（边缘内缩采样）、`Outward`（外推连续条带采样）。 |
| [`ExtrudeNoiseType`](src/extrude.rs) | `UniformRandom`（伪随机）、`Perlin`（连续梯度噪声）、`Cellular`（Voronoi 细胞阶梯噪声）。 |
| `repair_extruded_side_uv` | `(...) -> [[f32; 2]; 4]`：计算侧面重构的 4 角点 UV，并执行 Safe Padding Clamping 避免图集相邻纹理渗色。 |
| `process_flat_mesh_extrude_repair` | `(mesh: &FlatPolygonMesh, ...) -> ExtrudeMeshOutput`：基于扁平连续内存网格的高性能挤出修复。 |

### 2.5 确定性随机与植被偏移 (`random`)

| 函数 / 结构体 | 描述 |
| :--- | :--- |
| `mc_coordinate_seed` | `(x: i32, y: i32, z: i32) -> i64`：1:1 对标原版 `Mth.getSeed`，基于世界绝对整数坐标计算确定性 64 位哈希种子。 |
| [`JavaRandom`](src/random.rs) | 1:1 对标 Java 标准 48 位线性同余随机数发生器（LCG），提供 `next_int(bound)`、`next_float()` 等。 |
| [`OffsetType`](src/random.rs) | `None`, `XZ`（花草水平偏移）, `XYZ`（草丛/蕨类/树根 3D 偏移与微下沉）。 |
| `get_block_offset` | `(offset_type, x, y, z) -> Vec3`：1:1 计算植被在方块内的连续位置抖动偏移量。 |
| `determine_block_offset_type` | `(block_id_or_state: &str) -> OffsetType`：依据原版规则判定方块标识符的植被偏移类型。 |

### 2.6 真实物理进度上报与节流 (`progress`)

| 类型 / 结构体 | 描述 |
| :--- | :--- |
| [`ProgressReport`](src/progress.rs) | 核心进度里程碑描述结构，包含阶段标识符 `stage`、真实已处理计数 `current`、总任务数 `total`、描述文本 `message` 与百分比计算 `percent()`。 |
| `ProgressCallback` | 跨线程/跨语言进度回调契约：`&dyn Fn(ProgressReport) + Send + Sync`。 |
| [`ProgressThrottler`](src/progress.rs) | 面向多线程（Rayon）与高频循环的高吞吐原子节流器，支持步进过滤 `with_step()`，保证 100% 触发终态汇报并杜绝锁竞争。 |

---

## 3. 快速上手示例 (Quick Start)

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
mtk-core = { version = "0.1.0" }
glam = "0.29"
```

### 示例 1：构建基础单位立方体网格

```rust
use glam::{Vec2, Vec3};
use mtk_core::attributes::FaceAttributes;
use mtk_core::direction::Direction;
use mtk_core::geometry::Quad;
use mtk_core::mesh::MeshData;

fn main() {
    let mut mesh = MeshData::new();

    // 为 6 个主方向构建标准四边形面
    for dir in Direction::ALL {
        let quad = Quad::unit_cube_face(dir);
        let attrs = FaceAttributes::default();
        mesh.append_quad(&quad, &attrs);
    }

    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.triangle_count(), 12);
    assert_eq!(mesh.quad_count(), 6);
}
```

### 示例 2：执行自适应像素网格细分

```rust
use mtk_core::mesh::MeshData;
use mtk_core::subdivide::adaptive_pixel_split_mesh;
use mtk_core::geometry::Quad;
use mtk_core::direction::Direction;
use mtk_core::attributes::FaceAttributes;

fn main() {
    let mut mesh = MeshData::new();
    let quad = Quad::unit_cube_face(Direction::Up);
    mesh.append_quad(&quad, &FaceAttributes::default());

    // 对面执行 16x16 纹理自适应像素切分（每个像素 1 个四边形）
    let subdivided = adaptive_pixel_split_mesh(
        &mesh,
        &[],              // 逐面可选分辨率覆盖（空表示全部走默认）
        (16, 16),         // default_resolution: 16x16
        1.0,              // pixels_per_face
        64,               // max_subdivisions
        1e-4,             // weld_dist
    );

    println!(
        "Subdivided mesh: {} vertices, {} triangles",
        subdivided.vertex_count(),
        subdivided.triangle_count()
    );
}
```

### 示例 3：空间坐标系变换

```rust
use mtk_core::geometry::{mc_local_to_centered_z_up, mc_world_to_z_up};
use glam::Vec3;

fn main() {
    // 局部体素原点 (0.5, 0.5, 0.5) 变换到中心对齐 Z-Up 坐标系
    let local_center = mc_local_to_centered_z_up(0.5, 0.5, 0.5);
    assert_eq!(local_center, Vec3::ZERO);

    // Minecraft 世界坐标 (+X East, +Y Up, +Z South) -> 右手 Z-Up (+X East, +Y North, +Z Up)
    let world_pos = mc_world_to_z_up(10.0, 64.0, -20.0);
    assert_eq!(world_pos, Vec3::new(10.0, 20.0, 64.0));
}
```

---

## 4. Feature 开关与依赖

`mtk-core` 在 `Cargo.toml` 中提供特性开关：

| Feature | 默认启用 | 描述 |
| :--- | :---: | :--- |
| `serde` | 是 | 为核心数据结构实现 `Serialize` 与 `Deserialize`。 |

> 本 crate 仅支持 `std`（`wasm32-unknown-unknown` 亦走 `std`），不提供 `no_std` 构建。
| `parallel` | 否 | 启用 `rayon` 多线程并行加速（如空间顶点焊接与大批量细分）。 |

---

## 5. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
