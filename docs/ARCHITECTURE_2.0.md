# 2.0 分层与命名约定

2.0 只负责**体素世界**与**方块实体**。连续空间实体（画作、物品展示框、盔甲架等）在
`release/2.1` 分支上保留，未来以**独立附加包**的形式回归，不进入核心。

## 分层

| 层 | crate / 模块 | 允许知道什么 | 明确禁止 |
|---|---|---|---|
| L0 体素 | `mtk-voxel` | 方块状态、biome、section、网格；**通过 `BlockEntityTextureProvider` 只消费宿主提供的方块实体运行时贴图** | 任何实体概念、网络访问、贴图烘焙、皮肤缓存 |
| L1 模型 | `mtk-model::block_entity` | 方块实体 NBT → 几何（陶罐/旗帜/头颅） | 连续空间坐标 |
| L1 模型 | `mtk-model::builtin::props` | blockstate → 程序化几何（铜像） | 方块实体 NBT 之外的运行时数据 |
| L2 同步 | `mtk-sync` | 协议 v4：体素 + 方块实体流（0x11/0x12） | 0x13/0x14（为实体包保留） |
| L3 宿主 | `libmtk` / `bindings` / Blender | 装配、材质、图集、UI | 绕过 L1 直接解释 NBT |

## 命名规则（强制）

- **禁止单独使用 `entity`**。一律写成 `block_entity` 或 `continuous_entity`。
- `mtk-model::block_entity` 只装**由方块实体数据驱动**的几何；`builtin::props` 只装
  **由 blockstate 驱动**的程序化几何。
- 协议包名以 `BlockEntity` 结尾时，编号必须落在 0x11/0x12；0x13/0x14 永久预留给实体包。
- 自定义属性（Blender）沿用 `mtk:is_*` 前缀；方块实体与实体包各自命名空间，不共用。

## 方块实体的运行时贴图（玩家头颅 / 复合旗帜）

`mtk-voxel` 只定义 `BlockEntityTextureProvider` 接口：网格化前询问一次「这个世界需要哪些运行时精灵」，
拿到结果后合并进 `config.atlas_address_map`，网格化后由宿主读取 RGBA chunk 做材质绑定。
**皮肤下载、磁盘缓存、横幅烘焙都在宿主层**（`mtk-block-entity-textures` crate，由 `mtk-py` /
live-sync 客户端注入）。`mtk-voxel` 不再持有 `SkinResolver` / `BannerBaker` / `DynamicAtlasManager`。

## 铜像为什么留在核心里

`copper_golem_statue` 的几何是 `blockstate`（氧化等级 + 姿态 + 朝向）的纯函数，方块位于体素
网格上、随选区一起流式传输，因此它不是连续实体。它同时具备"方块实体 + 方块"双重身份，
这正是 `builtin::props` 存在的理由：**方块几何生成器**，与实体包无关。

## 实体附加包的接入点（未来）

1. 新 crate `mtk-entity`（独立仓库/包），提供 `EntityInstance`、渲染计划、程序化生成器。
2. `mtk-sync` 增加 0x13/0x14 与能力位；`PROTOCOL_VERSION` 升 5.0。
3. 宿主（Blender 插件）以可选扩展加载；未安装时协议能力位置 0。
4. 核心不得为了实体包新增分支：L0–L3 的表都不应出现 `EntityInstance`。
