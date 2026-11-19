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

## 关键约束：块模型空间是 y 向上

视觉验证结论：**原版 `ModelPart.Cube` 的数值不能直接套用**——纹理全部反了，双箱构建也错位了。

原因是空间约定不同：

- 原版模型空间 **y 向下**，渲染器再 `pose.scale(-1, -1, 1)`；
- 我们的 `BlockModelJson` 烘焙器工作在 **y 向上、+z 向南** 的方块空间。

把原版 box 搬到后者需要三步变换（MiEx 一直在做，所以它"看起来对"）：

1. **UV 置换**：`up ↔ down`、`north ↔ south`，`east/west` 不变；矩形按 v 升序存。
2. **z 镜像**：`z' = 16 - z`（单箱的锁舌因此从 z 15..16 变到 z 0..1，这正是面朝北时锁舌应在的一侧）。
3. **双箱层名互换**：`_left` 贴图配原版 `createDoubleBodyRightLayer` 的几何，反之亦然；且双箱**不屏蔽**
   贴合面（MiEx 输出 18 个面，屏蔽会少 3 个）。

另外 `from`/`to` 不是简单的 min/max：`from = (max_x, max_y, min_z)`、`to = (min_x, min_y, max_z)`，
烘焙器据此推导面绕向，写反会导致背面剔除错误（这也是"纹理反了"的一半原因）。

因此**验收标准是：原生生成器的输出必须与 MiEx 逐字节一致**。金色快照
`crates/mtk-model/tests/fixtures/builtin_models_golden.json` 就是这样来的，`test_builtin_model_golden.rs`
对全部状态做严格比较，不做任何跳过。

## 分层

- `builtin/props/cube.rs`（已落地）：`CubeSpec { origin, dimensions, tex_offs, mirror }`，
  `face_uvs`（原版 y-down 公式）、`model_face_uvs`（y-up 置换 + 矩形升序）、`model_corners`
  （`from`/`to` 顺序）、`visible_faces_except`。所有族共用。
- `builtin/props/<family>.rs`：每族一个生成器，输入 `BlockState` + 可选 `BlockEntityData`，
  输出 `BlockModelJson`（继续复用现有模型烘焙器，不重复实现 UV/面朝向逻辑）。

## 批次

| 批次 | 内容 | 验收 |
|---|---|---|
| **P1** ✅ | chest（含 trapped/ender/copper、single/left/right） | 与 MiEx 快照逐字节一致 + 5 个结构/UV 断言 |
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
