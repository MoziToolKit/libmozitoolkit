# 修复清单（逐项勾选）

生成时间：2026-11-16 · 基线：`libmozitoolkit a43a111` / `yefira 102a170`
配套报告：`CODE_REVIEW_2026-11.md`

---

## 怎么用这份清单

每一项都是**可独立执行的一格**：

```
复现 → 改 → 加测试（先红后绿） → 跑该行的验证命令 → 勾掉
```

规则（来自 AGENT.md，别绕过）：

- 一次只做一项，不要顺手重构；否则出问题无法归因。
- 每项必须补一个**在修之前会失败**的测试。
- 不用兼容层、旧格式双读、`#[allow(dead_code)]`、`unwrap_or_default()` 掩盖失败。
- 改完 `libmozitoolkit` 任何 crate 或绑定后必须 `cargo build-all-py`（下游 `dev/lib` 才会更新）。

状态列：`✅` = 我已用真实资产/真实数据复现；`◻` = 子审计报告指出、**尚未复现**（动手前先复现，本轮已出现 2 条假阳性）。

验证命令缩写：

- `FMT` = `cargo fmt --all -- --check`
- `CLIP` = `cargo clippy --workspace --all-targets -- -D warnings`
- `TEST` = `MTK_TEST_ASSETS=../Minecraft_Source cargo test --workspace --no-fail-fast`
- `BLD` = `cargo build-all-py`；`BLEND` = `blender --background --python tests/run_tests.py -- -rs -q`（要求 0 skipped）

---

# B0 · 立刻（阻塞物品/实体主功能）— 约 1.5 天

这一批直接对应用户感知到的"3D 物品大小和朝向全靠猜"。

| ID | P | 位置 | 现象 | 修法 | 验证 | 状态 |
|---|---|---|---|---|---|---|
| **M-01** ✅ DONE | P0 | `crates/libmtk/src/prebake/models.rs` `bake_single_item` | **46 个原版物品模型完全没进缓存**（1271 个 item JSON 里缺 46）。原因：`resolved.elements` 为空且无 `layer0/1..` 贴图时 `combined_model=None`，整项被 `baked?` 静默丢弃。这些正是 `builtin/entity` / BEWLR 物品 | 通用规则（不做逐物品 hack）：`elements` 非空 → 现有路径；否则有 `layer*` → PNG 挤出；**否则回退到 `block/<path>` 几何并保留 item 链的 `transforms`/`gui_light`**；仍无几何 → 按 M-03 记录失败。二选一例外表见下 | 新增 `mtk-model/tests/test_item_model_coverage.rs`：遍历 `../Minecraft_Source/assets/minecraft/models/item/*.json`，除白名单外全部必须能 bake | ✅ |
| **M-02** ✅ DONE | P0 | `crates/mtk-model/src/baker/baked_model/database.rs:get_item` 末尾 `self.get(clean)` | `get_item("minecraft:item/chest")` **静默返回 `minecraft:chest` 方块模型**（3 元素、18 面、`transforms == default`）。玩家看到的就是身份变换 → 大小/朝向错。这是"猜"的来源 | 删除 `get_item` 对方块状态模糊匹配的回退。方块物品只允许**精确** `item/<name>` → 显式 `block/<name>` 两步，且必须带 display 数据；找不到就 `None` | 单测：`get_item("minecraft:item/chest")` 在 M-01 修复前必须 `None`（而不是返回方块）；`get_item("minecraft:stone")` 仍能解析 | ✅ |
| **M-03** ✅ DONE | P0 | `prebake/models.rs` / `prebake/engine.rs` | 烘焙失败无任何可观测信号：`if let Ok(...)` 链 + `baked_models: N` 计数，用户不知道少了什么 | 预烘结束时收集 `Vec<ModelBakeFailure { key, reason }>`，写入 manifest（或日志），并在 Python 侧可读；`baked_models` 与实际条目数必须一致 | 故意放一个坏 item JSON，断言失败列表里出现该 key | ◻ |
| **M-04** ✅ DONE | P0 | `bindings/mtk-py/src/model.rs` | 模型库对 Python 只有 `__len__`/`is_empty`，**拿不到网格、拿不到 display transforms**；唯一入口在 `unstable.testing` | 生产面加 `BakedModelDatabase.get_mesh(key, clip_hidden=True, weld=False)` 与 `get_item_transforms(item_id)`（返回 `{context: {rotation, translation, scale}, gui_light}`）；删掉 testing 里的同名函数 | `cargo test -p mtk-py` + `tests/test_api_surface_contract.py` 补正向断言 | ✅ |
| **M-05** ✅ DONE | P0 | `crates/mtk-model/src/entity/item_frame.rs:326-380` | 展示框只会用 `transforms.fixed`；物品模型缺失时静默用单位变换，没有"这个物品我拿不到模型"的告警 | `EntityBaker` 在 `model_lookup` 与内置模型都失败时，往 `EntityRenderPlan.warnings` 推 `item_model_missing`（需要把 warning 通道传进 baker，或让 baker 返回 `Result`） | 渲染计划测试：未知 item_id 必须产生 warning 而不是静默 0 面 | ✅ |
| **M-06** | P1 | `crates/mtk-model/src/entity/render.rs:132` | 实体旋转按 `EulerRot::XYZ` 组合，MC 是 `Ry(yaw)·Rx(pitch)·Rz(roll)`；pitch≠0 时姿态错（实测 pitch=30°/yaw=45° 偏差明显）。当前展示框/画走 `facing` 未暴露，盔甲架一旦开烘就会错 | 统一走四元数：`q_mc = YXZ(yaw,pitch,roll)`；导出时分解成宿主 XYZ；`mtk-save::mc_rotation_to_model_euler` 同步改成"MC 矩阵 → XYZ 分解" | 新增 3 轴旋转测试（现在 `render.rs:826` 用 `rotation=ZERO`，永远发现不了） | ✅ |
| **M-07** | P1 | `crates/mtk-save/src/block_entity.rs:419-428` | `custom_name` 只认 `String`；1.20.5+ 实际是 JSON 文本组件，Compound 形态直接丢、JSON 形态返回带引号的脏值 | 解 Text Component：`{"text":...}` / `{"translate":...}` / 纯字符串，取纯文本；Compound 形态也支持 | 单测：三种形态都断言得到 `Hero Armor` | ✅ |
| **M-08** | P1 | `crates/mtk-save/src/block_entity.rs:93-100,313-333` | 不读 1.20.5+ `minecraft:base_color`，改用物品 id 前缀猜 → `light_blue_banner` 得 `"light"`，`shield` 永远白 | 优先读组件，其次 id 前缀；前缀必须按**最长匹配**（`light_blue` 先于 `light`） | 单测：`light_blue_banner`、`shield`、`white_banner` | ✅ |
| **M-09** | P1 | `crates/mtk-save/src/block_entity.rs:344,372` | `clean_id.contains("pot")` / `contains("banner")` 启发式：`flower_pot` 被当装饰陶罐、`*_banner_pattern` 被当旗帜 | 改成精确 id 表（`decorated_pot`、`*_banner`/`*_wall_banner`、`*_head`/`*_skull`） | 单测：`flower_pot` 必须 `None` | ✅ |
| **M-10** | P1 | `crates/mtk-model/src/entity/instance.rs:394-410` | `ItemComponentsData` 的 `dyed_color` / `custom_name` / `has_glint` 解码后**零消费者** | 按规则 1 删除这三个字段与解码（接消费者是另一个功能，单独开项）；保留 `block_entity` | `rg` 零引用 + 全绿 | ✅ |
| **M-11** | P1 | `crates/mtk-save/src/block_entity.rs:179-196` | 无 profile 时 `SkullKind` 默认 `Skeleton`，`zombie_head`/`creeper_head`/`dragon_head` 全变骷髅 | 按 id 判定（`SkullKind::from_block_name` 已存在，接上） | 单测 7 种头 | ✅ |
| **M-12** | P2 | `crates/mtk-save/src/block_entity.rs:14-25` | 告示牌从不解码，而 `BlockEntityData::Sign` 在生产路径无生产者 | 要么实现 sign 解码并接烘焙，要么删掉 `Sign` 变体 | `rg BlockEntityData::Sign` 无生产调用即删 | ◻ |

---

### M-01 附：46 个缺失物品的分类（已逐个比对真实 JSON）

| 类别 | 数量 | 物品 | 处理 |
|---|---|---|---|
| **A. 抽象模板，本就不是物品** | 13 | `air`, `generated`, `handheld`, `handheld_mace`, `handheld_rod`, `spear_in_hand`, `template_banner`, `template_bundle_open_back/front`, `template_chest`, `template_copper_golem_statue`, `template_music_disc`, `template_shulker_box`, `template_skull` | 白名单，测试里排除 |
| **B. 有方块模型，回退即可** | 27 | `chest`, `ender_chest`, `trapped_chest`, 4 种铜箱, 18 种潜影盒, `conduit`, `decorated_pot` | 走 M-01 通用规则：`block/<name>` 几何 + item 链 transforms |
| **C. 已有内置构造器可复用** | 1 | `dragon_head`（`SkullKind::Dragon` + `template_skull` 的 display） | 接 `BuiltinModelRegistry` |
| **D. 需要专用几何或显式不支持** | 5 | `shield`, `shield_blocking`, `trident_in_hand`, `trident_throwing`, `amethyst_bud` | 先记为 `unsupported_builtin_item` 并进 M-03 失败列表；以后单独排期 |

注：`item/amethyst_bud.json` 自带 display 但无 textures/elements，走哪条都不通；它属于 3D 装饰类 BEWLR，别硬猜。

---

# B1 · 安全（恶意/损坏输入会让宿主崩溃）— 约 1.5 天

这些都来自真实可构造的输入，不是理论问题。**先复现再修**。

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **S-01** | P0 | `mtk-save/src/nbt/reader.rs:173-263`（12 处） | `Vec::with_capacity(file_len)`：list 长度 `0x7FFFFFFF` → ~51GB 预分配 → 进程 abort（不是 `Err`） | 分配前用"剩余字节 / 最小元素大小"校验，越界返回 `NbtError::InvalidData` | ✅ |
| **S-02** | P0 | `mtk-save/src/region.rs:129-130` | `vec![0u8; chunk_len-1]` 信任文件内 4 字节长度；`sector_count` 解析了但不用 | 用文件长度 + sector 表双重限界；解压输出设上限（64MiB） | ✅ |
| **S-03** | P0 | `mtk-sync/src/client/session/assembler/mod.rs:214` | `with_capacity(decoded_target_bytes)` 来自线上 `u32`（≤4GiB）；zstd `read_to_end` 无上限 | 上界校验（≤ `MAX_SELECTION_VOLUME`），解压限流 | ◻ |
| **S-04** | P0 | `mtk-sync/src/client/session/assembler/mod.rs:190,233` | `total_chunks` 与累计 voxel 数无上界 → 无界内存增长 | 同 S-03 一个总预算 | ◻ |
| **S-05** | P0 | `mtk-package/src/reader.rs:186-200` + `toc.rs:210-216` | `zstd::decode_all` 无输出上限；TOC `decompressed_size` 不校验 → 解压炸弹 | 限流解码；校验 TOC 声明大小 | ◻ |
| **S-06** | P1 | `libmtk/src/prebake/reader.rs:236-243` | 用包内 chunk id 直接 `out_dir.join(...)` → 路径穿越（`../../.zshrc`） | 归一化 + 必须在输出目录内 | ◻ |
| **S-07** | P1 | `mtk-resource/src/pack/source.rs:168,193` | 伪造 zip entry size → 巨大预分配；`..` 检查可绕过 | 限制单 entry 大小；`Path::components` 校验 | ◻ |
| **S-08** | P1 | `mtk-sync/src/protocol/hasher.rs:146` | `parse_uuid_bits` 曾对非法输入返回 `(0,0)`、按字节切 `String` 可 panic | 已加严格 `parse_canonical_uuid`；**剩余**：`write_uuid` 仍用宽松版 → 改为传播 `Err` | ✅（一半） |
| **S-09** | P1 | `mtk-sync/src/protocol/codec/encode.rs:9` | `write_str` 写 `len as u16` 但追加全部字节，>65535 整帧错位 | 超限返回 `ProtocolError::StringTooLong`（变体已加），上游传播 | ✅ |
| **S-10** | P1 | `mtk-sync/src/protocol/codec/decode.rs:144` | 实体 JSON 解析失败 → `None` = "此处无方块实体" → **静默删除** | 改为协议错误（`InvalidPayload` 已加） | ✅ |
| **S-11** | P1 | `bindings/mtk-py/src/buffer.rs:136-146` | `PyBufferOwner` 钉住 Python 对象而非 `Vec`；attribute 替换/`clear()` 后旧 memoryview 读已释放内存 | owner 自持稳定分配（`Arc<[u8]>` 或拷贝），或在重分配前拒绝 | ◻ |
| **S-12** | P1 | `bindings/mtk-py/src/mesh/attributes.rs:107-122` | `PyBUF_SIMPLE` 不校验 `format/itemsize`（float64 被当 f32）；null buffer 走 `from_raw_parts(null,0)` | 校验并 `PyValueError`；0 长度用静态 dummy | ◻ |
| **S-13** | P1 | `mtk-jni/src/server.rs` 全部句柄 | 只校验 `!= 0`；`WebSocketServerManager.java:199` 与 `:186` 跨线程读/释放 → 关闭时 UAF | 句柄表 + 关闭态原子标志 | ◻ |
| **S-14** | P2 | `mtk-voxel/src/storage/section.rs:206` | `&self.palette[idx]` 无界（serde 无校验）→ 恶意 `.mtkscene` panic | `get` 越界返回 `Err` | ◻ |
| **S-15** | P2 | `mtk-voxel/src/storage/sparse.rs:262-265` | `columns.y[i]` 按 `columns.x.len()` 索引，serde 不校验长度一致 | 反序列化后校验 | ◻ |
| **S-16** | P2 | `mtk-package/src/header.rs:206-212` | 只比较 `version_major`，`version_minor` 不校验 | 严格匹配当前版本 | ◻ |
| **S-17** | P2 | `mtk-save/src/nbt/reader.rs:52,286,297` | `pos + n`、`len * 4/8` 在 32 位溢出 → 边界绕过 | `checked_*` | ◻ |

---

# B2 · 正确性（看得见的渲染错）— 约 2 天

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **V-01** | P0 | `mtk-voxel/src/mesher/ao.rs:156,167` | `shade_corner12/13` 返回 `shade0`（应为 `shade1`）→ 凹角两角漏光（0.8 而非 0.4） | 改返回值；补非 1.0 亮度用例 | ✅（代码明确） |
| **V-02** | P0 | `mtk-voxel/src/mesher/grid_pool.rs:61-64` | 顶点去重只按位置、忽略法线 → 焊接后 2/3 面法线是错的轴 | key 加 `normal`；或焊接后重算法线 | ◻ |
| **V-03** | P1 | `mtk-voxel/src/world/sync.rs:86` | `dirty_sections = dirty` 覆盖而非合并 → 同步前的脏段丢失，网格永久陈旧 | `extend` + 去重 | ◻ |
| **V-04** | P1 | `mtk-voxel/src/storage/world/container.rs:283-284` | 只改 biome 时不动 dirty → 染色永不刷新 | biome 变更也置脏 | ◻ |
| **V-05** | P1 | `container.rs:296-310` | 只标 6 正交邻居，padded apron 采样 8 对角 → 角落 AO/流体高度陈旧 | 标 26 邻居（或至少 18） | ◻ |
| **V-06** | P1 | `mtk-voxel/src/mesher/shading/resolver.rs:398` | `(cy >> 2) & 3` 在 `cy=16` 回绕到 0 → 3D biome 最上角采样最底层 | `min(cy>>2, 3)` | ◻ |
| **V-07** | P2 | `mtk-voxel/src/fluid/height.rs:82-100 vs 126-141` | 流体高度公式两份，waterlogged 分支不一致 | 收敛成一份 | ◻ |
| **V-08** | P2 | `mtk-voxel/src/mesher/section_mesher.rs:55` | 早退按 `non_air_count==0`：空气格里的方块实体（悬挂告示牌/头颅）完全无几何 | 有 block entity 也进 mesher | ◻ |
| **V-09** | P2 | `mtk-model/src/entity/item_frame.rs:343-380` | 展示框物品 `surface_offset = -0.015`、`BASE_ITEM_SCALE=0.5` 等魔数，与 vanilla `ItemFrameRenderer` 无对应注释；FIXED 变换的旋转用 `EulerRot::XYZ`（JOML `rotationXYZ` 是 Rz·Ry·Rx） | 抽常量 + 注明 vanilla 出处；用四元数对齐 JOML 顺序（当前只有铜像有 2 轴 fixed，风险低但别留坑） | ✅ |
| **M-13** | P1 | `mtk-model/src/parser/model_json.rs:37-45` | `scale: [f32;3]` 强制数组；大量资源包写 `"scale": 1.0`（标量）→ 整个模型 JSON 解析失败 → 模型静默缺失 | 自定义反序列化接受标量/数组 | ◻ |
| **M-14** | P2 | `mtk-model/src/item/generator.rs:462-520` | `default_item_generated_transforms()` 手工抄 vanilla `item/generated.json`，与真实 JSON 会漂移 | 从真实 JSON 读取，或加一致性测试 | ◻ |
| **C-01** | P1 | `mtk-resource/src/meta.rs:8-28` | `.mcmeta` 的 `frames` 解析了但从不使用，`frametime` 丢失 → 动画帧序/时长错 | 序列化/消费 `frames`，或删字段 | ◻ |
| **C-02** | P1 | `mtk-resource/src/pack/stack/collector.rs:176-190` | atlas `filter` 源用 `String::contains` 且顺序相关 → 行为随机 | 正则 + 顺序无关的两阶段 | ◻ |
| **C-03** | P2 | `mtk-resource/src/pack/stack/collector.rs:13-46` + `ctm/solver.rs:136` | CTM 同优先级排序依赖文件枚举序（HashMap 序）→ 每次结果可能不同 | 稳定排序（pack 序 + 文件名） | ◻ |
| **C-04** | P2 | `libmtk/src/prebake/engine.rs:204-206` | 请求 `blocks` 分类会烘全部 13 个图集（无单类选择） | 修分支条件 | ◻ |
| **C-05** | P2 | `libmtk/src/prebake/engine.rs:308-311` | 进度 total 与实际 `inc()` 次数不一致 → 百分比 >100% | 用同一计数源 | ◻ |
| **C-06** | P2 | `mtk-texture` 图集装箱 | **未审计**：重叠/边缘留白/HD 缩放 | 需补一轮审查 | — |

---

# B3 · 实时同步状态机（联机才会踩）— 约 1 天

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **Y-01** | P0 | `mtk-sync/src/client/session/handlers_delta.rs:88` | `PALETTE_UPDATE` 不推进 `state.sequence`，服务端却分配序号 → 每次"新方块状态"编辑都 `SequenceGap` → 无限 RESYNC | 按服务端序号推进（与其它包一致） | ◻ |
| **Y-02** | P0 | `handlers_delta.rs:102` | `DELTA_ENTITY` 同病（每个方块实体包烧一个序号） | 同上 | ◻ |
| **Y-03** | P1 | `handlers_delta.rs:149` | `rel_pos` 无范围校验，`i32::MAX` 溢出（debug panic / release 回绕） | 校验到选择框内 | ◻ |
| **Y-04** | P1 | `handlers_delta.rs:40` | baseline 期间只有 `DeltaOp` 进 staging；palette/entity/visual 立即应用后被 `sync_from_storage` 抹掉且不重放 → 大 baseline 期间编辑丢失 | 全类型 staging + commit 后重放 | ◻ |
| **Y-05** | P1 | `handlers_baseline.rs:216` | baseline 提交只发 upsert，不发删除 → 离线期间服务端删掉的实体在客户端永久残留 | 计算差集，产出 `deleted_uuids` | ◻ |
| **Y-06** | P1 | `server/service/server.rs:250` | `snapshot_uuid` 用 `Instant::now().elapsed()` ≈ 0 → 实际恒定 → 旧 baseline chunk 被新流接受 | 真随机/自增 UUID | ◻ |
| **Y-07** | P1 | `server/service/server.rs:157,170,228,239,306,407,433` + `state.rs:100` | `.lock().unwrap()`：任一持锁 panic 后所有握手线程永久 panic | 去毒化（`PoisonError::into_inner`）或 `parking_lot` | ◻ |
| **Y-08** | P1 | `client/transport.rs:149,263` | 断线时队列不清 → 重连后旧连接的命令（ACK/RESYNC/PONG）发给新连接 | 断开时清空命令队列 | ◻ |
| **Y-09** | P2 | `client/transport.rs:131` | 每次连接成功 `attempts=0` → "连上就断"的服务器导致无限重连 | 独立退避计数 | ◻ |
| **Y-10** | P2 | `server/baseline.rs:389` | `space_left.max(biome_len)` 恒等于全长 → `max_chunk_size` 失效，biome 续块分支死代码 | 用 `min` | ◻ |
| **Y-11** | P2 | `server/service/server.rs:331,335` | `block_states[i]` 无长度校验（panic）；palette >65535 时 `as u16` 静默回绕 → 错误方块 | 校验 + `u32` 或报错 | ◻ |
| **Y-12** | P2 | `protocol/codec/decode_visual.rs:52-63` + `jni/server.rs` | payload JSON 失败静默降级为 `Generic`，且 `Generic` 会把原始 payload 再发出去 → 永不自愈 | 已加 `InvalidPayload`；两处改用它 | ✅（变体已加） |
| **Y-13** | P2 | `server/service/server.rs:355-370` | `sequence.fetch_add` 与 `broadcast` 未串行化 → 并发提交时序号乱序 → 客户端 RESYNC | 序号+广播放进同一临界区 | ◻ |
| **Y-14** | P2 | `server/service/worker.rs:50-76` | HELLO 的 capabilities / subscribed 记录但从不使用 → v3.1 客户端仍收到 0x13/0x14 | 按订阅过滤广播 | ◻ |
| **Y-15** | P2 | `protocol/codec/decode_visual.rs:57` | 线上格式没有 `custom_name`，Rust 双向都硬编码 `None` | 协议加字段（需 Java+Rust 同步，单独排期） | ✅ |

---

# B4 · 缓存与确定性（"同输入两次烘焙不一样"）— 约 1 天

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **K-01** | P0 | `mtk-resource/src/pack/stack/mod.rs:197-213` | 栈指纹只哈希**包名**，不哈希内容 → 原地改包指纹不变，缓存被判 READY | 指纹纳入每个 pack 的文件列表+大小+mtime 或内容哈希 | ◻ |
| **K-02** | P0 | `libmtk/src/prebake/reader.rs:79-90` | 缓存有效性只看 64 字节 header，`CacheManifest.format_version` 从不校验 → 旧格式缓存被当新格式解析 | 校验 manifest 版本，不匹配即判失效 | ◻ |
| **K-03** | P0 | `libmtk/src/prebake/resolver.rs:63-77` | 不传指纹时直接返回目录里"最新"的 `.mtkcache`，零校验 | 必须显式指纹，否则报错 | ◻ |
| **K-04** | P1 | `mtk-texture/src/atlas/address_map/mod.rs:23,193`；`baked_model/database.rs:11`；`pack/stack/source.rs:132,217`；`collector.rs:69,94` | `HashMap` 迭代序进入序列化字节 → 同输入两次烘焙 `texture_id` 不同 | 序列化前 `BTreeMap`/排序 | ◻ |
| **K-05** | P1 | `libmtk/src/prebake/models.rs:73-90,112-121` | 模型缓存同时插短名并回退短名 → `mymod:block/stone` 解析到原版模型 | 只用全限定名 | ◻ |
| **K-06** | P1 | `mtk-resource/src/pack/source.rs:66-73` | `fs::read(..).ok()`：权限/IO 错误等同"文件不存在" → 静默用低优先级包的贴图 | 区分 `NotFound` 与其它错误 | ◻ |
| **K-07** | P1 | `mtk-resource/src/pack/stack/collector.rs:69` / `atlas/address_map` 序列化 | 精灵枚举顺序不稳定 → atlas 打包顺序不稳定 | 排序后枚举 | ◻ |
| **K-08** | P2 | `mtk-package/src/writer.rs:260,330-355` | `sync_all` 失败被吞；目录不 fsync → 崩溃后留下 0 字节包且被判有效 | 失败即报错 + 目录 fsync | ◻ |
| **K-09** | P2 | `libmtk/src/prebake/reader.rs:28-45` | 指纹字符串非 `mtk_fp_<hex>` 时截断 16 字节 → 前缀相同的不同指纹相等 | 严格格式校验 | ◻ |
| **K-10** | P2 | `mtk-package/src/toc.rs` / `error.rs:38` | TOC offset 不校验对齐/越界/重叠；`chunk_count` 不与 TOC 条目数比对；重复 chunk id 允许 | 加载时全量校验 | ◻ |
| **K-11** | P2 | `libmtk/src/scene/metadata.rs:20-56` | 清单字段全 `#[serde(default)]` → 缺字段被当 false/0 | 必需字段不给 default | ◻ |
| **K-12** | P2 | `libmtk/src/scene/package/validator.rs:35`、`exporter/source.rs:165-166,200` | 读 `biome/resolver` 旧格式分支，而写入方只写 `biome/mapping` → 死分支 | 删除旧读 | ◻ |

---

# B5 · 架构边界（规则违规，越拖越贵）— 约 1 天

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **A-01** ✅ DONE | P0 | `bindings/mtk-py/src/unstable/testing.rs:263,365` + `MoziToolKit/dev/ops/op_debug_item.py:234,347` | 生产 dev operator 依赖 `unstable.testing` 的 `model_database_get_mesh` / `build_items_gallery` | 并入 M-04 的生产面，删 testing 版本 | ✅ |
| **A-02** ✅ DONE | P1 | `unstable/testing.rs:283-338` | `model_database_get_transforms` **零消费者** | 删除（能力并入 M-04） | ✅ |
| **A-03** | P1 | `libmtk/src/prebake/engine.rs` + `mtk-voxel/src/storage/world/debug.rs:21,222-238` | 进程级全局 `static CACHED_DEBUG_STORAGE` + 公开 `clear_*`；core 内读 `MTK_TEST_ASSETS` | 缓存改为实例内/调用方传入；env 只在测试 crate | ◻ |
| **A-04** | P1 | `mtk-voxel/src/world/dynamic_entities.rs:130-265 vs 283-470` | skull/banner 扫描+解析两份实现（async/sync），5 个公开入口 | 收敛一份 + 一个入口 | ◻ |
| **A-05** | P1 | `mtk-save/src/loader.rs:243,256,272` | `load_box_into_storage` / `_with_progress` / `_with_entities` 三个近似入口，消费方只有测试 | 收敛成一个 + 显式配置 DTO | ◻ |
| **A-06** | P1 | `mtk-py/src/voxel/storage.rs:39-96` | `create_debug_world*` 5 个入口 | 收敛 | ◻ |
| **A-07** | P1 | `mtk-py/src/sync/client.rs:92-104` | `start` / `start_with_cache`（11 参数 + 哨兵默认值） | 一个入口 + 配置 DTO | ◻ |
| **A-08** | P2 | `mtk-py/src/scene/exporter.rs:20-46` | `**kwargs` 吞未知键（`max_atlas_sizes=1024` 被静默忽略） | 显式参数，未知键报错 | ◻ |
| **A-09** | P2 | `mtk-py/src/uv/fluid.rs:41-170` | 一个能力 3 个入口 + 4 段类型探测（u32/float64 被当 f32） | 一个入口 + 类型校验 | ◻ |
| **A-10** | P2 | `mtk-py/src/voxel/world.rs:159` | `get_world_mesh()` 未 meshed 时返回空 `MeshData` 而非错误 | 返回 `Option`/`Err` | ◻ |
| **A-11** | P2 | `bindings/mtk-py/src/buffer.rs:11` 等 | `#[allow(dead_code)]` 藏在绑定层（规则 8 禁止） | 删死代码 | ◻ |
| **A-12** | P2 | `mtk-voxel/src/fluid/test_verify_blender.rs`（330 行，`#[allow(dead_code)]` ×4） | 测试模块住在 `src/`，且 4 个字段从不读取（正是能抓住 V-01/V-02 的断言） | 移到 `tests/`，补上真实断言 | ◻ |
| **A-13** | P2 | `mtk-py/src/texture/atlas.rs:266-279`、`uv/extrude.rs:40-45` | 未知 `category`/`uv_mode` 静默回落默认值 | 报错 | ◻ |
| **A-14** | P2 | `mtk-save/src/error.rs:31` | `UnsupportedDataVersion` 从不构造；1.17 存档静默 `Ok(0 sections)` | 加载时校验 DataVersion | ◻ |
| **A-15** | P2 | `mtk-save/src/entity.rs:34-41,219,242-258` | 1.18+ 下限下仍在读 `Level.Entities`/`Motive`/`Dir`，且 `entity.rs:611` 测试锁定该违规 | 删旧布局 + 改测试 | ◻ |
| **A-16** | P2 | `MoziToolKit/utils/assets/cache_utils.py:1-24` | 明确的 "backwards compatibility" 再导出 shim | 删除，改调用点 | ◻ |
| **A-17** | P2 | `mtk-py/src/mesh/mod.rs:18-28` | `pub mod` + `pub` 字段 + `inner()/inner_mut()` 泄漏到 crate 外 | 收成 `pub(crate)` | ◻ |

---

# B6 · 测试体系（不修这个，上面全都会复发）— 约 0.5 天

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **T-01** | P0 | `mtk-testkit` 只认 `../mc` | 本机没有 `../mc` → 全部真实资产测试**静默跳过**，`cargo test` 仍全绿（AGENT.md 明令禁止） | 增加 `../Minecraft_Source` 自动探测；找不到资产时**失败**而非 return | ✅ |
| **T-02** | P0 | `MoziToolKit/tests/_assets.py:20` | import 时设 `MTK_TEST_SANDBOX=1`，让所有测试跳过指纹校验 → 缓存失效逻辑从未被测过 | 只在明确需要时开启；指纹路径单独跑一遍 | ◻ |
| **T-03** | P1 | `MoziToolKit/tests/test_item_frame_render_plan.py:259,474,557` | `glob(".ci-cache/*.mtkcache")[0]` 顺序不定 → 可能测的是旧包 | 明确选取（按指纹/env） | ◻ |
| **T-04** | P1 | 同上 `:370,466,626` | 断言 124/38/22 面数（实现细节），却**从不断言朝向/放置** | 改断言行为：正面法线方向、包围盒、FIXED 变换后的尺寸 | ✅ |
| **T-05** | P1 | `MoziToolKit/tests/test_item_frame_render_plan.py:30` | 原生模块缺失时 `skipTest` → 整个实体套件绿色空转 | 缺原生模块直接 fail | ◻ |
| **T-06** | P1 | 全仓 | 没有任何"损坏输入"测试（corrupt NBT / 截断 region / 解压炸弹 / 畸形 `.mtkscene`） | 每个 S-xx 项自带一个 | ◻ |
| **T-07** | P2 | `tests/test_api_surface_contract.py:26,50,71,124` | 只断言符号存在，不断言行为 | 补行为断言 | ◻ |
| **T-08** | P2 | `bindings/mtk-py/tests/`（3 个文件） | 1668 个物品模型、变换、gallery 都无覆盖 | 补 M-01/M-04 的测试 | ✅ |

---

# B7 · 消费端（Blender 插件 / 服务端）— 约 1 天

| ID | P | 位置 | 现象 | 修法 | 状态 |
|---|---|---|---|---|---|
| **H-01** | P0 | `MoziToolKit/utils/entities/reconciler.py:396-405` | 更新实体重注入网格，但 `ensure_world_materials` 只在 create 分支 → 换物品后材质槽错乱（灰面/错贴图） | 更新分支也重建材质 | ◻ |
| **H-02** | P0 | `reconciler.py:396-403` | `is_invisible` 不在 mesh diff → 隐转显仍是 0 面 | 加入 diff key | ◻ |
| **H-03** | P1 | `reconciler.py:407-436` | 自定义属性只在真值时写、从不清 → 取下物品后 `mtk:item_id` 残留 → 每次更新都重注入 | 无条件写（含空值/删除） | ◻ |
| **H-04** | P1 | `MoziToolKit/bridge/assets/readers.py:167` | biome resolver 缺失时静默用默认 66 色表 → 整世界统一 Plains 染色 | 报错或明确降级标记 | ◻ |
| **H-05** | P1 | `utils/assets/guard.py:275` | 未知缓存状态直接 `return True` | 白名单状态才放行 | ◻ |
| **H-06** | P1 | `utils/assets/guard.py:171` | 原生模块没加载报成 `"MISSING"` 缓存 → 触发无意义重烘并掩盖真实错误 | 单独状态 `NO_NATIVE` | ◻ |
| **H-07** | P1 | `bridge/assets/cache.py:389-410` | 模型库 memo key 用打开缓存**之前**的全局 → 换包后仍返回旧模型库配新图集 | 用打开后的路径/mtime | ◻ |
| **H-08** | P1 | `utils/materials/builder/world_binding.py:196-206` | 单 chunk 快路径把全部多边形塞进同一槽（忽略每面 chunk attr） | 用面属性 | ◻ |
| **H-09** | P1 | `utils/save/hierarchy.py:195-201` | 世界 bounds 由 Python min/max 重算（不用 `storage.bounds`）→ 实体整体偏移 | 用 storage bounds | ◻ |
| **H-10** | P1 | `bridge/sync/session.py:193`、`bridge/progress.py:55` | `except TypeError` 能力探测 → 掩盖真实 TypeError、重复消费事件 | 删探测 | ◻ |
| **H-11** | P2 | `utils/world/sync_container.py:245` vs `assembler.py:129` vs `injector.py:26` | 2–3 套世界装配流程 | 收敛一套 | ◻ |
| **H-12** | P2 | `utils/entities/reconciler.py:293-300` | key 冲突静默覆盖 → 被遮蔽对象永不被删/更新 | 冲突报错 | ◻ |
| **H-13** | P2 | `utils/scene/entities.py:130-137,158-176` | 解析失败静默丢实体；坐标回写失败保留旧值 | 报错/跳过并标记 | ◻ |
| **H-14** | P2 | 3 份 `safe_set_custom_prop`、3 份实体发现逻辑 | 重复实现，异常策略不同 | 收敛 | ◻ |
| **J-01** | P1 | `yefira/.../WebSocketServerManager.java:303-305 vs 456-497` | `trackedVisualEntities` 跨线程 clear+repopulate 无锁 → 重复 UPSERT + 伪 REMOVE | 加锁/换代 | ◻ |
| **J-02** | P1 | `WebSocketServerManager.java:471-474` | 变更检测漏 `scale`/`roll` | 补齐 | ◻ |
| **J-03** | P1 | `yefira/.../VisualEntityEntry.java:14-27,130-149` | 线上格式无 `custom_name`，scanner 也不读 `getCustomName()` | 与 Y-15 一起做 | ✅ |
| **J-04** | P2 | `versions/1.21.4|1.21.1|26.2 LevelCompat*.java` | 画 variant 只取 `getPath()`，丢命名空间 → 模组画布缺贴图 | 保留 namespace | ◻ |
| **J-05** | P2 | `versions/1.20.1|1.20.4 LevelCompat*.java` | 画尺寸 `/16` 已修，但 1.20.x **没有** item components 实现（默认 null） | 补 legacy `BlockEntityTag` 提取 | ✅（部分） |
| **J-06** | P2 | `yefira/.../compat/visual/VisualEntitySerializer.java:62` | `escape()` 已修（控制字符/代理） | — | ✅ |
| **J-07** | P2 | `yefira/.../encoder/VisualEntityEntry.java:39` | null UUID 静默替换为全零 → 两个实体互相删除 | 拒绝 null | ◻ |

---

# 已推翻的假阳性（不要做）

| 报告项 | 结论 |
|---|---|
| "`AttributeData::Bool` 位压缩 → memoryview 越界 8 倍" | **假**。Rust `Vec<bool>` 是 1 字节/元素 |
| "gallery `face_materials` 未偏移材质槽 → 物品贴图错位" | **假**。预烘模型 `face_materials` 存的是 atlas chunk id（`to_mesh.rs:92`），本来全局；按建议改会让 Blender 侧断言 `chunk 2` 变 `chunk 0` |

# 未审计区域（需要补一轮）

- `mtk-core`：`subdivide/*`、`clip`、`extrude` 的退化几何；`polygon.rs` 的包围盒/法线
- `mtk-cull`：`coplanar.rs:97` 未保护的 `normalize()`（NaN → 共面检测静默失效）
- `mtk-material`：PBR 解码表、`harcoded` 色表与原版一致性
- `mtk-texture`：图集装箱的重叠/留白/HD 缩放（C-06）

---

## 建议节奏

| 天 | 批次 | 产出 |
|---|---|---|
| D1 | B0 M-01 → M-05 | 46 个物品模型进缓存；`get_item` 不再猜；渲染计划能报"模型缺失" |
| D2 | B0 剩余（M-06..M-12） | 旋转语义、名字/染色/头颅解码、删死字段 |
| D3 | B1（S-01..S-10） | 损坏存档/包不再能打崩宿主 |
| D4 | B2（V-01..V-06, M-13） | AO/法线/dirty/biome 四类可见错误 |
| D5 | B3（Y-01..Y-08） | 联机不再无限 RESYNC / 丢编辑 / 幽灵实体 |
| D6 | B4 + B5 | 缓存指纹与版本校验；确定性；`unstable.testing` 生产依赖清零 |
| D7 | B6 + B7 | 测试不再空转；Blender 侧换物品/显隐/材质 |

---

## D1 完成记录（2026-11-16）

| 项 | 落地内容 | 验证 |
|---|---|---|
| M-01 | `bake_single_item` 增加三级几何来源：item 链 elements → `layer*` PNG 挤出 → **按方块 id 走内置/MiEx/方块模型几何**（保留 item 链的 `transforms`/`gui_light`）。真实原版资产实测：`chest`(3 元素) / `shulker_box`(2) / `decorated_pot`(3) / `conduit`(1) / `copper_chest`(3) 全部烘出，且 `gui=[30,45,0]`、`fixed=[0,180,0]` 正确；失败列表只剩 5 个真不支持的（`shield`、`shield_blocking`、`trident_in_hand`、`trident_throwing`、`amethyst_bud`） | `crates/libmtk/tests/test_item_model_bake.rs::real_vanilla_item_models_keep_display_transforms`（真实资产，3.7s） |
| M-01b | 物品模型不再插入 `<name>` 裸别名，避免覆盖同名方块条目（否则 `minecraft:chest` 会被物品几何+显示数据污染，方块网格错乱） | 同上：断言 `models["minecraft:chest"].transforms == default` |
| M-02 | `get_item` 对显式 `item/...` 请求只查 item 条目，绝不回落方块状态模糊匹配；裸方块 id 仍可解析代表变体 | 单测 + 断言 `get_item("minecraft:item/shield") is None` |
| M-03 | `BakedModelDatabase` 新增非序列化的 `bake_failures`（`ModelBakeFailure { key, reason }`）+ `push_bake_failure`/`bake_failures()`；预烘结束通过 progress 通道输出汇总行；Python 侧 `BakedModelDatabase.bake_failures` 可读 | 合成 pack 断言失败被记录；真实资产断言 `chest` 不在失败列表 |
| M-04 | 生产面新增 `BakedModelDatabase.get_mesh` / `get_item_transforms` / `build_items_gallery`；删除 `unstable.testing` 的同名函数与注册；`dev/ops/op_debug_item.py` 直接调用生产方式 | `clippy -D warnings` + Blender 套件 27 passed |
| M-05 | `EntityRenderConfig.model_lookup_available`（默认 false，Python 绑定按 `models is not None` 设置）+ `item_model_missing` 警告，展示框不再"静默没有物品" | `test_visual_entity_contract.rs`：有信号→告警，无数据库→不误报 |

剩余：B0 的 M-06～M-12 与 B1～B7 未动。
