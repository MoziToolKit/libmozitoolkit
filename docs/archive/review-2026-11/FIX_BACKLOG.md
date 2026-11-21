# 修复清单（按代码事实重新划档）

生成时间：2026-11-20 · 基线：`libmozitoolkit 3843e15`（含 B0/B1 校准提交）/ `MoziToolKit de8a080`
配套：`CODE_REVIEW_2026-11.md`（历史审查）、`BOUNDARY_PERF_AUDIT_2026-11.md`（跨 crate / 性能 / 基准）

> **这份清单取代此前同名文件的"状态列"。** 旧文件把"报告里写了"当成"已经修了"，
> 2.0 转向（`81ba2aa` 删除连续实体）之后也没有回写，导致约 1/3 条目指向已删除的文件。
> 现在每条只允许三种状态，并且必须给出**复核方式**（代码位置或测试名）：
>
> - **DONE**：本轮打开代码/跑测试确认行为正确；
> - **VOID**：2.0 范围之外（文件已删除或只属于 `release/2.1`），不做；
> - **OPEN**：仍存在于当前代码，附复现方式。
>
> 无法在本轮复核的，单独放 §5「未复核」，不要混进 DONE。

---

## 1. 现在怎么用

```
选一条 OPEN → 先复现（跑测试或最小示例） → 加先红后绿的测试 → 改 → 跑该行验证命令 → 勾掉
```

门禁（与 CI 对齐）：

```bash
cargo fmt --all -- --check
python3 tools/ci_guardrails.py
cargo clippy --workspace --all-targets -- -D warnings
MTK_REQUIRE_REAL_ASSETS=1 cargo test --workspace --no-fail-fast
cargo build-all-py                                        # 改过核心/绑定必跑
blender --background --python ../MoziToolKit/tests/run_tests.py -- -rs -q
```

`MTK_REQUIRE_REAL_ASSETS=1` 会让"缺资产跳过"变成失败。真实资产现在从
`../mc` **或** `../Minecraft_Source` / `../Minecraft_Unpacked` 自动发现，存档从
`../mtk-save-world` 自动发现（`mtk-testkit` + `../MoziToolKit/tests/_assets.py` 已同步）。

性能/基准相关不再放这里，见 `BOUNDARY_PERF_AUDIT_2026-11.md` §2、§8。

---

## 2. DONE（本轮复核过的）

| ID | 结论 | 复核方式 |
|---|---|---|
| **S-11** | `PyBufferOwner` 现在持有 `live_views: Arc<AtomicUsize>`，导出计数、变更前拒绝重分配 | `bindings/mtk-py/src/buffer.rs:27-41` + `__getbuffer__` |
| **S-12** | 属性缓冲改为 `PyBUF_FORMAT \| PyBUF_C_CONTIGUOUS`，校验 `format`/`itemsize`，空缓冲用对齐 dummy | `bindings/mtk-py/src/mesh/attributes.rs:104-140` |
| **Y-01 / Y-02** | `PALETTE_UPDATE`（0x12）与 `DELTA_ENTITY`（0x11）都走同一个 `advance_sequence`，与 DELTA 共享计数器；重复包不推进 | `mtk-sync/src/client/session/handlers_delta.rs:54,113,147,255-275` + 单测 `sequence_advances_only_on_the_next_packet` |
| **Y-03** | `rel_pos` 越界条目被丢弃并合并成一条 `Warning`，不再溢出到错误区块 | `handlers_delta.rs:164-186` |
| **K-01** | 目录包/zip 包/内存包都实现 `content_entries()`（路径+大小+mtime）；`compute_fingerprint_from_paths` 与加载栈指纹有等价测试 | `mtk-resource/src/pack/source.rs:143,242,381`、`stack/mod.rs:201-253`、`tests/test_pack_fingerprint.rs:95` |
| **K-02** | 资产缓存读取校验 `manifest.format_version`（不只是 64B header） | `libmtk/src/package/source.rs:100-110`、`prebake/reader.rs:55-69` |
| **V-03 / V-04 / V-05 / V-06** | dirty 合并、biome-only 置脏、26 邻居、`(cy>>2).min(3)` | `mtk-voxel/src/world/sync.rs`、`storage/world/container.rs`、`mesher/shading/resolver.rs:398`；测试 `sync_keeps_dirt_queued_before_the_sync`、`biome_only_change_still_marks_the_section_dirty`、`corner_edit_dirties_every_diagonal_neighbour`、`test_top_corner_samples_the_highest_quart_layer` |
| **T-01（Rust/Python）** | 资产发现补 `../Minecraft_Source` / `../Minecraft_Unpacked`；存档补 `../mtk-save-world`；跳过统一经 `mtk_testkit::skip()` 打印 `SKIP`，`MTK_REQUIRE_REAL_ASSETS=1` 时失败；CI 在真包齐备时自动开启该门禁 | `crates/mtk-testkit/src/lib.rs`、`../MoziToolKit/tests/_assets.py`、`.github/workflows/ci.yml` |
| **C-07** | `load_atlas_definition` 现在合并**所有包**的 `atlases/*.json`（低→高优先级，与 MC `getResourceStack` 的 `Lists.reverse` 一致）；同一份文件在多个包里出现时各自贡献 source，重复精灵由收集阶段去重 | `mtk-resource/tests/test_atlas_definition_merge.rs`（7 个测试）；审计 §23 |
| **C-08a** | `load_atlas_category` 返回 `Result`：只有 `NotFound`（没有任何包提供该文件）才回退到内置默认定义，读取/解析失败直接上抛（错误含包名与路径） | `mtk-resource/tests/test_atlas_definition_errors.rs`（4 个测试）；审计 §24.1 |
| **C-08b** | `pack.mcmeta` 的 `overlays` 已实现：`overlays.entries[].{formats, min_format/max_format, directory}` 按 `CURRENT_CLIENT_RESOURCE_FORMAT`(=88) 判定适用性；适用 overlay 作为更高层参与 `open`/`list_files`/`content_entries`，声明越靠后优先级越高。目录包与 zip 包都支持，路径指纹走同一套解析 | `mtk-resource/src/pack/mcmeta.rs`、`tests/test_pack_overlays.rs`（9 个）、`tests/test_pack_fingerprint.rs`（overlay 路径/栈指纹等值 + version.json 常量校验）；审计 §24.2 |
| **C-08c** | `pack.mcmeta` 的 `filter.block` 已实现：`IdentifierPattern` 正则列表（与 atlas filter 共用类型），`open_asset_raw` 命中后停止查找（先查本包资源），新增 `list_asset_files` 按 `filterAll` 语义剪枝；模型扫描 / CTM / 横幅 / biome 扫描全部改走合并列表 | `mtk-resource/src/identifier_pattern.rs`、`pack/mcmeta.rs`、`pack/stack/mod.rs`、`tests/test_pack_filter.rs`（7 个）；审计 §24.3 |
| **C-02** | `Filter` 的 JSON 形状原本是错的（`pattern` 被 `flatten`，vanilla 的嵌套形状解析成"匹配一切"→ 清空整个图集）；现按 `SourceFilter.pattern` 解析为 `{namespace?, path?}` 正则，用 `find()` 语义匹配，`pattern` 必填。顺序语义经 MC 源码确认是 vanilla 行为（`SpriteSourceList.list` 逐个 source `removeAll`），审计原建议的"两阶段"是错的 | `mtk-resource/src/atlas/definition.rs`、`pack/stack/collector.rs`、`tests/test_atlas_filter.rs`（8 个测试）；审计 §22 |
| **K-06** | `ResourcePack::open` 改为 `Result<Option<Vec<u8>>>`（`Ok(None)`=没有，`Err(ReadFailed{pack,path})`=在包里但读不出），全链路传播到 `open_asset_raw`/`open_texture_raw`/`open_model_raw`/`resolve_pbr_companions`/`load_ctm_rules`/`build_ctm_solver`/`BiomeResolver::load_from_pack_stack`/`process_paletted_permutations`/预烘模型扫描与 CLI `AssetStack`；删除零消费者的 `has_file` | `mtk-resource/tests/test_pack_read_errors.rs`（3）、`libmtk/tests/test_pack_read_failures.rs`（3）；审计 §21 |
| **C-01** | 烘焙时按 `.mcmeta` 的 `frames` 重排/重复条带（静态 sheet 取第一个播放帧），导出元数据清空 `frames`（防止宿主二次应用）；缓存 v12→13、场景包 v4→5。原版实证：`lava_still` 20 帧图片 → 38 帧序列，`prismarine` 22 帧 | `mtk-texture/src/atlas/builder/mod.rs::apply_animation_frame_order`、`tests/test_animation_frames.rs`（5 个测试）；真资产断言在 `libmtk/tests/test_package_determinism.rs`；审计 §20 |
| **K-04** | 新增 `mtk_core::serialize_sorted_map`（排序后仍以 map 编码，线上格式不变）；接入 11 处 HashMap 序列化（`AtlasAddressMap`×3、模型库 payload×2 与 `BakedElement.faces`、`BiomeResolver`×5、`ResolverOverrides`×2、`extra_props`）；场景包 `entity/slots` 与横幅图案块改 `BTreeMap` | `crates/mtk-core/src/serde_util.rs`、`mtk-texture/tests/test_address_map_determinism.rs`、`libmtk/tests/test_package_determinism.rs`（合成 + 真资产两组）；审计 §19 |
| **C-06** | 生产配置收敛到 `AtlasBuilderConfig::production()`：`padding = max(2, 2^mip)`（mip 0 时为 2），`default()` 也走生产策略；预烘/场景导出/CTM/atlas CLI/`PyAtlasBuilder` 全部改用；缓存格式 v11→12、场景包 v3→v4 | `mtk-texture/src/atlas/builder/types.rs`、`tests/test_atlas_padding.rs`（3 个新测试）、`libmtk/src/scene/exporter/tests/basic.rs`（改为像素级 crop 方向断言）；审计 §18 |
| **quart 契约** | `SectionBiomes::is_fully_assigned()` 现在把单元素调色板视为权威（`uniform`/2D 存档/单 biome 流），不再把"合法的单 biome 段"误判成占位泄漏 | `mtk-voxel/src/storage/biome.rs` + 重新启用的 `mtk-sync/tests/test_save_alignment.rs::test_real_save_import_and_live_sync_align`（此前一直被跳过） |
| **Y-07** | `server/service/{state.rs,server.rs}` 全面改用 `lock_unpoisoned()`（`PoisonError::into_inner`），持锁 panic 后握手线程与服务端恢复正常运行 | `mtk-sync/src/server/service/state.rs` + 单测 `handshake_context_recovers_after_selection_mutex_poisoning` |
| **Y-08** | `mtk-sync/src/client/transport.rs` 新增 `drain_stale_commands()`，在建连发送 `CONNECTED` 前与断线时清空残留命令队列 | `mtk-sync/src/client/transport.rs` + 单测 `reconnect_drains_stale_commands_queued_while_disconnected` |
| **K-05** | `model_cache` 仅存全限定 `loc.as_string()`，删除 `.or_else(\|\| model_cache.get(&loc.path))` 跨命名空间回退，仅对无冒号原版短名补 `minecraft:` | `libmtk/src/prebake/models.rs`、`mtk-model/src/baker/model_baker/resolver.rs` + 单测 `mod_namespace_model_never_falls_back_to_vanilla_short_name` |
| **V-08** | 全空section早退改为 `padded.is_empty && block_entities.is_empty()`，全空气段中的方块实体可正常烘焙 | `mtk-voxel/src/mesher/{section_mesher.rs,batch.rs,delta_mesher.rs}` + 单测 `empty_flagged_section_with_block_entity_still_emits_geometry` |
| **A-11 / mtk-py CI** | 清理 `buffer.rs` `#[allow(dead_code)]` 与死枚举变体；修复 `bindings/mtk-py/tests/` 并接入 `.github/workflows/ci.yml`；`API_VERSION` 升至 `3` | `bindings/mtk-py/src/{lib.rs,buffer.rs}`、`bindings/mtk-py/tests/test_bindings.py`、`.github/workflows/ci.yml` |
| **A-16** | 删除 `../MoziToolKit/utils/assets/cache_utils.py` 兼容再导出 shim，将宿主 OS 缓存统计/清理拆分至 `utils/assets/cache_ops.py` | `../MoziToolKit/utils/assets/cache_ops.py`、`rg cache_utils ../MoziToolKit` 为 0 |
| **H-05 / H-06** | `utils/assets/guard.py` 缺失原生核心时返回 `"NO_NATIVE"`（不返回 `"MISSING"`）；`ensure_pipeline_assets_ready_sync` 与 `run_task_with_asset_guard` 改为显式白名单状态才放行（fail-closed） | `../MoziToolKit/utils/assets/{guard.py,guard_eval.py}`、`../MoziToolKit/tests/test_asset_bake_guard.py::test_status_no_native_and_fail_closed_guard` |
| **H-07** | 修复 `open_active_asset_cache` 包堆栈变更盲区（`_ACTIVE_CACHE_PACK_SIG`），将 `load_*_inner` 的缓存键获取移至打开缓存之后，并同步修复材质构建链路向 `get_or_create_image` 透传 `prefs` | `../MoziToolKit/bridge/assets/{cache.py,readers.py}`、`../MoziToolKit/utils/materials/builder/`、`../MoziToolKit/tests/test_asset_cache_invalidation.py` |
| **H-10** | 删除 `bridge/engine.py`、`bridge/assets/{cache.py,pack.py}`、`bridge/sync/session.py`、`bridge/progress.py` 中对 `API_VERSION=3` 的 `hasattr`/`except TypeError` 运行时探测 | `../MoziToolKit/bridge/`、`../MoziToolKit/tests/test_live_sync_reconnect_state.py` |
| **T-09** | 测试前置注册 `MOZI_AddonPreferences` 条目并优化 `bridge/prefs.py` 类型解析，消除 5 条 `AddonPreferences not available` 跳过（含 `--factory-startup`） | `../MoziToolKit/tests/conftest.py`、`../MoziToolKit/bridge/prefs.py`、`test_dna_bridge.py` + `test_launcher_scan.py` 0 skipped |

### D1–D5 期间落地、本轮未逐行复核（按记录保留，不算 OPEN）

`M-01`（物品三级几何来源 + `bake_failures`）、`M-02`（`get_item` 不再模糊回退）、
`M-03`（方块/物品两侧失败可观测）、`M-04`（模型库生产面 `get_mesh`/`get_item_transforms`）、
`M-07`（custom_name 文本组件）、`M-08`（`minecraft:base_color` + 最长染料名）、
`M-09`（精确 id 表，`flower_pot` 不再误判）、`M-10`（删 `dyed_color/custom_name/has_glint`）、
`M-11`（物品头颅按 id 判定）、`M-12`（删 `BlockEntityData::Sign`）、
`S-01/S-02/S-05/S-06/S-07/S-08/S-09/S-10/S-14/S-15/S-16/S-17`、
`A-01`（生产 dev operator 不再依赖 `unstable.testing`）、`A-02`（删零消费者导出）、
`T-08`（物品模型/变换覆盖）、`J-05/J-06`（1.20.x 画尺寸、JSON 转义，属 2.1）。

要做时先按"复现 → 测试"补一次，别直接相信记录。

---

## 3. VOID（2.0 不再需要，别再开工）

`81ba2aa` 把连续空间实体（画作 / 展示框 / 盔甲架）整条流水线移到 `release/2.1`，
`PROTOCOL_VERSION=4`、`SCENE_PACKAGE_FORMAT_VERSION=2`、0x13/0x14 永久预留。

| 条目 | 原因 |
|---|---|
| `M-05`（展示框缺模型告警）、`V-09`（框内物品平面 / Euler 顺序） | 实体渲染计划已删除 |
| `Y-04`（baseline 期间 staging）、`Y-05`（baseline 删除事件）、`Y-12`（visual payload 降级）、`Y-15`（`custom_name` 上线路由） | 视觉实体包/载荷已删除 |
| `J-01..J-04`、`J-07`（yefira 视觉实体并发/变更检测/画 variant/UUID） | 归 `release/2.1` |
| `H-01/H-02/H-03`（`utils/entities/reconciler.py` 材质/显隐/属性残留）、`H-11..H-14` 中实体相关部分 | `MoziToolKit/utils/entities/` 已不在 2.0 插件中 |
| 被推翻的假阳性：`V-01`（AO 凹角）、`M-06`（Euler 顺序）、`M-13`（`scale` 标量） | 已用 26.2 源码/实测否定 |

---

## 4. OPEN（按风险排序）

P0 = 崩溃/数据错/安全；P1 = 用户可见错误或确定性；P2 = 边界/清理。

### B-安全与协议

| ID | P | 位置 | 现象 | 修法 | 验证 |
|---|---|---|---|---|---|
| **S-13** | P0 | `bindings/mtk-jni/src/server.rs`（8 处 `if handle == 0`） | 句柄只校验非零，无句柄表/关闭态原子标志；关闭竞态下 poller 线程读已释放对象 | 句柄表 + `AtomicBool`/代际号，关闭后所有入口返回错误 | JNI 侧并发关闭测试（或 Rust 单测句柄生命周期） |
| **Y-06** | P0 | `mtk-sync/src/server/service/server.rs:175` | `snapshot_uuid: [0u8; 16]` —— 所有 baseline 共用同一 UUID，旧 chunk 会被新流接受 | 用进程内自增/真随机 16 字节并随每次 baseline 变化 | 两次 baseline 的 UUID 必须不同；混入旧 chunk 必须被拒 |
| **Y-10** | P2 | `mtk-sync/src/server/baseline.rs:389` | `space_left.max(biome_len)` 恒等于全长 → `max_chunk_size` 失效、续块分支死代码 | 改 `min` | 大 baseline 产生多块（现有 loopback 基线） |
| **Y-11** | P2 | `server/service/server.rs:331,335` | `block_states[i]` 无长度校验（panic）；palette > 65535 时 `as u16` 静默回绕 | 校验长度 + 超限报错（或升 u32） | 构造超长/超量 palette |
| **Y-13** | P2 | `server/service/server.rs:237,324` | `sequence.fetch_add` 与 `broadcast_packet` 不在同一临界区 → 并发提交序号乱序 | 序号+广播同临界区 | 并发提交压力测试后客户端无 `SequenceGap` |
| **Y-14** | P2 | `server/service/worker.rs` | HELLO 能力位/订阅记录后从不使用 | 按订阅过滤广播，或删除记录字段 | 能力位为 0 的客户端收不到保留包 |

### B-正确性（可见渲染错）

| ID | P | 位置 | 现象 | 修法 | 验证 |
|---|---|---|---|---|---|
| **V-02** | VOID | 焊接键加面朝向/法线：仓库内**无消费者**（Blender 的 `inject_vertex_normals_dna` 是 stub，`MeshData.normals` 不进渲染；无 glTF 导出器），而拆分顶点会让 90° 拐角失去焊接、SSS 出黑边、顶点 +5.7%。已撤回（`git revert`），焊接键保持"仅位置" | 审计 §27 |
| **V-07** | P2 | `mtk-voxel/src/fluid/height.rs:85` vs `:252` | 两套流体高度公式，waterlogged 分支不一致 | 收敛成一份（`get_fluid_base_height` 调用 `calculate_fluid_corner_heights`） | 同一批 waterlogged 状态两函数结果一致 |
| **C-03** | P2 | `collector.rs:13-46` + `ctm/solver.rs:136` | CTM 同优先级排序依赖文件枚举序 | 稳定排序（pack 序 + 文件名） | 打乱目录枚举起始位置后结果不变 |
| **C-04** | P2 | `libmtk/src/prebake/engine.rs:204-206` | 请求 `blocks` 分类仍烘全部图集 | 按分类过滤 | 只请求 blocks 时 manifest 的 atlas 分类只有 blocks |
| **C-05** | P2 | `libmtk/src/prebake/engine.rs:308-311` | 进度 total 与 `inc()` 次数不同源 → 百分比 >100% | 用同一计数源 | 进度回调断言 `current <= total` |

| **C-11** | P1 | `mtk-resource/src/ctm/parser.rs` | 覆盖层方法（`overlay`/`overlay_ctm`/`overlay_random`/`overlay_repeat`/`overlay_fixed`/`overlay_horizontal`…）目前一律丢弃：解码器不产出额外的混合层，映射到基础方法会**静默替换**贴图（比不做更差） | 需要 mesher 支持附加四边形 + `tintIndex`/`layer`/AO + 17 格索引（Continuity `StandardOverlayQuadProcessor`/`SimpleOverlayQuadProcessor`） | 见审计 §29.4 |
| **C-12** | P1 | `mtk-resource/src/ctm/solver.rs` | `connect=tile` 实际等价于 `connect=block`；`connectTiles=`/`connectBlocks=` 列表在连接判断中被忽略（Continuity：状态相等 **或** 邻居该面贴图集合包含本面贴图） | 把邻居的面贴图传进 `check_connect`（mesher 侧已解析 `PaletteMeshingData`，需扩 `resolve_face_with_tangents` 的回调签名） | 见审计 §29.4 |
| **C-13** | P2 | `mtk-resource/src/ctm/{parser,solver,algorithms}.rs` | `random` 用 OptiFine 式坐标哈希 + 浮点权重（Continuity：Minecraft `RandomSource` + 整数权重 + `linked`/`randomLoops`/`symmetry`）；`repeat` 相位公式与 `orientation` 旋转不同（Continuity 逐面 `x/y/z` 公式含 `-1` 偏移）；`top` 未按 `AXIS` 属性选轴；规则排序按 `weight` 数值（Continuity：`prioritized` 布尔 → 包优先级 → resourceId） | 逐项对照重写 | 见审计 §29.4 |

### B-缓存与确定性

| ID | P | 位置 | 现象 | 修法 | 验证 |
|---|---|---|---|---|---|
| **K-03** | P2 | `libmtk/src/prebake/resolver.rs:24-37,63-77` | Rust API 在 `fingerprint=None` 时仍返回"最新包"（主机侧已不再这样调用，且注释写明仅供内省） | 让 `resolve_active_cache_package` 要求指纹；内省走 `newest_cache_package` | 单测：None 指纹返回 None |
| **K-08** | P2 | `mtk-package/src/writer.rs:260,330-355` | `sync_all` 失败被吞；目录不 fsync → 崩溃留下 0 字节包 | 失败即报错 + 目录 fsync | 注入写失败 |
| **K-09** | P2 | `libmtk/src/prebake/reader.rs:28-45` | 非 `mtk_fp_<hex>` 指纹被截断 16 字节 → 前缀相同即相等 | 严格格式校验 | 非法/短指纹被拒 |
| **K-10** | P2 | `mtk-package/src/toc.rs` / `error.rs:38` | TOC offset 不校验对齐/越界/重叠；`chunk_count` 不与条目数比对；允许重复 id | 加载时全量校验 | 合成畸形 TOC |
| **K-11** | P2 | `libmtk/src/scene/metadata.rs:20-56` | 清单字段全 `#[serde(default)]` → 缺字段被当 false/0 | 必需字段不给 default | 删字段后反序列化失败 |
| **K-12** | P2 | `libmtk/src/scene/package/validator.rs:35`、`exporter/source.rs:165-200` | 仍读 `biome/resolver` 旧键，写入方只写 `biome/mapping` → 死分支 | 删除旧读 | `rg biome/resolver` 零生产引用 |

| **C-09** | `CANONICAL_BIOMES` 改为由 `tools/gen_biome_palettes.py` 从 jar 数据生成，并新增 `test_biome_matches_jar.rs` 五项对照（id 集合、温度/湿度/水色、显式 `effects.*_color`、派生色 == colormap 采样、`grass_color_modifier` 规则）。修掉：`sparce_jungle` 幽灵条目（拼写错误，jar 里不存在）、缺失的 `sulfur_caves`、11 处派生色 ±1~4 漂移 | `tools/gen_biome_palettes.py`、`crates/mtk-material/src/biome/canonical_data.rs`、`crates/mtk-material/tests/test_biome_matches_jar.rs`；审计 §25 |
| **C-10** | `grass_color_modifier` 数据化：`BiomePalette.grass_color_modifier`（None/DarkForest/Swamp，生成器写入），`has_custom_grass` 只表示"jar 显式声明"；运行期按 modifier 分支（dark_forest 采样后 packed 变换、swamp 用噪声色），删除 `pal.id == "swamp"` 硬编码；导出的 tint type 仍为 HARDCODED（宿主无法自行应用 modifier）；顺带删掉 py 绑定里复制的 tint 表 | `mtk-material/src/biome/palettes.rs`、`mtk-voxel/src/{biome/color.rs,mesher/shading/resolver.rs}`、`bindings/mtk-py/src/material/biome/tint.rs`；审计 §26 |

### B-架构边界

| ID | P | 位置 | 现象 | 修法 |
|---|---|---|---|---|
| **A-03** | `mtk-voxel` 不再有环境探测与进程级调试世界缓存：删除 `VoxelStorage::create_debug_world()` / `CACHED_DEBUG_STORAGE` / `store_debug_world` / `clear_debug_world_cache`，核心只接受调用方传入的 states/pack stack/model db/dir；`ci_guardrails` 新增"核心 crate 不得读宿主环境变量"检查；删掉永远 skip 的 `test_debug_world_golden.rs`，换成结构不变量测试 | `tools/ci_guardrails.py`、`mtk-voxel/src/storage/world/debug.rs`、`tests/test_debug_world_layout.rs`、`bindings/mtk-py`（Py 入口改为显式输入）、MoziToolKit `bridge/debug.py`；审计 §28 |
| **A-04** | P1 | `mtk-voxel/src/world/dynamic_entities.rs:130`（async）vs `:268`（sync） | skull/banner 扫描+解析两份实现、5 个公开入口 | 收敛一份 + 一个入口（与"voxel 去实体化"一起做，见审计 §5.1） |
| **A-05** | P1 | `mtk-save/src/loader.rs:181,194` | 两个近似入口（`load_box_into_storage` / `_with_progress`） | 保留一个 + 显式配置 |
| **A-06** | P1 | `bindings/mtk-py/src/voxel/storage.rs:39-96` | `create_debug_world*` 5 个入口 | 收敛成一个 |
| **A-07** | P1 | `bindings/mtk-py/src/sync/client.rs` | `start` / `start_with_cache`（11 参数 + 哨兵默认值） | 一个入口 + 配置 DTO |
| **A-08** | P2 | `bindings/mtk-py/src/scene/exporter.rs:20-46` | `**kwargs` 吞未知键 | 显式参数，未知键报错 |
| **A-09** | P2 | `bindings/mtk-py/src/uv/fluid.rs:41-170` | 一个能力 3 个入口 + 4 段类型探测 | 一个入口 + 类型校验 |
| **A-10** | P2 | `bindings/mtk-py/src/voxel/world.rs:159` | `get_world_mesh()` 未 meshed 时返回空 `MeshData` | 返回 `Option`/`Err` |
| **A-12** | P2 | `mtk-voxel/src/fluid/test_verify_blender.rs` | 测试模块住在 `src/`，4 个字段从不读取 | 移到 `tests/` |
| **A-13** | P2 | `bindings/mtk-py/src/texture/atlas.rs:266-279`、`uv/extrude.rs:40-45` | 未知 `category`/`uv_mode` 静默回落 | 报错 |
| **A-14** | P2 | `mtk-save/src/error.rs:31` | `UnsupportedDataVersion` 从不构造；1.17 存档静默 `Ok(0 sections)` | 加载时校验 `DataVersion` |
| **A-15** | P2 | `mtk-save/src/entity.rs` | 已随 2.0 删除 entity 扫描 → **待复核是否仍存在** | 若已删则转 DONE/VOID |
| **A-17** | P2 | `bindings/mtk-py/src/mesh/mod.rs:6-8,78-86` | `pub mod` + `pub` 字段 + `inner()/inner_mut()` 泄漏到 crate 外 | 收成 `pub(crate)` |

### B-测试体系

| ID | P | 位置 | 现象 | 修法 |
|---|---|---|---|---|
| **T-02** | P1 | `../MoziToolKit/tests/_assets.py` | import 时设 `MTK_TEST_SANDBOX=1` → 指纹校验路径从不被覆盖 | 只在明确需要时开；指纹路径单独跑一遍 |
| **T-03** | P1 | `../MoziToolKit/tests/test_item_frame_render_plan.py` | `glob(".ci-cache/*.mtkcache")[0]` 顺序不定 | 按指纹/env 明确选取（该文件是否仍存在需确认） |
| **T-05** | P1 | 同上 | 原生模块缺失时 `skipTest` | 缺原生模块直接 fail |
| **T-06** | P1 | 全仓 | 没有"损坏输入"回归套件 | 每个 S-xx 项自带一个（S-01/S-02/S-05/S-14/S-15 已有，补 S-13/包/TOC） |
| **T-07** | P2 | `../MoziToolKit/tests/test_api_surface_contract.py` | 只断言符号存在 | 补行为断言 |

### B-消费端（Blender 插件）

| ID | P | 位置 | 现象 | 修法 |
|---|---|---|---|---|
| **H-04** | P1 | `bridge/assets/readers.py:167` | biome resolver 缺失时静默用默认 66 色表 → 全图 Plains 染色 | 报错或明确降级标记 |
| **H-08** | P1 | `utils/materials/builder/world_binding.py:196-206` | 单 chunk 快路径把全部多边形塞进同一槽 | 用面属性 |
| **H-09** | P1 | `utils/save/hierarchy.py:195-201` | 世界 bounds 由 Python min/max 重算 → 实体偏移 | 用 `storage.bounds` |

> 本轮实测还发现一条新的插件侧性能缺陷（不属于正确性，见审计 §2.1-3/4）：
> `ensure_world_materials` 每次注入都重写 33.6 万面的 `material_index`；且指纹缺失时
> `get_world_pipeline_assets` 不缓存、每次重解析 330–400 ms。归入 B2 性能批。

---

## 5. 未复核（不要当成 DONE）

- `M-03` 的 **M-03b 方块侧失败列表**是否仍覆盖新加的 chest/shulker 原生生成器；
- `K-07`（atlas 精灵枚举顺序）与 `K-04` 是否同一处；
- `A-15`（`mtk-save/src/entity.rs` 删除后的残留调用）；
- `H-14`/`H-12`/`H-13`（重复实现）在 2.0 删除实体目录后的实际剩余量；
- `mtk-core` 的 `subdivide/*`、`clip`、`extrude` 退化几何；`mtk-cull` 的
  `coplanar.rs:97` 未保护 `normalize()`（NaN → 共面检测静默失效）；
- `mtk-material` 的 PBR 预设 / `mtk-texture` 的图集内存预算（审计 §6 已给方向，未复现）。

---

## 6. 历史记录去哪了

本文件旧的 D1–D5 叙述、逐条 ❌/✅ 状态与大段"代理问题复核"已随
`CODE_REVIEW_2026-11.md` 一起归档；那些内容的价值是"证据"，不是"待办"。
后续只需要维护 §2（DONE，带复核方式）与 §4（OPEN，带复现方式）。
