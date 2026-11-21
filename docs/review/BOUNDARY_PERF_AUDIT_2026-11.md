# 跨 crate 边界 / 性能 / 基准有效性 审查

日期：2026-11-20 · 基线：`libmozitoolkit 3843e15` / `MoziToolKit de8a080`
配套：`CODE_REVIEW_2026-11.md`、`FIX_BACKLOG.md`（已按 §4 的三分法重写）

**进度**：B0（清单重划 + 资产发现/跳过门禁）、B1（删 `mtk-bench`、`mtk-cli bench world`、
宿主场景基准、面板双指标）、B2（外围性能）与 B2b（存档解码 / biome 元数据 / 重复注入）已落地；
前后实测见 §9、§10。

本轮不改代码，只做三件事：

1. 用真实资产 + 真实存档把"体素构建到底多快"测出来，并拆到阶段；
2. 用代码事实把 `FIX_BACKLOG` 的 190+ 条重新划一遍（已修 / 因 2.0 作废 / 仍开放）；
3. 深挖 `mtk-material` / `mtk-resource` / `mtk-texture` 与 crate 之间的边界。

---

## 0. 结论摘要

| 判断 | 依据 |
|---|---|
| **网格内核没有退化**，反而比历史里程碑快 5–6 倍 | 实测 `parallel_meshing` = 23.0 ms / 336,741 quads ≈ **13.6M quads/s**；历史提交 `3e08dc4` 记录的是 0.6M→2M quads/s |
| 用户感知的 "4.5M/s" 是**原生 mesh+assemble 的 quads/s**，端到端只有 **1.49M quads/s** | 真实存档导入：原生 140 ms、端到端 226 ms |
| 真正的瓶颈在管线外围，不在逐体素循环 | `extract_padded` 21.8 ms、`assemble` 18.2 ms、Blender 注入 39.1 ms、Blender 材质绑定 45.6 ms |
| `mtk-bench` 可以删，但它**从未测过真实路径**（`ModelSource::None` + 合成稠密段），所以删掉不会丢失任何现有信号 | 五个 bench 全部如此；CI 不跑它；只有 4 处文档引用 |
| Blender 面板压测**不是**场景压测 | 它测的是 debug world（稀疏、每段 1 格、96,612 quads、不含注入/材质），且资产未命中缓存时 `get_world_pipeline_assets` 会静默返回 `None` 再每次重解析 300–400 ms |
| `FIX_BACKLOG` 已经不能直接照着做 | 2.0 转向（`81ba2aa`）删掉连续实体后，B0 的 M-05、B3 的 Y-04/Y-05/Y-12/Y-15、B7 的 J-\*、H-01..H-03 等已作废；另有一批标 ✅ 的条目实际仍未修（T-01、S-03/S-04 状态列） |

**建议的下一步不是"再来一轮全量审查"，而是四件确定的事**：重划清单 → 删 bench 并补真实场景基准 → 打掉三个外围性能热点 → 修 2.0 相关仍开放的边界/正确性项。见 §8。

---

## 1. 实测：真实存档 + 真实资产

环境：Blender 5.2.3 LTS（headless），`Minecraft_Source`（26.2）+ 三包 `.ci-cache`
（18 atlas chunks / 20,550 models / CTM 规则齐），存档 `mtk-save-world`
选择 `(-64,-64,-64) .. (512,320,512)`。

**624 sections / 336,741 quads / 336,229 verts**，热态（第三次，`tools/blender/scenario_bench.py` 实测）：

| 阶段 | ms | 占比 | 说明 |
|---|---:|---:|---|
| `load_region_chunks` | 60.7 | 26.8% | Anvil 解压 + NBT → storage |
| `extract_padded` | 23.3 | 10.3% | 18³ padded 数组 + biome 列 + 方块实体 |
| `parallel_meshing` | 24.1 | 10.7% | 内核（含模型/图集/CTM/biome） |
| `cache_insertion` | 0.1 | 0.0% | |
| `assemble`（含全量 weld） | 14.2 | 6.3% | 336k 顶点空间哈希去重 |
| `blender_mesh_injection` | 34.6 | 15.3% | DNA 直写 |
| `blender_material_binding` | 46.6 | 20.6% | 热态；只有 7 个 chunk |
| **合计（用户体感）** | **226** | 100% | **1.49M quads/s** |

冷启动（第一次）另有两笔一次性成本，不能混进稳态：`material binding` **3,243 ms**
（创建 shader/节点组 + 载入 18 张图集纹理），`parallel_meshing` 34.8 ms。
如果用户"第一次导入觉得慢"，主因是这里而不是网格。

同一 world 只跑 `rebuild_all`（热态，min of 3）：

| 配置 | 总 ms | quads/s | extract | meshing | assemble |
|---|---:|---:|---:|---:|---:|
| 全开（models+CTM+atlas+biome） | 70.6 | 4.77M | 24.8 | 24.8 | 15.6 |
| 关 CTM | 64.0 | 5.26M | 24.6 | 20.7 | 15.3 |
| 关 models | 66.6 | 5.02M | 23.8 | 24.6 | 15.0 |
| 关 biome/colormap | 61.3 | 5.49M | 23.8 | 19.8 | 14.9 |
| 关 CTM + biome | 56.5 | 5.96M | 25.0 | 15.0 | 15.0 |
| 关 AO | 66.9 | 5.04M | 24.6 | 24.8 | 14.6 |
| 关 weld | 64.0 | 5.26M | 26.4 | 23.7 | 9.2 |

读法：

- **内核 13.6M quads/s**（336,741 ÷ 24.8 ms），全开 vs 全关只差 65%，说明模型/CTM/biome 的
  单面成本可控；
- **`extract_padded` 与配置无关**（23.8–26.4 ms 恒定），它是纯数据搬运开销，有独立的优化路径；
- **`assemble` 的全量 weld 占 15–25 ms**，而增量路径（`merge_welded_sections_filtered`）
  只需要 weld 段与段的接缝，实测关 weld 时 assemble 从 15.6→9.2 ms，说明 weld 占到 6–7 ms；
  但注意：全量 weld 实际是**重复劳动**（每段已经在 `SectionMesher` 内部 weld 过）。
- **Blender 侧占端到端 36%**（39+46），是第二大块。

### 1.1 复现方式

新增脚本 `libmozitoolkit/tools/blender/scenario_bench.py`（本轮新增，见 §8-B1）：

```
blender --background --factory-startup --python tools/blender/scenario_bench.py -- \
    --assets ../Minecraft_Source \
    --cache ../MoziToolKit/.ci-cache/mtk_fp_58d0c7fee81ad3bf.mtkcache \
    --save ../mtk-save-world --bounds -64,-64,-64,512,320,512 \
    --iterations 3 --inject
```

注意 `--cache` 的选择会影响数字：上面的 3 包缓存（含 CTM）是 336,741 quads；
只有原版包的缓存是 335,641 quads、无 CTM。基准必须固定缓存文件，否则趋势不可比。

---

## 2. 为什么"以前很快，现在 4.5M/s"

三个不同量纲被混用了：

| 数字 | 来源 | 量纲 | 是否可比 |
|---|---|---|---|
| 40–60M | `mtk-bench` `bench_4000_chunks` | **voxels/s**，`ModelSource::None`，稠密合成段 | 否 |
| 4.5M | Blender `Mesher Benchmark` 面板 | **quads/s**，debug world，只算原生 mesh | 半可比 |
| 1.5M | 真实存档导入端到端 | **quads/s**，含 I/O / 注入 / 材质 | 用户真正体感的 |

4.5M 这个数本身是**对的**（我实测 4.77M），但它只覆盖管线中 9.7% 的那一段。所以"掉速"的
叙事应该改成："内核没掉，外围从来没人量"。

### 2.1 确定性的四个外围热点（按收益排序）

1. **biome 列平滑**（`storage/world/padded.rs:91-160`）
   每个 section 计算 256（2D）或 1024（3D）列，每列走 `BIOME_KERNEL_R2` 25 个采样点，
   每个采样点做一次 `HashMap<[i32;2], String>` 查询 + 边界 clamp；
   `get_smoothed_column_biome`（`biome/color.rs:108`）还在**每列新建一个
   `HashMap<String, …>` 并 `insert(b_name.to_string())`**。
   624 section × 256 列 × 25 采样 ≈ 4M 次哈希 + 数百次 String 分配/秒级。
   **修法**：按世界（或按 (x,z)/quart）预计算平滑列色，section 提取时只 memcpy；
   平滑函数改用 `&str` key 的复用缓存。预期 `extract_padded` → <5 ms。

2. **全量 weld**（`world/assembly.rs:56-72`）
   `rebuild_all` 一律 `merge_welded_sections`（全量空间哈希）。增量路径已有
   `section_seam_predicate` + `merge_welded_sections_filtered`，语义等价（每段内部已 weld，
   UV 是 per-loop 所以按位置合并安全）。全量路径应当复用同一谓词，省 6–7 ms/次。

3. **Blender 材质绑定重复写 material_index**（`utils/materials/builder/world_binding.py:143-200`）
   注入阶段 `inject_face_materials_dna` 已经把 `face_materials` 写进 `material_index`；
   绑定阶段又读回 `mtk_atlas_chunk_id` 全表、`np.unique`、再 `foreach_set` 一次。
   7 个 chunk 花 45 ms，和 chunk 数无关，纯粹是 336k 面的读写。
   **修法**：注入后 `material_index` 已就位 → 身份映射直接 return；
   仅在槽位重排时做 remap。

4. **资产解析不进缓存**（`bridge/world/assets.py:69-92` 的 `_WORLD_PIPELINE_ASSETS_CACHE`）
   memo 条件是 `fp is not None`。指纹为 `None`（缓存缺失/包不匹配）时**每调用一次**就
   `load_models` + `load_atlas` + `load_biome`，实测 330–400 ms（`load_models` 383 ms /
   `load_atlas` 173 ms），并且返回 `(None, None, None)` 让世界**用回退立方体**继续烘。
   两个问题分开修：① 失败要显式报错（operator 已有 guard，bridge API 层没有）；
   ② 成功路径的 memo key 不要绑在指纹上（绑 pkg path+mtime）。

### 2.2 逐体素/逐面的分配（次要但便宜）

| 位置 | 问题 |
|---|---|
| `mesher/section_mesher.rs:133,172` | `is_non_mesh_placeholder_block` 每体素调 **2 次**，而它内部 `to_ascii_lowercase()` → 每次一个 `String`（每 section 最多 8k 次）。且结果只取决于 blockstate → 应作为 `ResolvedPaletteEntry` 字段预计算 |
| `mesher/palette_entry.rs:86-88` | 流体邻居 `get_state` 返回 `String`（`.to_string()` / `"minecraft:air".to_string()`），每个流体面采样多次 |
| `mesher/shading/resolver.rs:156-162` | CTM 分支每面构造 `String` `"block/<name>"` + `ResourceLocation`，再进 `CtmSolver::resolve_face` 再 `extract_block_name` + `canonical_name.to_string()`。实测 CTM 让 meshing +20% |
| `mtk-material/src/resolver/mod.rs:262` | `MaterialResolver::resolve` 第一行 `clean_identifier` = 2–4 次 `String`；每个 blockstate × 6 方向 × 2–4 候选 |
| `mesher/delta_mesher.rs:38` | **增量重建完全不共享 palette 缓存**：每个脏段重新 `mesh_section`（`palette_cache=None`），一次编辑 8 段 → 8 倍解析。应复用 `batch.rs` 的 `palette_cache` |

### 2.3 顶点法线（V-02，仍未修）

`mesher/grid_pool.rs:57` 的去重键只有坐标（`(gx*17+gy)*17+gz`），`normals` 取先到者。
默认 `weld_vertices: true`，所以每个共享角点的 3 个面只有 1 个法线正确。
Blender 侧 `use_smooth=False` 时用面法线，**当前无可见影响**；但 glTF/其他 DCC/开 smooth
就会错。修法代价 3× 顶点，建议：加 `weld_normals: bool`（默认 false = 保持现状并显式承认），
或让网格 key 带 6 向法线索引。

---

## 3. `mtk-bench`：可以删，但先补位

### 3.1 事实

- 5 个 bench 全部 `|_st| ModelSource::None`（`bench_4000_chunks.rs:73,111`、
  `bench_pipeline_stages.rs` 同），即**没有模型、没有图集、没有 CTM、没有 biome tint**；
- 反序列化/缓存路径都不测；
- `.github/workflows/ci.yml` 不跑它；`tools/ci_guardrails.py` 只把它归类为 `/benches/`；
- 外部引用 4 处：`Cargo.toml`、根 `README.md:46`、`docs/ARCHITECTURE.md:24,101`、
  `docs/API_REFERENCE.md:27`、`docs/BINDINGS_DESIGN.md:24`、`crates/mtk-cli/README.md:22`；
- 它能编译（`cargo check -p mtk-bench --all-targets` 通过）。

### 3.2 结论

**删。** 它测的是一条生产上不存在的路径（"无模型"），数字无法用于回归判断，却会被当成
"我们的体素速度"引用——这正是这次误判的来源。

删除面（一次性，10 分钟内可完成）：

1. `crates/mtk-bench/` 整目录；
2. `Cargo.toml` 的 workspace member 与 `mtk-cli`/`README`/`docs` 6 处文字；
3. 顺手清理 `.VSCodeCounter/` 的旧统计（可忽略）。

### 3.3 补位（重要）

删掉之后必须有"真实场景基准"，否则下次还是只能靠体感。建议两级：

- **Rust 级**：`mtk-cli bench world --jar <26.2-Fabric.jar> --world <save>`
  输出 §1 的六阶段与端到端 quads/s（可进 CI 的非阻断 job，做趋势）；
- **宿主级**：本轮新增的 `tools/blender/scenario_bench.py`（真实资产 + 真实存档 + 注入 +
  材质绑定），即 §1 的数字来源。把 Blender 面板的 "Mesher Benchmark" 改成调用它，
  或至少在面板上并列显示"原生 / 端到端"两个数，避免再次混淆量纲。

---

## 4. `FIX_BACKLOG` 与提交的实际对账

`81ba2aa`（2.0 转向）之后，清单里有相当一部分条目**不再属于 2.0**。以下是按代码事实重新划的
三类（只列需要决策的，已修的不再赘述）。

### 4.1 已修（我本轮复核到代码/测试）

`M-01..M-04`、`M-07..M-12`、`S-01/S-02/S-05..S-10/S-14..S-17`、
`S-11`（`PyBufferOwner.live_views` + 变更加锁）、`S-12`（`PyBUF_FORMAT|C_CONTIGUOUS` + itemsize 校验）、
`V-03..V-06`、`Y-01/Y-02`（`advance_sequence` 统一计数器）、`Y-03`（`rel_pos` 越界丢弃 + Warning）、
`K-01`（目录包/zip/memory 都实现 `content_entries`，且有 fingerprint 一致性测试）、
`K-02`（`package/source.rs:100` 现在校验 `format_version`）、
`A-01/A-02`、`T-08`、`J-05/J-06`、以及被推翻的 `V-01`/`M-06`/`M-13`。

### 4.2 因 2.0 作废（别再花时间）

- `M-05`、`V-09`：实体渲染计划随 `81ba2aa` 删除；
- `Y-04`（baseline 期间 staging）、`Y-05`（baseline 删除事件）、`Y-12`/`Y-15`（visual entity 载荷）：
  0x13/0x14 已删，`PROTOCOL_VERSION=4`；
- `J-01..J-04`/`J-07`、`H-01..H-03`、`H-11..H-14`（`utils/entities/` 已不在 2.0 插件里）：
  归 `release/2.1`，不进 2.0 列表。

### 4.3 仍开放，且与 2.0 直接相关

| ID | 复核结果 |
|---|---|
| **S-13** | `bindings/mtk-jni/src/server.rs` 仍是 `if handle == 0` 八处，无句柄表/关闭态原子标志 → 关闭竞态 UAF 未修 |
| **T-01（Rust 侧）** | `mtk-testkit/src/lib.rs:72-84` 的 `real_assets_root` **只找 `../mc` 和 `mc`**，没有 `../Minecraft_Source`。本机不设 `MTK_TEST_ASSETS` 时所有真实资产测试静默走 vendored fixtures（或 skip），与 AGENT.md 冲突。CI 靠 env 绕过，本机没有 |
| **T-01（Python 侧）** | `MoziToolKit/tests/_assets.py:46-56` 同样只找 `mc`/`Minecraft_Unpacked` |
| **Y-06** | `server/service/server.rs:175` 现在是 `snapshot_uuid: [0u8; 16]` —— 从"恒定近似值"变成了**恒定全零**，旧 baseline chunk 仍然会被新流接受 |
| **Y-07** | `server.rs:53,152,157,170,214,237,...` 仍有 7 处 `.lock().unwrap()` |
| **Y-08** | transport 的 `cmd_receiver` 跨重连复用，断开时不清队列 → 旧连接的 ACK/RESYNC 发给新连接 |
| **Y-10 / Y-11 / Y-13 / Y-14** | 未动（`space_left` 的 max、`block_states[i]` 无长度校验、`fetch_add` 与 broadcast 未同临界区、HELLO 能力位） |
| **V-02** | 见 §2.3 |
| **V-07** | `fluid/height.rs` 仍有两套高度公式（`:85` 与 `:252`） |
| **V-08** | `mesher/section_mesher.rs:42` 仍按 `padded.is_empty` 早退；空气格里的方块实体无几何 |
| **C-01** | `.mcmeta` 的 `frames` 仍只解析不消费（`meta.rs:56`；`extract_animation_metrics` 只用 `frametime`/`interpolate`） |
| **C-02 / C-03** | `collector.rs:176-190` 仍是 `String::contains` + 顺序相关；CTM 同优先级排序仍依赖枚举序 |
| **C-04 / C-05** | `prebake/engine.rs` 的 `blocks` 分类仍会烘全部图集；进度 total 与 `inc()` 仍不同源 |
| **C-06** | 见 §6.3，坐实 |
| **K-03** | `prebake/resolver.rs` 仍需复核（本轮未逐行看，但 Python 侧已改为"无指纹即 None"，风险降低） |
| **K-04** | `AtlasAddressMap.sprites/anim_sprites/sprite_sources` 是 `HashMap`，直接 serde → 两次烘焙字节不同（§6.3） |
| **K-05** | `prebake/models.rs:101` 仍 `model_cache.insert(loc.path.clone(), …)` 插短名并回退 |
| **K-06** | `mtk-resource/src/pack/source.rs:176` 仍 `fs::read(..).ok()`（IO 错误 = 文件不存在） |
| **K-08..K-12** | 未动 |
| **A-03** | `mtk-voxel/src/storage/world/debug.rs:21` 仍是进程级 `static CACHED_DEBUG_STORAGE` |
| **A-04** | `world/dynamic_entities.rs` 仍 async(`:130`) / sync(`:268`) 双实现 |
| **A-05..A-17** | 未动（`mtk-py` 的 `inner()/inner_mut()` 仍 `pub`，`buffer.rs:11` 仍有 `#[allow(dead_code)]`） |
| **B6 其余** | `T-02`（`tests/_assets.py` import 时就设 `MTK_TEST_SANDBOX=1`）、`T-03/T-05/T-06/T-07` 未动 |

### 4.4 清单本身的三个问题

1. **状态列不可信**：`S-03/S-04`、`M-03` 写完"✅ DONE"但状态列仍是 `◻`；
   反过来 `T-01` 标 ✅ 而代码没做（§4.3）。建议把 `FIX_BACKLOG` 降级为历史记录，
   以"代码事实 + 测试名"为准。
2. **缺"作废"标记**：2.0 转向没有回写到清单，下一个 agent 会去修已删除的文件。
3. **缺收益/成本字段**：性能项（C-06、K-04）和崩溃项（S-13、Y-06）同样标 P1/P2，
   实际风险差一个数量级。

---

## 5. 跨 crate 边界与内部设计

### 5.1 分层文档与代码不一致（最需要先决策）

`docs/ARCHITECTURE_2.0.md` 写：

> L0 体素 `mtk-voxel`：允许知道 方块状态/biome/section/网格，**明确禁止任何实体概念**。

实际 `mtk-voxel` 里：

- `world/mod.rs:54-58` `VoxelWorld` 直接持有 `DynamicAtlasManager`、`SkinResolver`
  （`mtk-resource::skin`，带 ureq HTTP）、`BannerBaker`；
- `mesher/section_mesher.rs:455` 直接调 `mtk_model::BuiltinModelRegistry::bake_block_entity`
  → L0 反向知道 L1 的几何生成；
- `world/dynamic_entities.rs` 477 行做玩家皮肤解析/下载/别名。

两种收敛方式（择一，别继续并行）：

- **A（推荐）**：在 `mtk-voxel` 定义 `trait BlockEntityMeshSource { fn bake(&self, state, data) -> Option<BakedModel> }`
  + `trait DynamicTextureSink`，由 `libmtk`（L3）实现并注入；`mtk-voxel` 不再依赖
  `mtk-model`/`mtk-resource::skin`/`mtk-texture::BannerBaker`。
- **B**：承认现状，把文档改成"L0 允许方块实体装饰，但不得有连续空间实体"。

A 的收益是顺带解决 `A-04`（把 async/sync 双实现挪到宿主层一起收敛）和网络栈下沉问题。

### 5.2 `mtk-cli` 自带一套资产读取（双实现）

`crates/mtk-cli/src/utils/loader.rs` 用裸 `zip::ZipArchive` + 自己的缓存重新实现了
blockstate/model 读取，`commands/{bench,export,model}.rs` 全走它。问题：

- 不受 `S-07` 的 entry 大小/路径穿越加固约束（拿恶意包跑 `mtk model` 可打爆内存）；
- 不认 `.mcmeta`/`atlas` 分层/`pack` 叠加 → CLI 输出与生产预烘**会漂**；
- 与 `mtk-resource` 的 `ResourcePack`/`ResourcePackStack` 是第二套实现，正是
  `CODE_REVIEW_2026-11.md §1 模式 A` 的 crate 级翻版。

建议：`mtk-cli` 改为消费 `libmtk::prebake` + `mtk-resource`，把 `UniversalAssetLoader`
删除。这也是删 `mtk-bench` 后补 `mtk-cli bench world` 的自然落点。

### 5.3 依赖图上的具体问题

| 问题 | 位置 | 影响 |
|---|---|---|
| `mtk-resource` 默认开 `net`（ureq+rustls） | `Cargo.toml` `default = ["parallel","zip","net"]` | `mtk-texture`/`mtk-material`/`mtk-model`/`mtk-voxel` 全部传递链接 HTTP/TLS 栈；只有 Blender 的皮肤需要。建议 `net` 默认关，由 `libmtk`（或独立 `mtk-skin`）显式开 |
| `mtk-resource` 的 `parallel` 是空 feature | `parallel = []` | 5 个 crate 在转发它（`mtk-material/parallel`、`mtk-texture/parallel`、`mtk-voxel/parallel`…），实际什么都不做 → 死配置 |
| 目录遍历实现两份 | `pack/source.rs:143-175` 与 `pack/stack/mod.rs:260-305` | 二者**必须逐字节一致**才能让主机指纹与加载栈一致；只有 1 个测试覆盖单目录场景。建议抽一个 `content_entries_from_dir` 并加"两路等价"测试 |
| `mtk-py` 绕过 `libmtk` facade 直接依赖 12 个 crate | `bindings/mtk-py/Cargo.toml` | 绑定层可以绕过 facade 直接调内部 API，是 `S-11/S-12/A-17` 类问题反复出现的原因。建议逐 crate 收敛（先 `mtk-texture`/`mtk-material`） |
| `mtk-save` → `mtk-model` + `mtk-voxel` | `mtk-save/Cargo.toml` | 读存档拖动模型几何 + mesher。若只为 `BlockEntityData`，把该数据类型下沉到 `mtk-core` 或独立 `mtk-nbt` 可显著减依赖面 |
| `mtk-package` 同时被 `libmtk` 与 `mtk-py` 直接依赖 | 两者的 `Cargo.toml` | 刚在 `3843e15` 合并了两套 profile/reader，值得补一条"绑定层只经 libmtk"的 guardrail |

### 5.4 `ci_guardrails.py` 的盲区

现在它守住了：模块私有、绑定层无 `unwrap`、无死根导出、文档不宣传退休符号。
**没有**守住（都是本轮实际踩到的类）：

- crate 依赖方向/白名单（现在只有 `mtk-blender-dna` 一条 shell grep）；
- 序列化确定性（`HashMap` 进 serde）；
- 同一能力两份实现（`content_entries`、`UniversalAssetLoader`、banner、fluid height）；
- 性能回归（无基线）。

建议加两条低成本规则：
① 依赖白名单表（`crate → 允许依赖`）+ `cargo metadata` 校验；
② `HashMap`/`HashSet` 不允许出现在 `AtlasAddressMap`、`*Metadata` 这类会被序列化的 `pub struct` 字段
（正则 + allow 清单），或直接用"同输入烘两次字节相等"的集成测试守住。

---

## 6. 深挖：`mtk-material` / `mtk-resource` / `mtk-texture`

### 6.1 `mtk-material`（5,949 行）

**结构**：`biome/{canonical_data,hardcoded,palettes,colormap,resolver}` + `pbr/{labpbr,preset,bake}`
+ `remap/` + `resolver/`。依赖 `mtk-texture`（`RgbaBuffer`/`AtlasAddressMap`）与 `mtk-resource`，
方向正确，无环（PBR 类型放 texture、解码放 material，注释也写明了理由）。

坑：

1. **66 条 `CANONICAL_BIOMES` 是手抄表**（`canonical_data.rs`，864 行），jar 里正好也有
   66 个 `data/minecraft/worldgen/biome/*.json`，但**没有任何一致性测试**。
   MC 版本一升（26.3 加 biome / 改 water color）就会静默漂。建议：
   ① 立刻加"遍历真实 jar 的 biome JSON，逐个比对 id 与 `grass/foliage/water_color`"的测试；
   ② 中期改为烘焙时从 jar 生成（和 `AtlasDefinition` 一个待遇）。
2. **未知 biome 静默回落 plains**：`palettes.rs:161-174` 找不到就 `default_biome_palette()`。
   这是"模式 B：失败降级成看起来正常的数据"，新维度/模组 biome 会全图 Plains 染色。
   建议返回 `Option` 或计数上报（`BiomeResolver` 那条路已经有自定义 biome 的概念）。
3. **`precomputed_index()` 是指针区间算术**（`palettes.rs:87-99`）：靠
   `CANONICAL_BIOMES.as_ptr()` + `size_of::<BiomePalette>()` 判断"这个引用是不是静态表里的"。
   安全但不是好设计：一旦有人 `Box`/`Vec::from` 一份 palette、或结构体字段顺序变化带来
   padding 差异，就会静默走 fallback 路径（结果仍对，但 `LazyLock` 的预计算失效）。
   建议给 `BiomePalette` 加 `index: Option<u16>` 或改用 `phf` 表。
4. **热路径线性扫描**：`get_biome_palette` 对 66 条做 `eq_ignore_ascii_case`。它被
   `get_smoothed_column_biome`（每列 1–3 次）放大，是 §2.1-1 的一部分。建议 `phf`/静态排序二分。
5. `HARDCODED_BLOCK_TINTS` 只有 15 条（birch/spruce/lily_pad/stem/redstone），
   而 vanilla 1.21 的 `BlockColors` 还有 `attached_*`、`double_plant`、`reed` 等；
   建议同样从 jar/反编译表生成并加断言，别手工维护。
6. PBR：`labpbr.rs` 有 `IOR_LUT` 与文档（`pbr_channel_spec.md`），`test_pbr_decode.rs` 有覆盖；
   但 `pbr/preset.rs`（Bedrock MER / MetallicRoughness）与 `mtk-texture::PbrSourceFormat`
   之间没有交叉一致性测试。建议补一个"每个 preset ↔ 每个 enum 分支都可往返"的测试。

### 6.2 `mtk-resource`（6,811 行）

**结构**：`pack/`（source/stack/collector）+ `atlas/`（definition/category）+ `ctm/`
+ `meta` + `identifier` + `texture_key` + `skin/` + `probe/`。

坑：

1. **`K-06` 仍在**：`pack/source.rs:176` `fs::read(...).ok()`。
   权限错误/IO 错误 = "文件不存在" → 静默用低优先级包的贴图。这条被推迟了两轮，
   但它是"我改了包却没生效"的另一半来源。做法：`ResourcePack::open` 改 `Result<Option<Vec<u8>>>`
   （缺文件 = `Ok(None)`，IO/权限 = `Err`），`ResourcePackStack` 全链路传播。
2. **`.mcmeta.frames` 解析但不消费**（C-01）：`meta.rs:6-56` 完整建模了
   `AnimationFrame::{Index, Detailed{index,time}}`，`sprite.rs` 也存了 `animation`，
   但烘焙与运行时只用 `frametime`/`interpolate`/`frame_count`（`extract_animation_metrics`）。
   自定义帧序（例如 `frames:[0,2,1,3]`）会按 0,1,2,3 播。
   要么在 `anim_atlas` 里按 `frames` 重排 strip，要么删掉 `frames` 字段（规则 9）。
3. **`AtlasSource::Filter` 用 `contains` 且顺序相关**（C-02）：`collector.rs:176-190`
   对 `namespace`/`path` 用 `String::contains`，而且 `results.retain` 依赖处理顺序。
   `minecraft:atlas` 的过滤器语义是**精确前缀/正则**，且源之间是有序覆盖。建议按
   `AtlasFilterPattern` 做两阶段（先收集所有 source，再按 pack 优先级覆盖），并用
   `starts_with`/`glob`。
4. **CTM 解析每面重建字符串**（§2.2）。`CtmSolver::resolve_face` 还先 `extract_block_name`
   再 `format!("{}:{}")`。建议：
   ① `CtmSolver` 提供 `has_rules_for_block(&str)`，在 palette 解析期缓存；
   ② `resolve_face` 接受预切好的 `&str` block 名 + 复用 `SmallVec`/`String` 缓冲。
   实测 CTM 让 meshing +20%（24.8 vs 20.7 ms）。
5. **skin 在 resource crate 里做网络**：`skin/{fetch,mirror,resolver,index}` 用 `ureq` +
   sha512 镜像校验。这是"资源读取"与"网络 I/O"混层。建议独立 `mtk-skin` crate
   （或至少 `net` feature 默认关，见 §5.3）。
6. `probe/`（classify_dir / classify_file / mod）与 `pack` 的读取逻辑重复；
   `probe` 只服务于主机"这个路径是不是原版资产"，可以并进 `pack::probe`。

### 6.3 `mtk-texture`（4,561 行）

**结构**：`stitcher/`（二叉分割 + 区域）+ `image/`（解码/alpha/调色板/padding）+ `atlas/`
（builder static/anim/paletted + address_map + dynamic + pbr meta + banner）。

坑（这一组是本轮最实的）：

1. **生产 padding = 0（C-06 坐实）**
   `libmtk/src/prebake/engine.rs:247-251` 与 `scene/exporter/builder.rs:99-103` 都写死
   `mip_level: 0, padding: 0`；`AtlasBuilderConfig::default` 也是 0。
   而 `static_atlas.rs:186-195` 的 UV 正好落在精灵边界：
   `u_min = inner_x / width`、`u_max = (inner_x + fw) / width`（`inner_x = slot.x + padding`）。
   后果：图集里精灵**紧挨着**排，Blender 侧默认开 mipmap/双线性时会在精灵边界采样到邻居
   （HD 包、细线、树叶更明显）。`padding.rs` 的实现是对的，只是没人打开。
   修法：`padding = 2..=4` 且 `mip_level` 保证 `padding >= 2^mip`，或维持 0 但把 UV 内缩
   半个纹素（`0.5/width`）。二者选一，并补一个"边界像素不渗色"的测试。
2. **`AtlasAddressMap` 的 `HashMap` 直接 serde（K-04）**
   `address_map/mod.rs:21-33`：`sprites`/`anim_sprites`/`sprite_sources` 都是
   `HashMap<ResourceLocation, _>`，`to_json`（`:192`）直接序列化 → **同输入两次烘焙字节不同**。
   缓存以内容哈希/指纹为键时就是"每次都 STALE"，也是 `T-03`（`glob[0]` 测试不确定性）的根。
   另外 `texture_id_counter` 的分配顺序来自 `Vec<DecodedSprite>` 的顺序（collector 侧），
   与地址表的迭代序无关，但两者一旦被人当成同序使用就会错位。
   修法：`BTreeMap` 或 `#[serde(serialize_with = "sorted_map")]`，并加"烘两次字节相等"的测试。
3. **动态图集回绕会留下幽灵映射**
   `dynamic/chunk.rs:141-156` 在 256 槽用尽后 round-robin 复用槽位，并
   `slot_map.retain(|_, v| v != s || v < pinned)` 清掉旧键；
   但 `dynamic/manager.rs:101-131` 的 `key_to_location` **没有同步清理**。
   结果：被逐出的皮肤 key 仍然 `manager.contains_key() == true`、
   `key_to_location` 仍指向已被覆盖的槽 → 该皮肤永远不会重新 blit，看到的是别人的皮肤。
   修法：`allocate_slot` 回绕时把被覆盖槽位对应的 key 从 `key_to_location` 里一并删除
   （或干脆用 `slot_map` 作为唯一事实源，删掉 manager 里的重复表）。
4. **`MAX_DYNAMIC_CHUNKS = 16` + `base = 0xFFF0` → 最后一个 chunk id 是 `0xFFFF`**
   `0xFFFF` 是 `u16::MAX`，同时 Python 侧 `DYNAMIC_CHUNK_END = 0xFFFF` 把它当"动态区间上界"，
   而网格/材质里 `chunk_id` 还有 `i32`/`u32` 的转换。建议 `MAX_DYNAMIC_CHUNKS = 15`
   （0xFFF0..0xFFFE），把 `0xFFFF` 永久留作哨兵。
5. **`lookup_animated_str` 对非 `block/` 路径是线性扫描**（`address_map/mod.rs:88-115`）
   遍历 `anim_sprites` 并做 4 种路径前缀比较。动画纹理数量不大（水/岩浆/火/传送门/…），
   目前可接受，但它是"每个动画面"的路径，建议先按 namespace 建二级索引。
6. **图集内存没有全局预算**：每个 chunk 最多 4096²×(albedo+normal+orm+aux) ≈ 268 MB，
   `atlas_chunks = 18` 时理论峰值 >4 GB（实际取决于每类尺寸与是否存在 PBR 伴随）。
   本轮 `load_atlas` 实测 173 ms（说明没有全部展开/或 chunk 尺寸小），但**没有断言**。
   建议在 `AtlasBuilder` 里加总字节预算并在超限时报错，而不是等 OOM。
7. `BannerBaker` 在文本 crate，而 `mtk-resource::banner` 提供图案/dye 表——分工是合理的；
   但 `BannerBaker::pattern_sprites` 是 `HashMap<String, RgbaBuffer>`，图案注册顺序影响合成
   结果，且没有"未知图案"的告警（同样属于模式 B）。

---

## 7. 测试体系（决定上面会不会复发）

1. **本机 `cargo test` 不碰真实资产**：`mtk-testkit::real_assets_root()` 只认
   `../mc` / `mc`（`lib.rs:72-84`），本机资产在 `../Minecraft_Source`。
   于是 `has_real_assets() == false`，唯一用它的测试
   （`mtk-model/tests/test_item_generator_integration.rs:24`）直接走 else 分支。
   `FIX_BACKLOG` 里 `T-01 ✅` 的"增加 `../Minecraft_Source` 自动探测"**并未落地**。
   Python 侧 `tests/_assets.py:46-56` 同样缺。→ 一行候选加两处，成本极低，收益最大。
2. **Windows 与本地仍靠"跳过"**：`.github/workflows/ci.yml` 的 Windows job 注释明写
   "skip gracefully when the real Minecraft pack is unavailable"，与 AGENT.md
   的"缺资产视为失败"冲突。至少要让"跳过"在输出里可见（`cargo test` 汇总里打印
   `skipped_real_asset=NN`），否则永远全绿。
3. **没有"同输入两次烘焙字节相等"的测试** → `K-04` 这类确定性缺陷只能靠人眼。
4. **没有真实的"场景基准"回归**（§3.3）→ 性能只能靠体感，本次误判的直接原因。
5. `T-02`（`tests/_assets.py` import 时设 `MTK_TEST_SANDBOX=1`）仍在：指纹校验路径
   在 Python 套件里从不被覆盖。

---

## 8. 建议的执行顺序（每批可独立验证）

**B0 · 校准（0.5 天，无风险）**
- 重写 `FIX_BACKLOG`：加"作废"分区、把状态列改成"代码事实 + 测试名"、删掉重复的 D1–D5 记录；
- `mtk-testkit::real_assets_root` / `tests/_assets.py` 增加 `../Minecraft_Source` 探测；
- 验收：本机 `cargo test --workspace` 输出里真实资产测试数量 > 0。

**B1 · 基准（1 天）**
- 删 `crates/mtk-bench` + 6 处文档引用；
- 落地 `tools/blender/scenario_bench.py`（本轮已加）与 `mtk-cli bench world`；
- Blender 面板改为显示"原生 / 端到端"两列，或直接调场景基准；
- 验收：同一存档连续两次输出稳定的阶段表。

**B2 · 外围性能（已完成，见 §9）**
- biome 列预采样网格（`padded.rs` + `biome/color.rs`）→ `extract_padded` 24.3 → 6.5 ms
- `rebuild_all` 统一 seam-filtered weld → `assemble` 17.9 → 14.0 ms
- `is_non_mesh_placeholder_block` 去分配 + 进 `ResolvedPaletteEntry`；CTM 按 blockstate 预判
- `DeltaMesher` 复用 palette cache
- Blender `ensure_world_materials` 增加"注入即最终槽位"快路径
- `get_world_pipeline_assets` 改为廉价签名 memo + 缺资产告警
- 结算：内核（mesh+assemble）70.5 → 42.0 ms（8.03M quads/s，+68%）；端到端 226 → 178 ms。
  未达 ≤130 ms 的原因是 save I/O（~60 ms）与 Blender 注入（~38 ms）本就不在 mesher 内，见 §9.3。

**B3 · 边界（2 天）**
- `mtk-voxel` 去实体化（trait 注入，见 §5.1-A），顺带收敛 `A-04`；
- `mtk-cli` 删除 `UniversalAssetLoader`，改走 `libmtk::prebake`；
- `net` feature 默认关 + `mtk-resource/parallel` 空 feature 清理；
- 两个 `content_entries` 目录遍历合一 + 等价测试；
- `ci_guardrails.py` 加依赖白名单。

**B4 · 三个 crate 的坑（3 天）**
- `mtk-texture`：C-06 padding/mip 决策 + UV 内缩、K-04 `BTreeMap`、动态图集回绕、chunk id 上界、图集内存预算；
- `mtk-resource`：K-06 fallible pack API、C-01 frames 消费或删除、C-02 filter 两阶段；
- `mtk-material`：CANONICAL_BIOMES/jar 一致性测试、未知 biome 不再静默 Plains、`phf` 查找。

**B5 · 剩余安全/正确性（2 天）**
`S-13`（JNI 句柄表）、`Y-06`（真 UUID）、`Y-07`（去毒化）、`Y-08`（断开清队列）、
`Y-10/Y-11/Y-13`、`V-07`、`V-08`、`K-05`、`K-08..K-12`、`A-03/A-05..A-17`。

> 依赖：B0 → B1 → B2 可并行 B3；B4 与 B5 之间无依赖。

---

## 附：本轮新增/可复现文件

- `tools/blender/scenario_bench.py`：真实资产 + 真实存档的场景基准（§1 数据来源）。
- 本文件。

---

## 9. B2 落地实测（2026-11-20）

基准命令（同一份缓存 + 同一份存档 + 同一 bounds）：

```
mtk bench world --cache ../MoziToolKit/.ci-cache/mtk_fp_58d0c7fee81ad3bf.mtkcache \
    --save ../mtk-save-world --bounds -64,-64,-64,512,320,512 --iterations 9 --threads 8

blender --background --factory-startup --python tools/blender/scenario_bench.py -- \
    --assets ../Minecraft_Source --cache <同上> --save ../mtk-save-world \
    --bounds -64,-64,-64,512,320,512 --iterations 5 --inject
```

### 9.1 内核（624 sections / 336,741 quads，8 线程，best of 9）

| 阶段 | 之前 | 之后 | 变化 |
|---|---:|---:|---:|
| `extract_padded` | 24.3 ms | **6.5 ms** | −73% |
| `parallel_meshing` | 27.1 ms | **20.8 ms** | −23% |
| `assemble` | 17.9 ms | **14.0 ms** | −22% |
| 合计 | 70.5 ms | **42.0 ms** | **−40%（4.77M → 8.03M quads/s）** |

### 9.2 端到端（含 save 读取、Blender 注入、材质绑定，best of 5）

| 阶段 | 之前 | 之后 | 变化 |
|---|---:|---:|---:|
| `load_region_chunks` | ~60 ms | ~59 ms | I/O，未动 |
| `mesh_voxel_world` | ~70 ms | **~44 ms** | −37% |
| `blender_mesh_injection` | ~35 ms | ~38 ms | 未动 |
| `blender_material_binding`（热） | **45.6 ms** | **20.5 ms** | −55% |
| 端到端 | **226 ms** | **178 ms** | **−21%（1.49M → 1.89M quads/s）** |
| 材质绑定（冷启动） | 3.2 s | 3.2 s | 未动（建 shader + 载图集） |

### 9.3 为什么没有到 ≤130 ms

剩余 178 ms 的构成：save I/O ~59、网格 ~44、注入 ~38、材质 ~21、桥接/配置 ~16。
其中 **97 ms 不在 mesher 里**：

1. `load_region_chunks` 读的是外置卷上的真实存档，`mtk-save` 的逐块直读已在此前的批次优化过；
   再往下要动 Anvil 并行解压（独立批次）。
2. `blender_mesh_injection` 是把 ~70 MB 顶点/属性写进 Blender DNA 通道的固有成本；
   要动得先削减 per-face 属性（`biome_tint_color` 64 B/face、`anim_*` 24 B/face）。
3. `blender_material_binding` 的 20.5 ms 是热路径；3.2 s 的冷启动是建材质 + 载 18 张图集，
   属"首次导入"体验，需要单独的懒加载/共享材质方案。

### 9.4 本批新增的回归测试

| 测试 | 守住什么 |
|---|---|
| `biome::color::tests::kernel_is_a_uniform_5x5_box` | 预采样网格只在 5×5 均匀核下有效 |
| `biome::color::tests::grid_path_matches_the_closure_path` | 网格路径与逐点路径逐位一致 |
| `mtk-voxel/tests/test_biome_columns.rs` | 2D / 3D section 的 `biome_data` 与参考实现完全一致，含跨段边界 clamp |
| `world::assembly::tests::seam_only_weld_matches_a_full_weld` | 只焊接缝 == 全量 weld（位置/法线/索引/材质） |
| `ctm::solver::tests::applies_to_state_*` | CTM 预判按 block 名 / base tile 两种键精确匹配 |
| `test_ctm_meshing::unrelated_ctm_rules_leave_the_mesh_unchanged` | 跳过无关 CTM 不改变任何网格字节 |
| `mtk-cli bench::tests::bounds_*` | 真实场景基准的输入校验 |

---

## 10. B2b 落地实测（2026-11-20）

B2 之后端到端还停在 178 ms，其中约 97 ms 在 mesher 之外。本轮先量后改：

### 10.1 定位（改之前的实测）

`load_region_chunks` 116 ms，在 `loader.rs` 里打点拆开：

| 子阶段 | 时间 |
|---|---:|
| `read_chunk_decompressed`（seek + read + inflate） | 64.3 ms |
| `ChunkParser::parse_chunk` | 10.9 ms |
| chunk 循环其它（`set_section` / 方块实体过滤） | ~3 ms |
| **`finalize_biome_metadata`** | **38.1 ms** |

`blender_mesh_injection` 35–45 ms，逐调用计时：

| 调用 | 时间 |
|---|---:|
| `inject_topology_dna`（`clear_geometry` + 3×`add()` + 直写） | 20.4 ms |
| `mesh.update(calc_edges=True)` | 7.6 ms |
| `inject_uvs_dna` | 2.7 ms |
| `inject_custom_attributes`（**全部 10 个属性**） | **2.2 ms** |
| `inject_color_attributes_dna` / `inject_face_materials_dna` | 0.9 ms |

**修正上一轮的错误判断**：per-face 属性瘦身没有价值——全部自定义属性加起来只有 2.2 ms
（DNA 直写本来就是 memcpy）。真正的成本是 Blender 的几何分配与 edge 重建。

### 10.2 三项改动

1. **`finalize_biome_metadata` 单遍化**（37.9 → ~8 ms；其中 7.5 ms 是最终 `column_map` 的
   33 万次插入，无法避免）。旧实现为每列从最高层向下扫 section（576×576 列 × 24 层 ≈ 800 万次
   HashMap 查询）；新实现每个 section 只贡献自己 16 列的 `qy=3` 生物群系，按 `sec_y` 高者胜出，
   临时数组 6 B/列（576×576 选区 ≈ 2 MB，且被既有的 `String` 值 map 掩盖）。
   `biome_meta.rs::tests` 保留旧算法作为 oracle，对"混合高度/缺段/边界裁剪/无 bounds"逐列比对。

2. **Anvil 并行解码**（读+解析 75 → ~15 ms；`load_region_chunks` 116 → 59.7 ms）。
   以 region 文件为任务单位 `par_iter`：每个任务自己 `RegionFile::open`（头表已在 open 时读入），
   解压 + NBT 解析 + 方块实体裁剪全部在 worker 内完成，结果按 plan 顺序收集后**顺序**合入
   storage（坐标互斥，结果与顺序无关）。进度节流器保持原子计数。
   `parallel_region_decoding_is_scheduling_independent` 用 1 线程 / 8 线程两个 pool 各加载一次，
   断言 section 数、bounds、`biome_column_map`、`primary_biome` 与抽样方块完全一致。

3. **重复注入复用拓扑**（35–38 → 27 ms；其中几何分配部分 20.4 → 0）。
   `inject_mesh_data` 现在先比较 `(vertices, loops, polygons)` 三个计数；一致且走 DNA 时，
   `inject_topology_dna(reuse_existing=True)` 跳过 `clear_geometry` + 三次 `add()`，直接重写
   顶点/corner-vert/loop-start 缓冲；同时 edge 列表不变，`update(calc_edges=True)` 也一并跳过。
   计数不一致（拓扑真变了）走原路径。`tests/test_mesh_injection_reuse.py` 用
   `clean_zombie_attributes` 调用次数分别钉住"复用"与"重分配"两条分支。
   首次导入仍需 20 ms 分配（Blender 地板），这部分不省。

### 10.3 结果

| 阶段 | B2 后 | B2b 后 |
|---|---:|---:|
| `load_region_chunks`（端到端测量） | ~59 ms | **~22.6 ms** |
| `blender_mesh_injection`（重复） | ~38 ms | **~27 ms** |
| `mesh_voxel_world` | ~44 ms | ~42 ms |
| `blender_material_binding`（热） | ~20.5 ms | ~21.5 ms |
| **端到端 best** | **178 ms** | **131.2 ms**（2.57M quads/s） |

冷启动单列：首次导入的 `blender_mesh_injection` 361 ms（拓扑分配 + edge + 首次属性层创建），
之后稳定在 27–29 ms；材质绑定冷启动 279 ms、热 21 ms。这两项属"首次导入"，与稳态无关。

### 10.4 没做 / 不做的

- **P4「延后 edge 构建」**：复用路径已经天然跳过；首次导入保留 `calc_edges=True`，
  因为硬边分析（`analyze_mesh_hard_edges`）会读 `mesh.edges`，延后只是把 7.6 ms 挪个位置。
- **属性瘦身**：实测总收益 2.2 ms，不做（见 10.1 的修正）。
- **材质冷启动 3.2 s → 279 ms 观察到的差异**来自缓存命中（图集 PNG 已在 Blender 里），
  不是代码路径变化；真正的首次成本仍未优化，需要图集纹理懒加载/跨会话复用。

### 10.5 回归：并行加载把真实宿主卡死（GIL 死锁）

**现象**：B2b 提交后，在真实 Blender 里导入存档立即整体卡死（不是变慢，是进程无响应、无输出）。

**根因**（与多线程本身无关）：

```
Python 后台线程（AsyncTask）
  └─ mtk.load_and_mesh_minecraft_save(...)        ← 持有 GIL，未 detach
       └─ rayon par_iter（P2 新增）
            └─ ProgressThrottler::inc()
                 └─ 进度回调 → Python::attach()   ← 在 rayon worker 上申请 GIL
```

`bindings/mtk-py/src/save.rs` 在加载阶段**没有** `py.detach`（网格阶段有）。改动前加载是单线程，
回调在同一条线程上重入 GIL 是允许的；改成 rayon 并行后，worker 申请 GIL，而持有 GIL 的宿主线程
正阻塞在 `par_iter().collect()` 里等 worker —— 互等，整进程冻结。

**为什么测试没抓到**：`test_save_bridge.py::test_bounded_mesh_save_with_progress` 用的是
2×2 chunk 的小选区，rayon 会直接在调用线程上跑完，不会真正派发到线程池；只有真实导入
（1296 chunk）才会踩到。

**修法**：

1. `save.rs` 的 `load_region_chunks` 包进 `py.detach`（与网格阶段一致）。
2. 新加载器**不再从 worker 调用宿主回调**：进度在合并完成后由调用线程 `inc_by(total_chunks)`
   报一次（该阶段现在 ~20 ms，一次 100% 是诚实的）。网格阶段仍由 worker 报进度，但它在
   `py.detach` 内，且宿主的回调本身就是线程安全的队列投递。
3. 守卫测试 `progress_callback_runs_on_the_calling_thread`：用 576×384×576 的选区（足以
   迫使 rayon 真正并行）断言回调的 `ThreadId` 始终等于调用线程。**已验证它在旧实现上失败**
   （108 次 off-thread 回调），在新实现上通过。

**验证**：模拟宿主的后台线程 + 回调导入同一存档——修复前进程 120 s 无输出被强杀（exit 124），
修复后 ~95 ms 返回、107 次进度事件。

**教训**：任何"宿主回调 + 原生线程池"的组合都必须在**足够大的负载**下测试线程亲和性，
小样本会被 rayon 的调用线程执行优化掩盖。

---

## 11. 线程治理（T-1 已落地，T-2/T-3 待做）

起因：B2b 的并行加载在真实宿主里触发 GIL 死锁（见 §10.5）。复盘结论是"契约没写下来"，
而不是"多线程本身危险"。因此新增一条治理线：

| 步 | 内容 | 状态 |
|---|---|---|
| **T-1** | 门禁 + 绑定层边界 helper + 契约文档 | ✅ 本轮 |
| **T-2** | `mtk-core::threading`（缓存池 + 进程级线程覆盖）+ 13 个 rayon 站点收敛 + `libmtk_py.set_thread_count/threading_info` | ✅ 本轮 |
| **T-3** | 三个回调入口的大负载线程亲和测试（save/mesh/prebake）+ 绑定契约测试 | 待做 |

### T-1 落地内容

- `tools/ci_guardrails.py::check_threaded_bindings_detach()`：函数级扫描
  `bindings/mtk-py/src`，任何 `Option<ProgressCallback…>` 变量的**使用点**、以及任何
  `par_iter`/`into_par_iter`，都必须落在 `run_detached(...)`/`py.detach(...)` 的区间内。
  带一个能正确跳注释/字符串/原始字符串的括号匹配器，避免 `format!("{x}")` 的干扰。
  **已验证**：对旧版 `save.rs`（加载不 detach）报错 1 条；对临时注入的
  `par_iter` 无 detach 探针报错 1 条；对当前树通过。
- `bindings/mtk-py/src/threading.rs::run_detached`：唯一认可的 GIL 释放边界（文档 + 门禁锚点）。
- `bindings/mtk-py/src/progress.rs::PyProgressCallback`：把两份重复的 dict 构造/attach 逻辑
  收敛成一处，并把"回调可能来自任意线程、宿主必须自行 marshal"写在类型旁边。
- `docs/BINDINGS_DESIGN.md §4`：线程与 GIL 契约、门禁实现、线程预算。

### 已定但未实现的策略（T-2 输入）

- 默认线程数 = `max(1, available_parallelism() - 2)`（给宿主与系统留两个核）；
- 用户显式指定时以用户值为准（插件偏好 `thread_count`）；
- 目标：一次 `set_thread_count` 让**所有** rayon 站点生效，而不是现在的 5/13。

### T-2 落地内容（2026-11-20）

**一处策略 + 一处池**：`mtk-core/src/threading.rs`（`constants::concurrency` 已删除并入）。

- 默认线程数 `max(1, available_parallelism() - 2)`（用户决定，给宿主与后台程序留两核）；
  `set_thread_override(n)` 覆盖，`0` 复位；`threading_info()` 可观测。
- `install(…)` / `install_with(Some(n), …)` 按线程数缓存 `Arc<ThreadPool>`（上限 4），
  `execute_parallel` 变成它的薄包装 —— 不再每次调用 `ThreadPoolBuilder::build()`
  （实测单次建池 ~130 µs；一次世界重建原本要建两个）。
- 嵌套保护：若当前已在 rayon 区域内（`current_thread_index().is_some()`）则直接内联执行，
  避免 `prebake → bake_batch` 这类嵌套再建一个池造成的线程爆炸。
- 收敛的站点：`mtk-voxel`(2)、`libmtk`(engine + models×3)、`mtk-model`、
  `mtk-material`、`mtk-texture`(2)、`mtk-core/weld`、`mtk-voxel/dynamic_entities`、
  `mtk-save`(存档解码) —— 13 个并行入口现在共享同一个预算。
- `mtk-model` 的 `determine_conservative_threads()` 从"固定上限 4"改为跟随进程策略。
- `bindings/mtk-py`：`set_thread_count(0=auto)`、`threading_info()`；`MoziToolKit` 在
  addon `register()` 与偏好变更时各推一次（`bridge/engine.py::apply_thread_count`），
  偏好描述与面板文案同步成"auto = cores - 2"。

**实测**：本机 10 核（4P+6E）默认 8 workers。`mtk bench world` 默认策略下
42.0 → **40.7 ms**（8.27M quads/s，反而比用满 10 核略快，P/E 混跑更少）；
端到端 `scenario_bench` 130.6 ms，与 T-1 后持平。

---

## 12. C1：两个 P0 + 动态图集 + T-3（2026-11-20）

性能与线程批完成后，先把两件会真正"炸"的联机缺陷清掉，再处理一处会一直看错的画质 bug。

### 12.1 S-13 · JNI 句柄 UAF（P0）

旧实现把 `Box<LiveSyncServer>` 的裸指针当 `jlong` 发给 JVM，每个入口 `unsafe { &*(handle as *const _) }`。
Java 侧 poller 线程读句柄、主线程 `nativeServerStop` 释放 → use-after-free，JVM 无法阻止。

改为 `bindings/mtk-jni/src/handle.rs::HandleTable<T>`：

- 句柄是自增 id，表里存 `Arc<T>`；每次调用 `get()` 克隆一个 `Arc`，正在执行的调用在 stop 之后仍然安全；
- `remove()` 之后句柄不再解析，重复 stop 是 no-op，之后所有调用走"未运行"返回值而不是读已释放内存；
- 锁被 panic 毒化时用 `PoisonError::into_inner` 恢复，不会让整个 JNI 面永久 panic（原来 server.rs 里 7 处 `.lock().unwrap()` 也不复存在）。

`LiveSyncServer::stop` 由 `&mut self` 改为 `&self`（`listener_thread` 移进 `Mutex`），这样共享 `Arc` 也能停。
测试（无需 JVM）：句柄唯一且可解析、**stop 后 in-flight Arc 仍有效且句柄不再解析**、毒化后仍可服务。

### 12.2 Y-06 · baseline 的 `snapshot_uuid` 恒定（P0）

`submit_baseline` 用 `Instant::now().elapsed().as_nanos()` 生成快照 id —— 这是"刚创建的 Instant 到现在的纳秒数"，
实际恒为 0 附近。客户端正是靠这个 id 丢弃"上一个中断的 baseline 的迟到分片"，于是**重连/移动选区后会混入旧流的分片**。

改为 `ServerInner::next_snapshot_uuid()`：单调计数器 + 服务器启动 uuid（宿主传全零时由 `start` 生成）
经 splitmix64 混淆，低 8 字节放计数器保证会话内绝不重复，高 8 字节与进程身份相关避免重启后首包撞车。
测试：同会话 1024 次不重复、两个"进程"首包不同、相邻计数器哈希半区不同。

### 12.3 动态图集回绕的幽灵映射（P1，画质）

`DynamicAtlasManager` 在 16 个 chunk 全满后于最后一个 chunk 内轮转复用槽位。
`DynamicAtlasChunk::allocate_slot` 会清掉自己 `slot_map` 里旧 key，但 manager 的 `key_to_location` **没同步**：
被逐出的玩家皮肤仍然 `contains_key == true`、仍解析到已被别人重画的槽位，并且永远不会再 blit ——
"某个人的头颅一直显示成别人的皮肤，重启才好"。

修法：`DynamicAtlasChunk::will_wrap()` 暴露"下一次分配会回收槽位"，manager 在回收时把该 (chunk, slot) 的旧 key
从索引里一起删掉。测试把 16×256 个槽位填满后分配第 4097 个 key，断言被逐出的 key 不再解析、
不出现在 `all_slot_locations()`、也不会被重新注册进图集地址表。

### 12.4 T-3 · 线程契约的"另一半"

save 侧已经断言"回调只在调用线程"；网格化相反——它**故意**从 rayon worker 上报进度（宿主负责 marshal）。
新增 `mtk-voxel/tests/test_mesher_threading.rs`：32 段实心世界 + 4 线程，断言有进度事件、
同一阶段内 `current` 不倒退、并且（多核时）**至少有一个事件来自非调用线程**。
这样"回调可能来自任意线程"就从隐含假设变成了被测试钉住的契约。

prebake 路径暂未单测（它走同一套 `ProgressCallback`，且 guardrail 已在绑定侧守住 detach）。

### 12.5 门禁

`fmt` / `ci_guardrails`(0) / `clippy -D warnings` / `MTK_REQUIRE_REAL_ASSETS=1 cargo test --workspace`
（0 失败 0 SKIP）/ `cargo build-all-py` / Blender 全套件 591 passed。

### 12.6 下一批的优先级（我的建议）

| 顺序 | 批次 | 理由 |
|---|---|---|
| 1 | **B3 边界** | `mtk-voxel` 直接持有 `SkinResolver`(HTTP)/`BannerBaker`/模型烘焙，与 `ARCHITECTURE_2.0` 的 L0 禁令矛盾；`mtk-cli` 还自带一套未加固的 zip loader。越晚改越贵。 |
| 2 | **C-06 + K-04**（mtk-texture） | 图集 `padding=0` 的 HD/mipmap 渗色，以及 `HashMap` 进 serde 导致的非确定性字节；两项都要动烘焙输出，需要一次缓存格式版本递增，适合一起做。 |
| 3 | **B4 其余** | `mtk-resource` 的 fallible pack API（K-06）、`.mcmeta.frames`（C-01）、filter 顺序（C-02）；`mtk-material` 的 66 biome 与 jar 一致性测试。 |
| 4 | **B5 剩余** | Y-07/Y-08/Y-10/Y-11/Y-13、V-07/V-08、K-05/K-08..K-12、A-03/A-05..A-17。 |

---

## 13. B3 第 1 步：`mtk-voxel` 去实体化（2026-11-20）

### 13.1 问题

`ARCHITECTURE_2.0` 的 L0 表写着 `mtk-voxel` **禁止任何实体概念**，但 `VoxelWorld` 实际持有：

- `mtk_texture::DynamicAtlasManager`（动态图集分配）
- `mtk_resource::skin::SkinResolver`（**HTTP 下载 + 磁盘缓存**）
- `mtk_texture::BannerBaker`（旗帜程序化烘焙）
- 以及 479 行的扫描/解析/回调逻辑（async 与 sync 两份近似实现，即 A-04）

L0 因此间接依赖了网络栈与图像烘焙，任何想替换皮肤来源或禁用网络的主机都无法做到。

### 13.2 切法

`mtk-voxel` 只保留一个接口：

```rust
pub trait BlockEntityTextureProvider: Send + Sync {
    fn prepare(&mut self, storage: &mut VoxelStorage, atlas: &mut AtlasAddressMap,
               sections: Option<&[IVec3]>, mode: TextureResolveMode,
               progress: Option<ProgressCallback>) -> TexturePrepareOutcome;
    fn atlases(&self) -> Vec<(u16, &[u8])>;
    fn slots(&self) -> HashMap<String, (u16, u16)>;
    fn set_skin_cache_dir(&mut self, Option<PathBuf>);
    fn set_banner_baker(&mut self, BannerBaker);
    fn set_skin_resolved_callback(&mut self, SkinResolvedCallback);
}
```

- `VoxelWorld` 持有 `Option<Box<dyn BlockEntityTextureProvider>>`，构造时为空（`None` = 没有运行时贴图），
  由宿主 `set_block_entity_textures(...)` 注入；`rebuild_all` 的 prep 阶段调用一次 `prepare`。
- `SkinResolvedCallback` / `TextureResolveMode` / `TexturePrepareOutcome` 与 trait 同居一处。
- 新增 crate **`mtk-block-entity-textures`**：实现上述 trait，把原来的扫描逻辑合并成**一份**
  （A-04 的双实现收敛：`Cached` 走后台队列、`Blocking` 走并行阻塞，共用同一段扫描），
  持有 `DynamicAtlasManager` / `SkinResolver` / `BannerBaker`。
- `MesherConfig.banner_baker` 字段删除（它从来不是"mesher 配置"，而是宿主贴图来源）；
  `mtk-py` 的 `PyMesherConfig` 自己保存 baker，并在构造 world 时交给 provider。
- 注入点：`mtk-py`（`PyVoxelWorld` 构造函数 / save 导入）、`mtk-sync`（live-sync 客户端的 world）、
  `mtk-cli bench world`（保持"真实场景"）。

### 13.3 行为保持一致的一个坑（值得记录）

把"预热 16 个原版旗帜槽位"从构造器搬到 `prepare` 之后，`test_ctm_bridge` 有 3 个用例失败：
standalone 材质模式（`atlas=None`）下，provider 的首个 `prepare` 会把自己的动态图集合成一个
**只有动态精灵的** `atlas_address_map`，于是 CTM 分支看到 `Some(atlas)` 但在里面查不到 CTM 子图 →
静默退回基础贴图。

修法是让"发布规则"显式化：

```
atlas_changed && (原有 atlas 存在 || 本次真的新增了动态精灵)
```

即：**已经有图集的世界**把 provider 的精灵（含预热的旗帜）合并进去；**没有图集的世界**
（standalone）只有当真的出现动态精灵时才生成图集。这样既保留了旧构造器的预热行为，
又不改变 `atlas_address_map: None` 的语义。

### 13.4 测试搬迁与新增

| 测试 | 位置 | 说明 |
|---|---|---|
| `test_player_skull_dynamic_atlas_meshing_and_uv_remapping` | `mtk-block-entity-textures/tests` | 内存皮肤 → 动态图集 → 网格 UV 全部断言 |
| `test_banner_dynamic_atlas_meshing_and_baking` | 同上 | 复合旗帜烘焙 + 槽位 UV |
| `test_plain_banner_without_block_entity_meshing` | 同上 | 无方块实体的纯旗帜依赖 provider 的预热槽位 |
| `mtk-voxel` 的 `test_mesher` | 保持不变 | 方块实体几何（陶罐/头颅/铜像）仍通过 `BuiltinModelRegistry` 默认路径覆盖 |

这三个测试原先直接访问 `world.skin_resolver` / `world.dynamic_entities`，搬迁后改用公开面
（`set_block_entity_textures` + `prepare_block_entity_textures` + `dynamic_entity_slots/atlases`），
顺带证明宿主的公开入口足够用。

### 13.5 结果

- `mtk-voxel` 不再引用 `mtk_resource::skin`、`mtk_texture::DynamicAtlasManager`、
  `mtk_texture::BannerBaker`（仍使用 `AtlasAddressMap` 等数据类型的来自 `mtk-texture`）。
- `VoxelWorld` 的 `Debug`/`Clone` derive 去掉（provider 不可克隆，克隆一个世界本来也没有语义）。
- 门禁全绿：fmt / guardrails(0) / clippy -D warnings / `MTK_REQUIRE_REAL_ASSETS=1 cargo test --workspace`
  （0 失败 0 SKIP）/ build-all-py / Blender 全套件 591 passed。
- 场景基准端到端 130–143 ms（机器噪声区间内，无回归）。

### 13.6 B3 剩余

1. ✅ `mtk-cli` 删除 `UniversalAssetLoader`，改走 `mtk-resource`（见 §14）；
2. `mtk-resource` 的 `net` feature 默认关、`parallel` 空 feature 清理；
3. ✅ 两处 `content_entries` 目录遍历合一（见 §16）；
4. `ci_guardrails` 依赖白名单（防止下一个 crate 反向依赖）。

---

## 14. B3 第 2 步：`mtk-cli` 不再自带资产读取器（2026-11-20）

### 14.1 问题

`crates/mtk-cli/src/utils/loader.rs` 是一套手写的 zip/目录读取：

- 直接用 `zip::ZipArchive`，**绕过** `mtk-resource` 的单 entry 256 MiB 上限与路径穿越过滤（S-07
  只加固了 `ZipPack`，CLI 这条路径没有）；
- 只认 `assets/minecraft/...`（jar 模式连命名空间都不保留），不认多包叠加、`.mcmeta`、`atlases` 分层；
- 与生产预烘各自解析同一份 JSON，任何一边的模型查找规则变化都会让 CLI 与 Blender 的结果漂移。

### 14.2 改法

新增 `crates/mtk-cli/src/utils/assets.rs::AssetStack`，只是 `ResourcePackStack` 之上的一层薄胶水：

- 目录 → `DirectoryPack`，`.zip`/`.jar` → `ZipPack::from_file`（都走上限/穿越加固）；
- `list_all_blockstates()` = `list_all_blockstate_locations()` 排序去重（多命名空间）；
- `load_blockstate` / `load_model` 只负责取字节 + `serde_json` 解析 + memo；
  模型路径解析交给 `ResourceLocation::model_candidate_asset_paths`；
- 删除 `UniversalAssetLoader`（173 行）与 `mtk-cli` 对 `zip` 的普通依赖（测试仍用它造包）。

四个调用方（`model` / `export` / `bench model`）只改了类型与构造名，方法签名保持一致；
`atlas` / `ctm` 本来就已使用 `ResourcePackStack`。

### 14.3 测试

`utils::assets::tests`：

| 测试 | 覆盖 |
|---|---|
| `directory_pack_lists_and_resolves_assets` | 多命名空间列举排序、`minecraft:`/裸 id/模组命名空间三种模型 id、原始字节读取 |
| `archive_pack_matches_the_directory_pack` | 同一份内容打包成 zip 后解析结果与目录包一致 |
| `path_traversal_entries_are_not_reachable` | `../`、`assets/../../`、绝对路径 entry 均不可读——CLI 不再绕过加固 |

真实 jar 冒烟：`mtk model dump ../26.2-Fabric.jar "minecraft:oak_stairs[facing=east,half=top,shape=inner_left]"` 正常输出几何。

### 14.4 B3 剩余

2. ✅ `mtk-resource` 的 `net` feature 默认关 + 清理空的 `parallel` feature（见 §15）；
3. ✅ 两处 `content_entries` 目录遍历合一（见 §16）；
4. ✅ `ci_guardrails` 依赖白名单（见 §17）。

---

## 15. B3 第 3 步：`mtk-resource` 的 feature 边界（2026-11-20）

### 15.1 问题

`mtk-resource` 的默认 feature 是 `["parallel", "zip", "net"]`：

- `net`（`ureq` + rustls）默认开，于是 `mtk-texture` / `mtk-material` / `mtk-model` / `mtk-voxel`
  全部**传递链接**了 HTTP/TLS 栈 —— 而它们没有一处需要抓皮肤；
- `parallel = []` 是空 feature，却被 `mtk-material` / `mtk-texture` / `mtk-model` / `mtk-voxel` /
  `libmtk` 五个 crate 转发，等于死配置。

### 15.2 改法

- `default = ["zip"]`；`net` 保留但改为**显式 opt-in**；
- 删除 `parallel` feature 与全部转发；
- 只有 `mtk-block-entity-textures`（皮肤三级解析/旗帜烘焙）打开 `net`；
- 顺带修掉一个被 feature 默认值掩盖的测试 bug：`skin/mod.rs` 的 `circuit` 测试在 `net` 关闭时
  引用了不存在的 `SkinResolver.circuit` 字段（`mod circuit` 原先被 `any(test, ...)` 放行）。
  现在 `circuit` 与相关测试都严格 `cfg(net && !wasm)` —— **`cargo test -p mtk-resource`
  在无网络 feature 下也能编译并通过**，这是之前从未验证过的路径。

### 15.3 验证

| 检查 | 结果 |
|---|---|
| `cargo tree -p mtk-voxel/mtk-model/mtk-texture/mtk-material` 含 `ureq`/`rustls` | **0**（改前全部非 0） |
| `cargo test -p mtk-resource`（net 关） | 28 passed |
| `cargo test -p mtk-resource --features net` | 33 passed |
| `cargo check -p mtk-wasm --target wasm32-unknown-unknown` | 通过 |
| `cargo tree -p mtk-py` 含 `ureq` | 2（宿主需要，符合预期） |

`mtk-cli` 仍会链接 net（它注入 `BlockEntityTextures` 以便 `bench world` 能解析存档里的头颅皮肤）；
这是宿主选择，不是库层默认。

### 15.4 B3 剩余

3. ✅ 两处 `content_entries` 目录遍历合一（见 §16）；
4. ✅ `ci_guardrails` 依赖白名单（见 §17）。

---

## 16. B3 第 4 步：资产指纹的两份实现合一（2026-11-20）

### 16.1 问题

`ResourcePackStack` 有两种指纹入口：

- `compute_stack_fingerprint()` —— 从**已加载的包**取 `content_entries()`；
- `compute_fingerprint_from_paths()` —— 主机在 UI 线程上、加载任何归档**之前**用路径算。

两者必须逐字节一致，否则会出现"主机判断 READY，却加载到另一个包"的情况。为此两处各自
实现了一份目录遍历（`DirectoryPack::content_entries` 与 `content_entries_from_path`），
以及一份相同的"排序 + 逐字段喂哈希"循环。两份代码只靠一个测试的目录场景保证一致——
任何一边的排序键、字段顺序、路径分隔符改写变化都会静默破坏另一个入口。

### 16.2 改法

- `pack/source.rs::content_entries_from_dir(root)` 成为唯一目录遍历；`DirectoryPack::content_entries`
  直接委托。
- `pack/stack/mod.rs::content_entries_from_path` 只保留"文件 → 单条 (size, mtime)"的分支，
  目录分支委托同一个遍历。
- `hash_pack_content(hasher, label, entries)` 成为唯一"排序 + 编码"实现，
  两个指纹入口共用。
- `modified_secs` 提升为 `pub(crate)`，只保留一份。

### 16.3 测试

`test_pack_fingerprint.rs` 新增 `path_fingerprint_matches_the_loaded_archive_stack`：
归档（zip）场景下，路径指纹必须等于加载后的栈指纹。原有目录场景的等值测试保留，
两组覆盖了两种 pack 类型。

### 16.4 B3 剩余

4. ✅ `ci_guardrails` 依赖白名单（见 §17）。

---

## 17. B3 第 5 步：把分层变成门禁（2026-11-20）

### 17.1 问题

`ARCHITECTURE_2.0` 的 L0–L3 表一直是**散文**。第 1 步修的正是它失效的后果：`mtk-voxel` 悄悄长出了
皮肤 HTTP、磁盘缓存、旗帜烘焙与动态图集，没有任何机械手段阻止下一次。

### 17.2 改法

`tools/ci_guardrails.py::check_internal_dependency_layering()`：

- 通过 `cargo metadata --no-deps` 读取工作区真实的 normal 依赖（dev/build 依赖不参与，
  dev-dependency 允许成环）；
- 每个 crate 只允许 `_INTERNAL_DEP_WHITELIST` 中列出的内部 crate；
  `mtk-cli` / `mtk-py` 这类工具与绑定层用 `"*"`；
- 白名单里写清规则来源：`mtk-core` 无内部依赖、`mtk-voxel` **不允许**依赖
  `mtk-block-entity-textures`（否则 HTTP/烘焙就回到 L0）、`mtk-resource` 不得反向依赖上层等；
- `cargo` 不存在时跳过（打印提示），`cargo metadata` 失败时**报错而不是跳过**——否则
  依赖成环这类问题会让检查静默失效。

### 17.3 验证

- 当前树：0 violations（同时跑在 CI lint job 的 `Architectural guardrails` 步骤）。
- 反向验证：临时给 `mtk-save` 加 `mtk-texture` 依赖 → 报
  `mtk-save: depends on mtk-texture, which the layering whitelist forbids`；还原后通过。
- 白名单同时是文档：改分层必须同一提交内更新它。

### 17.4 B3 完成状态

| 项 | 状态 |
|---|---|
| 1. `mtk-voxel` 去实体化 | ✅ §13 |
| 2. `mtk-cli` 去自带资产读取器 | ✅ §14 |
| 3. `net` feature 默认关 + 清理空 `parallel` | ✅ §15 |
| 4. 两处 `content_entries` 合一 | ✅ §16 |
| 5. 依赖白名单门禁 | ✅ 本节 |

---

## 18. C-06：图集不渗色（padding 策略收敛）—— B4 第 1 步

### 18.1 问题

三处生产/诊断代码都写死 `padding: 0`（`prebake/engine.rs`、`scene/exporter/builder.rs`、
`mtk-cli/{atlas,ctm}.rs`），而 `AtlasBuilderConfig { padding }` 的实现其实**早就支持** padding：
slot 尺寸 = `sprite + 2*padding`、内容画在 `slot + padding`、外圈用
`apply_edge_clamping_padding` 复制精灵自身边缘。也就是说这套机制从没在生产里跑过——
`padding` 只有单元测试碰过。

后果：每个精灵的 UV 矩形正好落在纹理边界（`u_min = x/W`），Blender 双线性采样在边界探到
**相邻精灵**的纹素；`mip_level > 0` 时更糟。HD 材质包上方块边缘的细线就是这个。

### 18.2 改法

- `AtlasBuilderConfig::padding_for_mip(mip)` = `max(2, 1<<mip)`（`checked_shl`，不静默欠配）；
- `AtlasBuilderConfig::production(max_w, max_h, mip)` —— 唯一的生产入口，`padding` 由 mip 推出；
- `Default for AtlasBuilderConfig` 也走 `production(4096, 4096, 0)`：`padding: 0` 不再是能
  "顺手拿到"的默认值，只有显式构造（测试固定布局）才会无 padding；
- 5 个调用点收敛（预烘引擎、场景导出、`mtk atlas`×2、`mtk ctm`）；`PyAtlasBuilder` 的
  `padding` 参数改为 `Option`，缺省取生产策略；
- 缓存格式 `ASSET_CACHE_FORMAT_VERSION` 11→12、`SCENE_PACKAGE_FORMAT_VERSION` 3→4：
  指纹只覆盖资源包内容，不覆盖烘焙配置，不 bump 就会把旧的无 padding 图集当有效缓存继续用。

UV 矩形**没有**改：`uv_bounds`/`pixel_rect` 本来就取 `inner`（不含 padding），所以网格 UV、
`pixel_rect`、Blender 侧材质绑定全部无需变动。

### 18.3 代价（实测，非估算）

图集层（`mtk-texture` 探针，13 个标准类别、5734 精灵、4096²）：

| padding | chunk 数 | 图集面积 | 烘焙 |
|---|---|---|---|
| 0 | 17 | 37.7 MiB | 190 ms |
| 1 | 17 | 39.8 MiB | 194 ms |
| **2（选定）** | **17** | **43.8 MiB** | **216 ms** |
| 4 | 17 | 47.9 MiB | 223 ms |

全量预编译（真实 jar + Mojang 资源 + SPBR，best of 3，`precompile_all_assets`）：

| | 耗时 | `.mtkcache` 字节 |
|---|---|---|
| padding 0 | 2016 ms | 24,329,279 |
| padding 2 | **2066 ms (+2.5%)** | **24,309,310 (−20 KB)** |

padding 让边缘整行整列重复，PNG/压缩反而更好，所以包体略小；chunk 数不变，宿主的纹理
数量与显存分配不变（+6 MiB 图集面积，远低于 100 MB 预算）。
运行时无变化：`mtk bench world`（624 sections / 335,641 quads / 8 线程）41.5 ms → 8.08M quads/s，
与改动前的 40.7 ms 同噪声区间。

### 18.4 测试（先红后绿）

`crates/mtk-texture/tests/test_atlas_padding.rs`：

1. `production_padding_covers_the_mip_level` —— padding ≥ 2^mip（mip 0..=8）；
2. `neighbours_never_touch_a_sprite_edge` —— 两个纯色精灵，逐个检查 slot 内 padding 环的每个
   像素都等于该精灵自身的颜色（padding 0 时这里读到的是邻居颜色）；
3. `uv_bounds_exclude_the_padding` —— uv 矩形严格等于 `pixel_rect`，padding 不进 rect。

红态证据：`production()` 先按原值（`padding: 0`）落地后运行，2 个断言失败；改成
`padding_for_mip` 后全绿。

顺带修了一个"用浮点数假装在验证方向"的测试：`libmtk` 的
`test_uv_coordinate_formula_orientation` 原先断言 `0.0/0.5/1.0` 这些硬编码分数（等价于把
padding 0 的布局抄了一遍）。现在改为：源图上半红下半蓝、精灵取下半区，断言迷你图集里该
精灵矩形内**每个像素**都是蓝色（翻转就会变红），再断言 uv ↔ pixel_rect 的 v 翻转公式。

### 18.5 连带的消费端改动

`../MoziToolKit/tests/test_scene_interchange.py` 断言场景包 `format_version == 3` → `4`
（与上次 2→3 同样的耦合方式）。

### 18.6 B4 剩余

2. ✅ K-04：`HashMap` 直接进 serde → 同输入两次烘焙字节不同（见 §19）；
3. K-06 缓存读错误的回退、C-01 `.mcmeta.frames`、C-02 filter 顺序、biome 一致性测试等。

---

## 19. K-04：烘焙产物字节级确定性 —— B4 第 2 步

### 19.1 问题与实测范围

`HashMap` 的迭代顺序对每个 map 实例随机（`RandomState` 每次 `new()` 取新种子），任何直接
serde 的 `HashMap` 都会让"同输入两次烘焙"产出不同字节。审计只点了 `AtlasAddressMap`，实际写的
时候是按"测试找证据"的方式做的——先写包级对照测试，让它逐个报错，再逐个修。

实测发现（真实 jar + Mojang 资源 + SPBR，17 MB 包 / 18878 模型 / 17 图集块）：

| 块 | 是否不确定 | 原因 |
|---|---|---|
| `models/database` | 是 | `BakedModelDatabase` 的 payload 两个 map，且每个 `BakedElement.faces` 也是 `HashMap` |
| `biome/mapping` | 是 | `BiomeResolver` 5 个 map（overlay / tint 类别 / 硬编码色 / models / models_by_stem） |
| `banner/patterns` | 是 | `HashMap<String, Vec<u8>>` 直接 postcard |
| `entity/slots`（场景包） | 是 | 槽位表直接 JSON |
| `atlas/mapping` | 是 | `sprites` / `anim_sprites` / `sprite_sources` |
| `resolver/overrides` | 是 | `ResolverOverrides` 两个 map |
| `manifest` | **预期不同** | `created_at_epoch_secs`，不是输入 |

### 19.2 改法

`mtk_core::serialize_sorted_map`（`mtk-core/src/serde_util.rs`，经 crate 根 re-export）：
把 map 的条目按 key 排序后仍用 `serialize_map` 输出。**线上格式完全不变**（postcard / JSON 的
map 编码都是 `len + 条目`），所以旧包照常反序列化——已用改动前写出的 v12 包验证过加载。

接入点：`AtlasAddressMap`×3、`CompactDatabasePayload`×2、`BakedElement.faces`、
`BiomeResolver`×5、`ResolverOverrides`×2、`PackageManifest.extra_props`。
另外两处不用 serde 路径，直接换容器：场景包 `entity/slots` 与横幅图案块用 `BTreeMap`。

**没有 bump 缓存格式**：读路径与语义都没变，只有写出来的条目顺序变了；旧包仍然正确。

### 19.3 测试（先红后绿）

- `mtk-texture/tests/test_address_map_determinism.rs`：同输入烘两次字节相等；
  反序插入的 map 序列化结果相同且 key 有序。（红：两个都失败）
- `libmtk/tests/test_package_determinism.rs`：
  - 合成 48 模型栈烘两次，**逐块**比较（`CHUNK_TAGS` 覆盖全部块类型，并断言找到的块数
    等于 `header.chunk_count`，防止新增块类型让测试悄悄变窄）。红：先是
    `models/database`（排序 map 后仍差）→ 再 `biome/mapping` → 逐个修完转绿。
  - 真资产栈（18k 模型）烘两次逐块比较，只有 `manifest` 跳过（时间戳）。
- `libmtk` 场景包单测：`entity/slots` 以 key 序写出（红：`{"zzz":..,"aaa":..,"mmm":..}`）。

### 19.4 结论

真实资产 `.mtkcache` 现在两次烘焙**逐块字节相同**（除 `manifest.created_at_epoch_secs`）。
`ASSET_CACHE_FORMAT_VERSION` 保持 12。

### 19.5 B4 剩余

3. K-06 缓存读错误静默回退、C-01 `.mcmeta.frames`、C-02 filter 顺序、biome 常量一致性等。

---

## 20. C-01：`.mcmeta` 的 `frames` 真正参与烘焙 —— B4 第 3 步

### 20.1 问题

`AnimationMetadata.frames` 解析得很完整（`Index` / `Detailed{index,time}`，允许重排与重复），
但烘焙只用了 `frametime`/`interpolate`：条带永远是图片里的文件顺序。原版自己就在用这个特性：

| 纹理 | 图片帧数 | `frames` 条目 | 后果（改前） |
|---|---|---|---|
| `lava_still` | 20 | 38（0..19 再 18..1 乒乓） | 只播 20 帧、没有回弹段 |
| `prismarine` | 4 | 22（大量重复） | 只播 4 帧、闪烁节奏错 |
| `magma` | 3? | 3 | 顺序相同，无影响 |

也就是说这不是"将来可能有用"，而是**当前原版岩浆/海晶石动画就播错**。

### 20.2 改法

`apply_animation_frame_order`（`mtk-texture/src/atlas/builder/mod.rs`）在**静态/动画分流之前**
执行，因此一次解决三件事：

1. 动画条带按 `frames` 重排、重复 → `frame_count` 变成 `frames.len()`；
2. 静态 sheet 的 Frame 0 自动取"第一个播放的帧"（原来取文件第 0 行）；
3. 导出的 `animation.frames` 清空 —— 条带已经是播放顺序，留着会让宿主应用两次
   （`frametime` 保留）。

PBR 伴随层同步重排（仅当它本身也是整条条带；单帧伴随层继续由 `blit_sprite_layers` 平铺）。
`frames` 里出现越界索引时**整表忽略**（继续用文件顺序），因为截断播放会悄悄缩短动画。

缓存 `ASSET_CACHE_FORMAT_VERSION` 12→13、场景包 `SCENE_PACKAGE_FORMAT_VERSION` 4→5：
条带内容与地址表元数据都变了，不 bump 会继续吃旧图集。

顺带删除零消费者的 `AnimationFrame::time()`（逐帧时长从未进入宿主动画驱动），在文档里写明
该限制。

### 20.3 验证

- 新测试 `mtk-texture/tests/test_animation_frames.rs`（5 个）：显式顺序烘出 `[蓝,红,绿]`、
  重复帧扩长条带（4 帧条目 over 3 帧图片）、无 `frames` 保持文件序、静态 sheet 取首播帧、
  导出元数据 `frames == None`。红态：4 个失败（唯一通过的是"无 frames 保持文件序"）。
- 真资产（原版 + Mojang 资源）：63 个动画精灵，`lava_still` `frame_count = 38`（原本 20）、
  `prismarine` 22、`magma` 3，且**没有任何**导出精灵仍带 `frames`——这条不变量写进了
  `libmtk/tests/test_package_determinism.rs` 的真资产测试。
- 全量烘焙仍逐块确定性（同测试的另一半）。

### 20.4 B4 剩余

4. ✅ K-06 缓存读错误静默回退（见 §21）；C-02 filter 顺序；biome 常量一致性测试。

---

## 21. K-06：把"读不出"和"没有"分开 —— B4 第 4 步

### 21.1 问题

`ResourcePack::open` 返回 `Option<Vec<u8>>`，`DirectoryPack` 实现是 `fs::read(..).ok()`：

```rust
let norm = Self::normalize_path(relative_path)?;
fs::read(self.root_dir.join(norm)).ok()      // 权限错误 == 文件不存在
```

于是 `ResourcePackStack::open_asset_raw` 在最高优先级包读失败时继续往下找，**静默返回下一个包的
同名字节**。用户把材质包里的 `stone.png` 改坏权限（或文件被占用、NFS 抖动），渲染里看到的是原版
贴图，日志里什么都没有；预烘还会把这个结果写进 `.mtkcache`，从此一直是错的。

### 21.2 改法

- 新增 `ResourceError::ReadFailed { pack, path, source }`（带包名与路径，错误信息可直接给用户看）；
- `ResourcePack::open` → `Result<Option<Vec<u8>>, ResourceError>`：
  - `NotFound` / `NotADirectory` / `IsADirectory` → `Ok(None)`（确实没有，或那个路径是个目录）；
  - 其他 I/O 错误 → `Err(ReadFailed)`。
  - `MemoryPack` / `ZipPack` 恒为 `Ok(...)`（zip 的读错误在装载时就已暴露）；
- 栈层：`open_asset_raw` / `open_texture_raw` / `open_model_raw` / `resolve_pbr_companions` /
  `load_ctm_rules` / `build_ctm_solver` 全部返回 `Result`，**第一个 Err 直接中断查询**；
- 上游一路传播：`mtk-material` 的 `BiomeResolver::load_from_pack(_stack)`、
  `mtk-texture` 的 `process_paletted_permutations` 与 `BannerBaker::load_from_pack_stack`（原本返回
  `Result` 却被 `let _ =` 吞掉）、`mtk-voxel` 的 debug world（映射进 `MissingAssets`）、
  `libmtk::prebake` 的模型扫描/blockstate/物品贴图、`mtk-cli` 的 `AssetStack`、`mtk-py` 的
  `bake_blockstate` / material resolver / `build_ctm_solver` / `get_ctm_rule_count`。
- 删除 `ResourcePack::has_file`：零生产消费者（只在测试里用于路径穿越断言），它会把 `Err` 折成
  `false`，正是本项要消灭的模式。

### 21.3 无法返回错误的回调

`ModelBaker::bake_blockstate(state, def, lookup: impl Fn(&str) -> Option<BlockModelJson>)` 的父模型
查找回调不能返回错误。两处按同一方式处理：把第一个读失败**暂存**，烘焙结束后立即上抛。

- `libmtk::prebake`：`Mutex<Option<ResourceError>>`（解析阶段是并行的），`get_model` 记录，使用
  `resolved_map` 之前检查；
- `mtk-cli`：`ReadFailureCell`（`RefCell` + `capture`/`into_result`），每个 bake 调用点包一层。

物品烘焙原本的错误类型是 `(String, &'static str)`（诊断），改为
`enum ItemBakeError { Read(ResourceError), Diagnostic { key, reason } }`，读失败进 `Read` 并中止整次
预烘，真正的诊断（`no_geometry` 等）行为不变。

### 21.4 测试（先红后绿）

`mtk-resource/tests/test_pack_read_errors.rs`：

1. `an_unreadable_asset_does_not_fall_back_to_a_lower_priority_pack` —— chmod 000 的 `stone.png`
   + 低优先级包同名文件。**旧行为实测返回 `Some("from-vanilla")`**（红），现在要求
   `ReadFailed { pack: "user", path: … }`；
2. `a_directory_at_the_asset_path_is_absent` —— 目录不是资产，必须是 `Ok(None)`；
3. `a_missing_asset_is_absent` —— 普通缺失仍是 `Ok(None)`。

`libmtk/tests/test_pack_read_failures.rs`：

1. `a_model_that_cannot_be_read_fails_the_precompile` —— 模型 JSON chmod 000，预烘必须报错且错误里
   含文件名；
2. `a_failing_pack_is_not_replaced_by_a_lower_priority_pack` —— 栈查询不得返回低优先级包的同名模型；
3. `the_fixture_bakes_when_readable` —— 同一个 fixture 可读时能正常烘出模型（保证前两个测试是因
   为权限而不是 fixture 本身失败）。

红态证据：临时把 `DirectoryPack::open` 改回"任何错误都 `Ok(None)`"，上面 libmtk 的两个测试分别以
`an unreadable model must fail the bake instead of silently skipping it` 与第 2 条断言失败，而
fixture 测试仍通过；恢复实现后三个全绿。

### 21.5 行为约定（写进 crate README）

- `Ok(None)` 才会让栈去找下一个包；`Err` 中止整次查询并上抛；
- 越界路径（`../`、绝对路径、`C:`）按"没有"处理（既不是错误也不泄漏）；
- "路径是目录"也算"没有"。

### 21.6 B4 剩余

5. ✅ C-02 atlas `filter`（见 §22）；✅ C-07 图集定义按 stack 合并（见 §23）；C-08 拆成 a/b/c 三项（见 §24）；biome 常量与 jar 一致性测试仍在。

---

## 22. C-02：atlas `filter` —— 先读 MC 源码，再改代码

### 22.1 先纠正审计里的两个错误判断

审计当时建议"两阶段：先收集全部 source，再按 pack 优先级覆盖"，理由是"顺序相关 = bug"。读过
`Minecraft_Source/net/minecraft/client/renderer/texture/atlas` 之后可以确认这个建议是**错的**：

```java
// SpriteSourceList.list(...)
this.sources.forEach(s -> s.run(resourceManager, output));   // 顺序执行
...
// SourceFilter.run(...)
output.removeAll(this.filter.locationPredicate());           // 只删"此刻已收集"的
```

`filter` 的位置是 vanilla 语义的一部分：放在 directory 源之前的 filter 什么都不会删。改顺序会
偏离原版。

第二处：`IdentifierPattern` 的匹配不是 `contains`，而是

```java
// IdentifierPattern
this.namespacePredicate = namespacePattern.map(Pattern::asPredicate)...
this.pathPredicate = pathPattern.map(Pattern::asPredicate)...
// ExtraCodecs.PATTERN = Codec.STRING -> Pattern.compile(...)
```

即 **Java 正则 + `Pattern.asPredicate()`（`matcher.find()`，子串匹配）**。对字面量模式而言
`find()` 与 `contains` 等价，所以旧实现的差别只在正则模式上——但真正的问题不在匹配，而在**形状**。

### 22.2 真正的问题：`pattern` 被 `flatten` 了

我们的 `AtlasSource::Filter` 是

```rust
Filter { #[serde(flatten)] pattern: AtlasFilterPattern },   // 期望 namespace/path 在源对象顶层
```

而 vanilla 的形状是 `{"type":"…filter","pattern":{"namespace":…,"path":…}}`（`fieldOf("pattern")`）。
实测（新测试集第一条只是把它暴露出来）：

```
输入 {"type":"minecraft:filter","pattern":{"namespace":"minecraft","path":"block/"}}
解析结果 Filter { pattern: AtlasFilterPattern { namespace: None, path: None } }
```

`pattern` 键被忽略、两个字段都是 `None` → 匹配器认为"匹配一切" → `retain` 把**整个图集清空**。
任何带 `filter` 的材质包在当前版本会得到一个只剩 fallback 的图集。

### 22.3 改法

- `Filter { pattern: AtlasFilterPattern }`（嵌套字段，`pattern` **必填**——缺省值会退化成"删光"）；
- 新增 `FilterPattern`：保存原始文本 + 编译好的 `regex::Regex`（`Serialize` 写回文本，
  `PartialEq` 比文本）；`AtlasFilterPattern::matches` = namespace 与 path 各自 `find()`；
- 无效正则 → 反序列化失败 → 整个 atlas 定义解析失败（MC 同样在 `Pattern.compile` 失败时丢弃
  该文件的 source 列表，我们退化为该分类的内置默认定义）；
- 保留"按位置 `retain`"的顺序语义，并加测试把 vanilla 行为钉住（防止未来有人把它"清理"成两阶段）。

**新增依赖 `regex`**：atlas `filter` 的数据格式本身就是正则，用 `contains` 近似等于换一套语义。
`regex` 是纯 Rust、可用 `wasm32-unknown-unknown`（已验证 `cargo check -p mtk-wasm --target
wasm32-unknown-unknown`）。Java 专有构造（lookbehind/backreference）会被拒绝并报出模式文本——
比"静默按字面量处理"更容易发现。

### 22.4 测试（红→绿）

`crates/mtk-resource/tests/test_atlas_filter.rs`（8 个）：

| 测试 | 覆盖 |
|---|---|
| `a_literal_filter_removes_only_the_named_sprite` | 字面量只删目标精灵 |
| `a_regex_filter_matches_like_minecraft` | `block/.*_ore` 命中 `diamond_ore` |
| `an_anchored_regex_only_matches_the_whole_path` | `^block/stone$` 不误伤 |
| `a_namespace_only_pattern_ignores_the_path` | 只给 namespace |
| `a_filter_before_the_directory_source_removes_nothing` | 顺序语义（vanilla） |
| `an_invalid_regex_is_rejected` | 无效正则 → 解析失败 |
| `a_filter_without_a_pattern_is_rejected` | `pattern` 必填 |
| `unrelated_sources_still_collect` | 无 filter 时行为不变 |

红态证据：改前前 6 条里 5 条失败，输出是 `got []`——图集被清空，正是 22.2 描述的后果。

### 22.5 顺带发现（已记为 C-07，未在本项修）

`load_atlas_definition` 只读**最高优先级包**的那一份 `atlases/blocks.json`；而 MC 的
`SpriteSourceList.load` 是 `for (Resource entry : resourceManager.getResourceStack(resourceId))`
——把**所有包**的同名文件按优先级顺序拼起来。于是"只追加自己的 sprite、不重复原版列表"的材质包
在我们这里会丢掉整个原版目录源（渲染成 fallback）。这是与 filter 相邻但独立的一项，见 §23 / backlog C-07。

---

## 23. C-07：图集定义按 stack 合并 —— B4 第 5 步

### 23.1 源码依据（先确认方向，再写代码）

`SpriteSourceList.load` 把**所有包**的同名 `atlases/*.json` 拼起来：

```java
for (Resource entry : resourceManager.getResourceStack(resourceId)) {
    loaders.addAll(SpriteSources.FILE_CODEC.parse(contents).getOrThrow());
}
```

关键在于 `getResourceStack` 返回的顺序。`FallbackResourceManager.getResourceStack` 的收集循环是
`for (i = fallbacks.size()-1; i >= 0; i--) result.add(...)`，最后一行是

```java
return Lists.reverse(result);
```

而 `fallbacks` 的末尾是最高优先级（`getResource` 同样自末尾向前找第一个命中）。所以**返回顺序 =
低 → 高优先级**，`loaders.addAll` 也是低 → 高。这与 `SpriteSourceList.Output.add` 的"后来的
替换先前的"一致：高优先级包的 sprite 覆盖低优先级包的。

### 23.2 问题

`load_atlas_definition` 只读**最高优先级包**的那一份文件：

```rust
let bytes = self.open_asset_raw(&path)?.ok_or_else(...)?;   // 只返回栈顶命中的那一份
```

于是"只追加自己的 sprite、不重复原版列表"的材质包会把整个原版目录源顶掉——原版方块全变
fallback。实测（红态）只有 `["minecraft:custom/glow"]`，`block/stone`、`block/dirt` 全丢。

### 23.3 改法

按 `packs()`（高→低）`.rev()` 遍历，即低→高；每个包里如果有该文件就读、解析、把 `sources`
追加到累积结果；一个包都没有时仍返回 `NotFound`（调用方据此回退到内置默认定义）。
解析失败时错误信息带上包名与路径（不静默丢弃——MC 会记日志丢弃该文件，我们选择让调用方
看到；`load_atlas_category` 的回退行为不变）。

### 23.4 测试（7 个，红→绿）

`crates/mtk-resource/tests/test_atlas_definition_merge.rs`：

| 测试 | 覆盖 |
|---|---|
| `a_pack_that_only_adds_sprites_keeps_the_lower_pack_sources` | 增量包不顶掉原版目录源（红态：`["minecraft:custom/glow"]`） |
| `sources_are_concatenated_from_low_to_high_priority` | 合并顺序 = 低→高 |
| `a_higher_pack_filter_can_remove_a_lower_pack_sprite` | 高优先级包的 filter 能删掉低优先级包的 sprite（C-02 的位置语义在合并后仍然成立） |
| `three_packs_all_contribute` | 三个包各贡献一份 |
| `a_missing_definition_is_reported` | 谁都没有 → `NotFound` |
| `a_broken_definition_in_one_pack_is_an_error_not_a_silent_skip` | 坏 JSON → 报错（含包名） |
| `a_single_pack_behaves_as_before` | 单包行为不变 |

真资产复核：jar + Mojang 资源目录 + SPBR 三者都带 `atlases/*.json`，合并后仍是 17 个图集块、
18878 个模型（重复 source 由收集阶段的 `registered_sprites` 去重），`mtk bench world` 与确定性
测试均不变。

### 23.5 顺带发现（C-08）

1. `load_atlas_category` 仍会把"atlas 定义解析失败"静默替换成内置默认（`unwrap_or_else`），
   连一条日志都没有；
2. MC 的 `getResourceStack` 还会尊重 pack.mcmeta 的 `filter.block`（`PackEntry.isFiltered`），
   我们完全没实现——被材质包过滤掉的资源在我们这里依然可见。

两项都记入 backlog C-08（P2），不在本项处理。

---

## 24. C-08：`pack.mcmeta` 与定义错误可见性

前几项（K-06 / C-02 / C-07 / C-06 / K-04 / C-01）都是"资源读取与烘焙对 MC 的忠实度"，
C-08 收尾同一簇的最后三处。全部依据 `Minecraft_Source` 逐行核对。

### 24.1 C-08a：atlas 定义解析失败不再静默回退

`load_atlas_category` 原本是 `load_atlas_definition(..).unwrap_or_else(|_| category.default_definition())`，
把"没有包提供该文件"和"文件坏了"混成一条路径。红态证据：

```
a pack shipped a broken blocks.json, yet the result is byte-identical to the built-in default
```

现在只有 `NotFound` 回退到内置默认（MC 客户端永远自带原版 `blocks.json`，内置默认是它的等价物），
读取失败与解析失败原样上抛，错误信息里带包名与路径。

`mtk-resource/tests/test_atlas_definition_errors.rs`（4 个）：坏 JSON 报错且含文件名/包名、
缺失仍回退内置定义、无权限报错、类别后处理（end portal 注入）仍生效。

### 24.2 C-08b：`overlays`

MC：`pack.mcmeta` 的 `overlays.entries[].{formats, directory}`；`formats` 是 `PackFormat` 区间
（26.2 客户端资源包格式 = `version.json` 的 `pack_version.resource_major` = **88**）。适用性命中的
overlay 目录被包装成独立 `PackResources`，`CompositePackResources` 的顺序是
`reverse(overlays) + primary`，即**声明越靠后的 overlay 优先级越高**，最后才是包根。

实现：

- `pack/mcmeta.rs`：`PackMetaFile` + `OverlayEntryFile`；`formats` 接受 int / `[min,max]` /
  `{min_inclusive,max_inclusive}` 三种形状，`min_format`/`max_format`（int 或 `[major,minor]`）
  优先于 `formats`，比较用 `(major, minor)` 元组（legacy `formats` 的上界覆盖整个 major）；
  目录名按 MC 的 `[-_a-zA-Z0-9.]+` 校验，杜绝 overlay 逃逸包根；
- `DirectoryPack` / `ZipPack` 各维护一份"适用 overlay，声明顺序"；查找顺序 = `overlays.rev()`
  再接包根；`list_files` 在包根一层跳过 overlay 子树，并把 overlay 内的文件按普通资产路径列出
  （不暴露 overlay 目录名）；`content_entries` 同样排除/合并，**路径指纹走同一个解析入口**
  （`directory_content_entries`），否则主机用路径算的指纹会和加载后的栈指纹不一致；
- `CURRENT_CLIENT_RESOURCE_FORMAT = 88` 是手写常量，因此加了一条测试直接读
  `Minecraft_Source/version.json` 对照 `pack_version.resource_major`（缺资产时 skip）。

测试 `test_pack_overlays.rs`（9 个）：适用 overlay 覆盖包根、声明靠后者优先、不适用被忽略、
三种 `formats` 形状、列表里不暴露 overlay 前缀、overlay 编辑改变指纹、zip 包同样支持、
无 overlays 的包行为不变、MemoryPack 不受影响。红态：5 条失败（目录包与 zip 包都读出包根内容，
overlay 里的文件在列表里带 `overlay_1/` 前缀）。

另在 `test_pack_fingerprint.rs` 补两条：带 overlay 的目录包"路径指纹 == 加载栈指纹"；
`CURRENT_CLIENT_RESOURCE_FORMAT` 与 `version.json` 一致。

### 24.3 C-08c：`filter.block`

MC：`pack.mcmeta` 的 `filter.block` 是 `IdentifierPattern` 列表（与 atlas filter 同一套正则）。
`FallbackResourceManager.getResource` 先查该包自己的资源，再判 `isFiltered`——命中就
**停止**向低优先级包查找（`LOGGER.warn` + `Optional.empty()`）；`listResources` 里
`entry.filterAll(已收集集合)` 会剪掉低优先级包已贡献的条目。也就是说：过滤不是"本包看不见"，
而是"从这里往下都看不见"，而且包自己的副本永远优先。

实现：

- `identifier_pattern.rs`：把 atlas filter 的 `AtlasFilterPattern` 提升为 MC 同名的
  `IdentifierPattern`（`{namespace?, path?}`，`FilterPattern` 保存原文 + 编译后的正则），
  atlas `filter` 与 `pack.mcmeta` `filter.block` 共用；`ResourceFilterSection` = `{block: [...]}`；
- 包层：`DirectoryPack` / `ZipPack` 从各自的 `pack.mcmeta` 解析 `filter`，通过
  `ResourcePack::filter()` 暴露（默认 `None`）；
- 栈层：`open_asset_raw` 按"本包资源 → 本包 filter 命中即停"的顺序；新增
  `list_asset_files(prefix)` 复刻 `listResources` 的
  `filterAll(已收集) → 加本包条目`（BTreeMap 排序去重）；
- 把四处"逐包 listing"改成合并列表：`collect_sprites_for_atlas` 的 directory 源、
  `load_ctm_rules`、`libmtk` 的模型扫描、`BannerBaker`、`BiomeResolver::load_from_pack_stack`。
  顺带修掉 `BiomeResolver` 逐包遍历时**低优先级覆盖高优先级**的问题。

测试 `test_pack_filter.rs`（7 个）：filter 隐藏低优先级包的同名资源（读取与列表都隐藏）、
过滤包自己的副本仍然生效、更高优先级包的资源不受影响、无关路径不受影响、正则与 namespace
模式、zip 包同样生效、无 filter 的包行为不变。

红态证据：临时去掉两处 filter 钩子后 3 条失败（`a_filter_hides_a_lower_packs_resource`、
`unrelated_paths_are_unaffected`、`regex_and_namespace_patterns_work`），恢复后 7 条全绿。

### 24.4 明确 VOID 的两条

- `.png.mcmeta` 的 `texture.blur` / `clamp`：只在运行时采样生效，烘焙期不采样，我们没有消费者；
- 包 `formats` 的兼容性判定（`isCompatible`）：MC 用它决定 UI 里能否勾选，不影响读取。

---

## 25. C-09：生物群系表改为"从数据生成 + 对照测试"

### 25.1 为什么这块一直很乱

盘点后的实际分层（也是"补丁叠补丁"的来源）：

| 层 | 内容 | 问题 |
|---|---|---|
| `canonical_data.rs`（864 行） | 66 个 `BiomePalette`：id/name/四个 hex/温度/湿度/三个 `has_custom_*` | **手工抄写**。派生色（colormap 采样结果）也被抄进来，逐渐漂移 |
| `palettes.rs` | `CANONICAL_LINEAR_COLORS` 预计算、`is_swamp` 相关查找、hex→linear | 依赖上面的表 |
| `hardcoded.rs`（470 行） | 方块 → tint 层注册表、`get_swamp_grass_color`（重实现 Perlin 噪声） | 与表里的 swamp 颜色并存 |
| `resolver/*` | 扫描包内模型 JSON 的 tintindex/overlay 关系 | 与表无关，独立 |
| `mtk-voxel/biome/color.rs` | `is_swamp = pal.id == "swamp" \|\| "mangrove_swamp"` | 硬编码 id，无视 `grass_color_modifier` |

### 25.2 对照结果（改前）

`jar` 侧有 66 个 `data/minecraft/worldgen/biome/*.json`。逐项对比后：

- **id 集合错了**：多一个 `sparce_jungle`（拼写错误的幽灵条目，jar 里不存在），少一个 `sulfur_caves`（26.2 新增，带显式 `grass_color #aba64f`、`water_color #34bf89`）——
  命中 `sulfur_caves` 的存档会因为查不到 palette 而回退到 plains 配色；
- **派生色漂移**：11 处 ±1~4（如 `birch_forest` `#88BB67` vs 实际 `#88BB66`；`windswept_hills` 的 dry foliage `#957853` vs `#967753`）；
- 温度/湿度/水色/显式色**全部正确**。

### 25.3 一个精度陷阱（解释漂移来源之一）

`ColorMapColorUtil.get` 的索引是 `(int)((1.0 - temp) * 255.0)`。Minecraft 的 `ClimateSettings`
把温度/湿度存成 **float**，`1.0 - 0.6f = 0.3999999761581421` → 索引落在 101，而用 double 算
`0.6` 会落到 102。Rust 侧 `BiomePalette.temperature: f32` 转 `f64` 恰好复刻了 Java 的加宽，
所以**我们的采样是对的**；如果生成器用 Python double 直接算，就会偏一个纹素。生成器
（`tools/gen_biome_palettes.py`）因此显式做 `as_f32()` 舍入——这正是那条 1 个色阶漂移的来源。

### 25.4 改法

- 新增 `tools/gen_biome_palettes.py`：读 66 个 biome JSON + 三张 colormap PNG（自带最小 PNG 解码器，
  不引入 Pillow 依赖），按 MC 规则算出全部字段，生成 `canonical_data.rs`；文件头写明"生成物，
  别手改"；
- 生成规则与 MC 一致：`effects.*_color` 优先；无显式色时采样 colormap（f32 舍入、`y<<8|x` 索引）；
  `grass_color_modifier = dark_forest` 用 packed ARGB 变换 `((base & 0x00FEFEFE) + 0x0028340A) >> 1`；
  `swamp` 取 `#6A7039`（高噪声色，逐方块由 `get_swamp_grass_color` 在低/高之间选择）；
- 新增 `crates/mtk-material/tests/test_biome_matches_jar.rs`（5 个），真资产缺失时 `skip`：
  1. id 集合与 jar 完全一致、无重复、条目数相等；
  2. 温度/湿度/`water_color` 逐项相等；
  3. 显式 `effects.{grass,foliage,dry_foliage}_color` ↔ `has_custom_*` + hex 相等（修饰符生物群系
     暂按"固定色"处理，见 C-10）；
  4. 派生色必须等于 colormap 采样值（逐字节，不留容差）；
  5. `grass_color_modifier` 恰好出现在 `dark_forest`/`swamp`/`mangrove_swamp`，且 dark_forest 的表值
     等于 packed 变换结果、swamp 系等于 `#6A7039`。

红态证据：改前 5 条里 4 条失败——缺 `sulfur_caves`、`sparce_jungle` 多出、`dark_forest` 的表值
（当时也是对的，是我的测试公式写成逐通道少算了 0x28/0x34）、`birch_forest` 派生色偏 1。

### 25.5 顺带确认的两个真实渲染缺陷（记为 C-10）

1. **`dark_forest` 的草色从未变暗**：全仓没有 `grass_color_modifier` 的实现，运行期对
   `dark_forest` 直接返回 colormap 采样值。深色森林的草因此偏亮（MC 会乘上固定变换）。
2. **`is_swamp` 是硬编码 id 判断**（`pal.id == "swamp" || "mangrove_swamp"`），而 `has_custom_grass`
   被当成"固定色"复用：真正语义应是 `effects.grass_color` 显式声明。两者混在同一个布尔上，
   导致下次新增/改名沼泽系生物群系就会静默失效。

这两项属于行为改动（需要独立测试 + 全套件验证），拆成 C-10 单独做。

### 25.6 影响面

生成后 `crates/mtk-material/src/biome/canonical_data.rs` 的 diff 仅 31 行：11 处颜色值 ±1~4、
`sparce_jungle` 删除、`sulfur_caves` 新增。全仓测试、`cargo build-all-py`、Blender 套件 591 passed
均通过（Blender 侧 PSNR 门限不受 1 个色阶影响）。

---

## 26. C-10：`grass_color_modifier` 数据化，删掉两处硬编码补丁

### 26.1 问题（C-09 的收尾）

C-09 把生物群系表变成"从数据生成"，但当时刻意保留了两处补丁（否则会改变行为）：

1. **`dark_forest` 的草色从未变暗**：全仓没有 `grass_color_modifier` 的实现。运行期
   `sample_column_tint_color` 对非自定义草色的生物群系直接返回 colormap 采样值，而 Minecraft 的
   `GrassColorModifier.DARK_FOREST` 会做 `((base & 0x00FEFEFE) + 0x0028340A) >> 1`。
2. **`has_custom_grass` 一布尔两义**：既表示"jar 声明了 `effects.grass_color`"，又被当成
   "这是固定色（modifier 派生）"来用；`swamp`/`mangrove_swamp` 另有一份
   `pal.id == "swamp" || pal.id == "mangrove_swamp"` 的硬编码判断。

### 26.2 改法

- `BiomePalette` 新增 `grass_color_modifier: GrassColorModifier`（`None` / `DarkForest` / `Swamp`），
  由 `tools/gen_biome_palettes.py` 从 JSON 写入；`has_custom_grass` 回归本义（只有显式
  `effects.grass_color` 才为 true）；
- `GrassColorModifier::apply_linear`：在 **packed ARGB** 上执行 dark_forest 变换（三个通道的偏移
  不同：0x28/0x34/0x0A，不能逐通道在 linear 空间做），其余 modifier 原样返回；
- `mtk-voxel`：`SmoothedBiomeColumn.is_swamp: bool` → `grass_color_modifier`；`BiomeColumnSums` 用
  "核内权重最大的 modifier"（dark_forest/swamp 都是整群系属性）；`sample_column_tint_color` 的
  GRASS 分支：swamp → 噪声色、dark_forest → 采样后变换、显式 → 表内色、其余 → 采样；
- **宿主契约不变**：dark_forest/swamp 的颜色宿主无法自行复现（宿主没有 modifier 逻辑），所以这两类
  仍然以 `TINT_TYPE_HARDCODED` 导出精确颜色。`compute_mesh_biome_attributes_paletted_flat` 与
  `resolve_biome_color_set_with_colormaps` 相应地把"固定草色"判定改为
  `has_custom_grass || modifier != None`。

### 26.3 顺带删掉的重复实现

`bindings/mtk-py/src/material/biome/tint.rs::compute_mesh_biome_attributes_from_storage` 里复制了
一整份 tint 分派表（GRASS/FOLIAGE/DRY/WATER/HARDCODED），且只看 `has_custom_grass`——这正是
"dark_forest 变不回 HARDCODED"的直接原因，也违反"绑定层不得复制核心逻辑"。现在导出
`mtk_voxel::sample_column_tint_color`，绑定直接调用，约 60 行重复逻辑消失。

### 26.4 测试

- `mtk-voxel` 单元测试（新 2 个）：
  - `dark_forest_grass_is_darkened_after_sampling`：断言 `has_custom_grass == false`、返回颜色等于
    "采样后 packed 变换"（期望值在测试里独立按 sRGB 传输函数计算，不复用被测代码）、且
    **返回 override = true**（宿主契约）；
  - `swamp_grass_ignores_the_colormap_and_keeps_its_noise_colour`：swamp 必须用噪声色而不是 colormap。
- `mtk-material/tests/test_biome_matches_jar.rs` 增加 `the_custom_grass_flag_means_explicit_not_modifier`：
  `has_custom_grass == (jar 声明 effects.grass_color)` 且 `grass_color_modifier` 与 JSON 一致；
  原来对 modifier 生物群系的"carve-out"删除。
- 红态：改前 `dark_forest_grass_is_darkened_after_sampling` 在第一条断言（`has_custom_grass == false`）
  失败；Blender 侧 `test_3d_biome_tinting` 的两条断言（dark_forest 应为 HARDCODED=4）也失败。

### 26.5 验证

- `MTK_REQUIRE_REAL_ASSETS=1 cargo test --workspace`：0 失败 0 SKIP；
- `cargo build-all-py` + Blender 套件 **591 passed**，包括 `test_3d_biome_tinting` 的
  dark_forest 变暗与 swamp 噪声两条行为断言；
- 生成器重跑后 `canonical_data.rs` 只多出 66 行 `grass_color_modifier` 字段与 3 处语义修正。

### 26.6 剩余（不在本簇）

多生物群系 uniform 混合路径（`resolve_biome_color_set_with_colormaps` 的 `multi_biomes` 分支）
仍返回 `has_custom_*=false` 但给出混合后的固定色；这一路径没有测试覆盖，等有消费方时再定语义。

