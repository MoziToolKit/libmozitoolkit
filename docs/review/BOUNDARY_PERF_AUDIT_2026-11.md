# 跨 crate 边界 / 性能 / 基准有效性 审查

日期：2026-11-20 · 基线：`libmozitoolkit 3843e15` / `MoziToolKit de8a080`
配套：`CODE_REVIEW_2026-11.md`、`FIX_BACKLOG.md`（已按 §4 的三分法重写）

**进度**：B0（清单重划 + 资产发现/跳过门禁）与 B1（删 `mtk-bench`、`mtk-cli bench world`、
宿主场景基准、面板双指标）已落地；§8 其余批次仍待做。

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

**B2 · 外围性能（2 天，按收益排序）**
- biome 列预计算（`padded.rs` + `biome/color.rs`）→ 目标 `extract_padded` < 5 ms；
- `rebuild_all` 改用 seam-filtered weld → `assemble` −6 ms；
- `is_non_mesh_placeholder_block` 去分配 + 进 `ResolvedPaletteEntry`；
- `DeltaMesher` 复用 palette cache；
- Blender `ensure_world_materials` 只在槽位变化时写 `material_index`；
- `get_world_pipeline_assets` 失败显式报错 + memo 不绑指纹；
- 验收：§1 的端到端 226 ms → 目标 ≤ 130 ms（2.6M quads/s）。

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
