# MiEx 专有表达式模型的清除计划

## 为什么删

`crates/mtk-model/assets/builtins/` 里 49 KB 的 JSON 用的是 MiEx 的私有方言：`variables` /
`condition` / 字符串拼接 / `thisBlock.state.*` / `thisBlock.name.contains(...)`。它不是 Minecraft
的模型格式，也不是任何通用格式。代价：

- 需要维护一个 DSL 解释器（`builtin/evaluator/` 757 行）和补丁层（`builtin/patch.rs` 204 行 +
  `patches/miex_patches.json`）；
- 几何来源是"某个人对原版渲染器的转写"，无法和原版源码逐条对齐；
- 未来接 GeckoLib / Blockbench 动画模型时，两套"用表达式描述几何"的机制会互相污染。

原版客户端这些方块的几何本来就不在 JSON 里：它们由 `BlockEntityRenderer` + `ModelPart` 在代码里
构建。我们有 `Minecraft_Source`，可以直接转写成 Rust 程序化生成器——铜像
（`builtin/props/copper_statue/`）已经是这个模式。

## 现有覆盖（MiEx 处理的 7 族）

| 族 | 状态轴 | 原版几何权威 |
|---|---|---|
| chest | `facing` × `type=single/left/right`，4 种材质（normal/trapped/ender/copper） | `ChestModel` + `ChestRenderer` |
| bed | `facing` × `part=head/foot`，16 色 | `BedRenderer` / `BedModel` |
| shulker_box | `facing`，16 色 + 默认 | `ShulkerBoxModel` + `ShulkerBoxRenderer` |
| sign | `rotation`(16) / wall `facing`，各木材 | `SignModel` / `SignRenderer` |
| hanging_sign | `attached` × `rotation`(16) / wall `facing` | `HangingSignModel` + `HangingSignRenderer` |
| skull | 8 种 × `rotation`(16) / wall `facing`，玩家皮肤 | `SkullBlockRenderer` + 各 `*Model` |
| end_portal | 无 | `EndPortalRenderer`（整面特殊渲染） |

原版 jar 里这些方块的 `models/block/*.json` 只有 `particle` 贴图（占位），所以**生成器是生产
路径**，不是可选优化。

不在 MiEx 里的（已经是原生 Rust 或纯 JSON）：`decorated_pot`、`banner`、`bell`、`conduit`、
`end_gateway`（`builtin/vanilla/*.rs`）。

## 已发现：MiEx 的 UV 约定与原版不一致

以 chest 主体为例（`w,h,d = 14,10,14`，`texOffs(0,19)`，64×64 贴图，除以 4 得到 0..16 模型空间）：

| 面 | 原版 `ModelPart.Cube` | MiEx 产出 |
|---|---|---|
| down | `(3.5, 4.75, 7.0, 8.25)` | `(7.0, 4.75, 10.5, 8.25)` ← 原版的 up |
| up | `(7.0, 8.25, 10.5, 4.75)` | `(3.5, 4.75, 7.0, 8.25)` ← 原版的 down |
| west / east | 一致 | 一致 |
| north | `(3.5, 8.25, 7.0, 10.75)` | `(10.5, 8.25, 14.0, 10.75)` ← 原版的 south |
| south | `(10.5, 8.25, 14.0, 10.75)` | `(3.5, 8.25, 7.0, 10.75)` ← 原版的 north |

即 MiEx 用的是 **y/z 镜像的模型空间**：`up/down` 与 `north/south` 的贴图区被互换。箱体上这四
个面恰好是同款木纹或被遮挡，所以看不出来；**床、告示牌、头颅这些非对称模型必须逐面验证**，不能
假设"现在看着对就是对的"。

补充证据（贴图实测，26.2-Fabric.jar）：

| 贴图 | west 区 `(0..14, 33..43)` | east 区 `(28..42, 33..43)` |
|---|---|---|
| `entity/chest/normal.png` | 100% 不透明 | 100% 不透明 |
| `entity/chest/normal_left.png` | **0% 不透明（全透明）** | 100% 不透明 |
| `entity/chest/normal_right.png` | 100% 不透明 | **7% 不透明（几乎全透明）** |

这正是原版对双箱隐藏 `allOfEnumExcept(WEST/EAST)` 的原因：那一侧的贴图区本来就是空的。所以
转换必须保留这个屏蔽，否则会多出一块透明面（MiEx 是把几何镜像后改用对侧贴图来"绕开"的）。

因此转换不能以"和 MiEx 逐字节相同"为验收标准。验收标准：

1. 结构测试：盒子数量、包围盒、可见面掩码、贴图 id；
2. UV 断言：非对称面（头颅的脸、床的床头、告示牌的正面）必须落在原版贴图的对应区域；
3. 视觉抽查：Blender 里注入后与预期朝向一致。

## 分层

- `builtin/props/cube.rs`（已落地）：原版 `ModelPart.Cube` / `CubeDefinition` 的转写。
  `CubeSpec { origin, dimensions, tex_offs, mirror }` + `face_uvs(face)` + `visible_faces_except`。
  所有族共用，避免每族重写 UV 展开。
- `builtin/props/<family>.rs`：每族一个生成器，输入 `BlockState` + 可选 `BlockEntityData`，
  输出 `BlockModelJson`（继续复用现有模型烘焙器，不重复实现 UV/面朝向逻辑）。

## 批次

| 批次 | 内容 | 验收 |
|---|---|---|
| **P1** | chest（含 trapped/ender/copper、single/left/right） | 结构测试 + UV 断言（double 的隐藏面）+ 金色快照比对差异已解释 |
| **P2** | shulker_box（16 色）、bed（16 色 × head/foot） | UV 断言床头/床尾、颜色 → 贴图映射 |
| **P3** | sign、hanging_sign（standing/wall、木材映射） | 木材质 → `entity/signs/<wood>`、正面 UV |
| **P4** | skull（8 类 × 16 旋转 + wall + 玩家皮肤） | 每类模型盒数、脸的 UV 区、玩家 profile 贴图 |
| **P5** | end_portal | 单面/整块的朝向 |
| **P6** | 删除 `builtin/evaluator/`、`builtin/loader.rs`、`builtin/patch.rs`、`assets/builtins/**`（含 `LICENSE-MIEX`）、`MIEX_*` 常量；更新 `docs/API_REFERENCE.md` | `rg -i miex` 零命中；全部方块烘出非零几何 |

每批独立提交，`crates/mtk-model/tests/test_builtin_model_golden.rs` 的 `MIGRATED` 列表随之增加
一行，从而金色快照只守护"尚未迁移"的族。

## 顺带修掉的缺陷

- MiEx 的 y/z 镜像约定（见上）在非对称模型上是潜在错贴图；
- `builtin/evaluator/expr.rs` 的字符串拼接与 `contains` 判断是"按 id 猜材质"，移除后改为显式映射表。
