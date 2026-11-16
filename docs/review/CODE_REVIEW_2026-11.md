# 代码审查报告 — libmozitoolkit / MoziToolKit / yefira

日期：2026-11-16
范围：`libmozitoolkit`（14 crate + 3 bindings + 2 adapters）、`MoziToolKit`（Blender 插件）、`yefira`（服务端模组）
基线：`fix/code-review` 分支，起点 `cceb3f7`（libmozitoolkit） / `544fab1`（MoziToolKit）

---

## 0. 验证基线（实测，不是推断）

| 门禁 | 结果 | 说明 |
|---|---|---|
| `cargo check --workspace --all-targets` | 通过 | 3.7s，无警告 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 静态分析零告警 |
| `cargo test --workspace --no-fail-fast` | 全部通过 | **但见下方"静默跳过"** |
| `cargo build-all-py` | 通过 | 18.5s，`dev/lib` 软链产物可用 |
| `blender --background --python tests/run_tests.py -- tests/test_item_frame_render_plan.py` | 10 passed | 含新增的陶罐碎片用例 |
| Rust 层实测 `build_entity_render_plan`（3 个 `.ci-cache`） | 正常 | 展示框+陶罐 = 38 quads，4 条碎片纹理路由齐全 |

**关键结论：门禁全绿，但功能仍在断裂。** 原因是测试体系有三处系统性失效：

1. **静默跳过**：`mtk-testkit` 只认 `../mc`，而该目录在本机不存在（真实资产在 `../Minecraft_Source`）。不设 `MTK_TEST_ASSETS` 时，所有真实资产测试直接 `return`，`cargo test` 依然显示全绿。AGENT.md 明确禁止"缺资产跳过状态下交付"，但默认命令就处于该状态。
2. **测试只测实现细节，不测契约**：`test_item_frame_render_plan.py` 断言 124/38/22 个面、断言 `glob(".ci-cache/*.mtkcache")[0]`，却**从未断言朝向**（注释说"floor frame 指向 +Z"，无断言）。实测 6 个 facing 的正面法线全部正确，但这是碰巧对的，没有任何测试守住它。
3. **测试专用入口被生产代码依赖**（见 §2.3），使得"删测试面"等于"删功能"，于是没人敢删。

---

## 1. 系统性根因（为什么"功能实现了但边界收不住"）

本次共确认 190+ 个缺陷，其中 12 个 critical。它们不是随机分布的，而是四种固定模式的重复：

**模式 A：一条能力有两套实现，只有一套被更新。**
序列化/解码/坐标转换/布局/材质绑定，几乎每组都有 2–3 份拷贝，其中一份是"正确的那份"，另一份是"跑在真实路径上的那份"。ERP 的物品组件问题就是典型：Rust 存档侧加了 `ItemComponentsData`，Java 实时同步侧完全没加。

**模式 B：失败被降级成"看起来正常的空数据"。**
`unwrap_or_default` / `.ok()` / `except Exception: pass` / `unwrap_or_else(|_| Generic)`。全仓统计见 §4。后果是错误不可观测：陈旧的模型库、错误的手指纹理、被吞掉的实体，用户只能看到"图不对"。

**模式 C：生产逻辑被塞进 `unstable.testing`。**
因为规则要求"测试入口不能污染稳定面"，实现者就把**功能**放进了测试命名空间，然后让生产代码去调它 —— 规则的字面被遵守了，边界被焊死了。

**模式 D：序列标识符没有单一事实源。**
UUID 三处格式化（其中一处无连字符）、实体 key 依赖 UUID 字符串、缓存指纹只看包名不看内容。任何一处不一致都会导致对象重复/丢失/缓存错配。

---

## 2. 专项：物品展示框为什么"做不下去"

这��用户当前的直接阻塞点，单独拆开。

### 2.1 真实状态：Rust 侧是对的，Java 侧是断的

- 存档路径（`mtk-save`）→ `ItemComponentsData` → `BuiltinModelRegistry::bake_block_entity` → 陶罐/旗帜/头颅模型融合：**实测可用**（38 quads，4 条碎片图案路由）。
- 实时同步路径（yefira → JNI → mtk-sync）：**物品 NBT 全部丢失**。
  - `yefira/common-core/.../encoder/VisualEntityScanner.java:145-152`：只取 `itemId`/`count`/`rotation`，`mapId` 硬编码 `null`，`components` 从未序列化。
  - `yefira/common-core/.../compat/visual/VisualEntitySerializer.java:25-38`：`serializeItemFrame` 无 `components` 字段。
  - 结果：`EntityBaker` 的 `f.components` 永远是 `None` → 展示框里的陶罐退化成平面物品贴图。

### 2.2 同时存在的、会让它"修好也还是错"的四个缺陷

| # | 位置 | 缺陷 | 后果 |
|---|---|---|---|
| 1 | `bindings/mtk-jni/src/server.rs:344` | UUID 用 `format!("{:016x}{:016x}")`（无连字符），而 `protocol/hasher.rs:158 format_uuid` 是 8-4-4-4-12 | 同一实体在 JNI 提交时 key=A，回环解码后 key=B；重连后 Blender 里出现重复物体，旧的永远删不掉 |
| 2 | `mtk-sync/src/protocol/hasher.rs:146-155` | `parse_uuid_bits` 非法输入返回 `(0,0)`；`&clean[0..16]` 可按字节切断 UTF-8 | 多个坏 UUID 塌缩成 nil UUID 互相覆盖；构造性输入可在服务端线程 panic |
| 3 | `VisualEntityScanner.java:121-123` | 直接输出 `getXRot()/getYRot()` 弧度；MC 的 yaw 0 指南，Rust 侧要求 yaw 0 指北 | 非 painting/item_frame 实体（如盔甲架）朝向整体反 180°，pitch 镜像 |
| 4 | `VisualEntityScanner.java:154` | `serializeArmorStand` 存在但零调用；盔甲架走 `serializeGeneric` | 盔甲架永远以"未知实体"流过去，`show_arms/no_base_plate/is_small` 丢失 |

### 2.3 "模型序列化修了一半"的确切含义

commit `e1e3ae3` 把 `ItemTransforms` / `GuiLight` 烘进了 `BakedModel`，commit `2b259f3` 把模型库接进了渲染计划。但：

- `PyBakedModelDatabase`（`bindings/mtk-py/src/model.rs`）对外只有 `__len__` / `is_empty`。**模型库在 Python 侧是不可查询的。**
- 唯一能拿模型/贴图/变换的入口是 `unstable/testing.rs`：`model_database_get_mesh`（:263）、`model_database_get_transforms`（:283）、`model_database_build_items_gallery`（:365）。
- 生产代码确实在调它们：`MoziToolKit/dev/ops/op_debug_item.py:234,347`。
- 于是：**物品展示框需要"拿模型 + 拿 FIXED 变换"的能力，这个能力只存在于测试命名空间里。** 这就是"修了一半"。

另外 `model_database_get_transforms` **零消费者**（全仓 grep 无调用方），属于规则 9 的应删项。

### 2.4 还发现：`ItemComponentsData` 里三个字段是死的

`dyed_color` / `custom_name` / `has_glint` 被解码（`mtk-save/src/block_entity.rs:419-428` 等）但**没有任何消费者**。真正被使用的只有 `block_entity`。按 AGENT.md 规则 1/9（零消费者即删除），它们要么接上消费者，要么删掉。

### 2.5 解码本身的三个实错

| 位置 | 缺陷 |
|---|---|
| `block_entity.rs:419-428` | `custom_name` 只读 `String` 标签。1.20.5+ 的 Text Component 是 JSON 字符串（`"{\"text\":\"Hero\"}"`）或 Compound；Compound 形态直接丢，JSON 形态原样返回带引号/带 `{"text":...}` 的脏值 |
| `block_entity.rs:93-100` | 从不读 1.20.5+ 的 `minecraft:base_color` 组件，改用物品 id 前缀猜颜色 → `light_blue_banner` 得到 `"light"`，`shield` 永远是白色 |
| `block_entity.rs:344,372` | `clean_id.contains("pot")` / `contains("banner")` 启发式：`minecraft:flower_pot` 会被当成装饰陶罐烘出 4 面空罐 |

---

## 3. 各 crate 发现（按严重度排序，节选）

### 3.1 `mtk-save`（critical/high）

1. `src/nbt/reader.rs:173-263` — 12 处 `Vec::with_capacity(len)` 直接用攻击者可控的 i32 长度。`len=0x7FFFFFFF` 的 list → ~51GB 预分配 → 进程 abort（不是 `Err`）。**无测试。**
2. `src/region.rs:129-130` — `vec![0u8; (chunk_len-1)]` 信任文件内 4 字节长度，`sector_count` 解析了但从不用于限界；deflate 炸弹无上限。
3. `src/chunk.rs:98-110` — `palette.len()>1` 但 `data` 缺失时，整段填 `palette[0]` 并谎报 `non_air_count = 4096`；`:110` 是死代码；**测试把这个错误行为固化了**（`chunk.rs:586-591`）。
4. `src/chunk.rs:218-219` — 缺 `xPos`/`zPos` 时默认 0 且跳过边界校验 → 整区块几何被静默挪到 (0,0)。
5. `src/loader.rs:360` — 一个坏 chunk 让整次加载 `Err`；同一循环里其它错误却被完全吞掉。
6. `src/block_entity.rs:179-196` — 无 profile 时 `SkullKind` 默认 Skeleton，`zombie_head`/`creeper_head`/`dragon_head` 全渲染成骷髅。
7. `src/block_entity.rs:14-25` — 告示牌从不解码，而 `BlockEntityData::Sign` 在生产路径上没有任何生产者。
8. `src/error.rs:31` — `UnsupportedDataVersion` 从未构造；1.17 存档静默 `Ok(0 sections)`。
9. `src/entity.rs:34-41,219,242-258` — 1.18+ 下限下仍在读 `Level.Entities`/`Motive`/`Dir` 等旧布局，且 `entity.rs:611` 有测试锁定该违规路径。
10. `src/nbt/tag.rs:280-282` — `NbtList::compounds()` 深度克隆整棵子树，在 chunk 热路径上每 chunk 复制 ~32KB/section。

### 3.2 `mtk-sync`（critical/high）

1. `client/session/handlers_delta.rs:88` — `PALETTE_UPDATE` 不推进 `state.sequence`，而服务端为每个 palette 包分配序号 → 每次"引入新方块状态"的编辑都会触发 `SequenceGap` → 无限 RESYNC。`:102` 的 `DELTA_ENTITY` 同病。
2. `client/session/assembler/mod.rs:214` — `with_capacity(decoded_target_bytes)`，该值来自 `u32` 线上字段；`zstd` 解压无上限 → 一次 baseline chunk 可 4GiB abort。
3. `client/session/assembler/mod.rs:190,233` — `total_chunks` 与累计 voxel 数无上界校验 → 无界内存增长。
4. `server/service/server.rs:250` — `Instant::now().elapsed().as_nanos()` ≈ 0，`snapshot_uuid` 实际恒定 → 旧 baseline 的 chunk 会被新 baseline 接受，混合快照。
5. `client/session/handlers_delta.rs:149` — `rel_pos` 无范围校验，`i32::MAX` 溢出（debug panic / release 回绕到错误区块）。
6. `client/session/handlers_delta.rs:40` — baseline 期间只有 `DeltaOp` 进 staging；`PaletteUpdate`/`DeltaEntity`/`DeltaVisualEntity` 立即应用后被 `sync_from_storage` 抹掉且不重放 → 大 baseline 期间的编辑静默丢失。
7. `client/session/handlers_baseline.rs:216` — baseline 提交只发 upsert，不发删除 → 离线期间服务端删掉的实体在客户端永久残留。
8. `server/service/server.rs:157,170,228,239,306,407,433` + `state.rs:100` — `.lock().unwrap()`；任一持锁 panic 后整个握手线程永久 panic。
9. `protocol/codec/decode.rs:144` — 实体 JSON 解析失败 → `None` = "此处无方块实体" → **静默删除**。
10. `protocol/codec/encode.rs:9` — `write_str` 写 `len as u16` 但追加全部字节，>65535 时长度前缀与内容不符 → 整帧错位。

### 3.3 `mtk-voxel`（high）

1. `mesher/ao.rs:156,167` — `shade_corner12/13` 返回 `shade0`（应为 `shade1`）→ 凹角两角漏光，亮度 0.8 而非 0.4。**无测试覆盖非 1.0 亮度。**
2. `mesher/grid_pool.rs:61-64` — 顶点去重只按位置、忽略法线 → 焊接后 1/3 面法线正确，其余是错的轴。`:305` 的测试只断言顶点数=8。
3. `world/sync.rs:86` — `dirty_sections = dirty` 覆盖而非合并 → 同步前标记的脏段被丢弃（网格永久陈旧）。
4. `storage/world/container.rs:283-284` — 只改 biome 时不动 dirty（`if changed` 只看方块状态）→ 染色永不刷新。
5. `container.rs:296-310` — 只标 6 个正交邻居，但 padded apron 采样 8 个对角邻居 → 对角角落 AO/流体高度陈旧。
6. `storage/section.rs:206` — `&self.palette[old_pal_idx as usize]` 无界（`Deserialize` 无校验）→ 恶意 `.mtkscene` 触发 panic；同一数据在 `get_local_state` 里却静默返回 air。
7. `storage/sparse.rs:262-265` — `columns.y[i]` 按 `columns.x.len()` 索引，serde 未校验长度一致 → 格式错误即 panic。
8. `mesher/shading/resolver.rs:398` — `(cy >> 2) & 3` 在 `cy=16` 回绕到 0 → 3D biome 最上角采样到最底层。

### 3.4 `mtk-py` / `mtk-jni` / `mtk-wasm`（critical）

> 更正：初审曾报告 `AttributeData::Bool(Vec<bool>)` 为位压缩、导出 memoryview 会越界 8 倍。**经本人复核该结论错误** —— Rust 的 `Vec<bool>` 是 1 字节/元素（位压缩的是 `bitvec` 这类库）。`attributes.rs:77-82` 的导出长度正确，无越界。真正的缺陷是下面第 2、3 条。
2. `buffer.rs:136-146` — `PyBufferOwner` 钉住 Python 对象而非 `Vec` 缓冲区；`HashMap` 插入/`clear()` 后旧 memoryview 读已释放内存。（已复核：owner 只持有 `Py<PyAny>`，跨语言别名无重分配防护，成立。）
3. `mesh/attributes.rs:107-122` — `PyBUF_SIMPLE` 不校验 `format`/`itemsize`，`nil` buffer 走 `from_raw_parts(null, 0)`（UB）；传 float64 会被当 f32 静默重解释。
4. `mtk-jni/src/server.rs` 全部句柄只校验 `!= 0`；`yefira/.../WebSocketServerManager.java:199` 在 poller 线程读 `serverHandle`，`:186` 在主线程释放 → 关闭时 UAF。
5. `mtk-wasm/src/lib.rs:128-172` — 裸指针交给 JS，`memory.grow`/`append_mesh` 后悬空。
6. ~~`unstable/testing.rs:447` — gallery 里 `face_materials.extend(...)` 不偏移材质槽~~ **假阳性，已推翻。** `to_mesh.rs:92` 的 `mat_slot = face.atlas_chunk_id.unwrap_or(slot)` 说明预烘模型（已重映射）的 `face_materials` 存的是**图集 chunk id**，本身就是全局的，无需偏移；按建议偏移反而会把 chunk id 当作槽位映射，Blender 侧测试 `test_item_generator_bridge.py` 立即回落为 chunk 0。全局去重的 `all_textures` 只用于材质命名。此条不动。
7. `attribute_memoryview` 等 8 个 `unstable.testing` 导出未纳入 `test_api_surface_contract.py` 的禁用清单。

### 3.5 `mtk-resource` / `libmtk` / `mtk-package`（critical）

1. `mtk-resource/src/pack/stack/mod.rs:197-213` — 栈指纹只哈希**包名**，不哈希内容 → 原地修改的包指纹不变，缓存被判为 READY。
2. `libmtk/src/prebake/reader.rs:79-90` — 缓存有效性只看 64 字节 header，`CacheManifest.format_version` 从不校验 → v4/v6 旧缓存被当作 v7 解析。
3. `libmtk/src/prebake/resolver.rs:63-77` — 不传指纹时直接返回目录里"最新"的 `.mtkcache`，零校验。
4. 确定性：`HashMap` 迭代序泄漏进序列化字节（`atlas/mapping`、`models/database`、zip 枚举顺序）→ 同输入两次烘焙产生不同 `texture_id`。
5. `mtk-package/src/reader.rs:186-200` — `zstd::decode_all` 无输出上限；TOC 的 `decompressed_size` 不校验 → 解压炸弹。
6. `libmtk/src/prebake/reader.rs:236-243` — 用包内 chunk id 直接 `out_dir.join(...)`，可写出输出目录（`../../../.zshrc`）。
7. `mtk-package/src/header.rs:206-212` — 只比较 `version_major`，`version_minor` 校验缺失。
8. `libmtk/src/prebake/models.rs:73-90,112-121` — 模型缓存同时插入短名 `block/stone` 并回退短名查找 → `mymod:block/stone` 解析到原版模型。
9. `mtk-resource/src/meta.rs:8-28` — `.mcmeta` 的 `frames` 列表解析了但从不使用，`frametime` 丢失 → 动画帧序/时长错误。
10. `libmtk/src/prebake/engine.rs:204-206` — 请求 `blocks` 分类会烘全部 13 个标准图集（无单类选择）。

### 3.6 `MoziToolKit`（high）

1. `utils/entities/reconciler.py:396-405` — 更新实体时重注入网格，但 `ensure_world_materials` 只在 create 分支调用 → 换物品后材质槽错乱（灰面/错贴图）。
2. `reconciler.py:396-403` — `is_invisible` 不在 mesh diff 里 → 展示框由隐转显仍是 0 面网格。
3. `reconciler.py:407-436` — 自定义属性只在真值时写入、从不清除 → 取下物品后 `mtk:item_id` 残留，此后每次更新都重注入网格。
4. `bridge/assets/readers.py:167` — biome resolver 缺失时静默返回默认 66 色表 → 整世界统一 Plains 染色。
5. `utils/assets/guard.py:275` — 未知状态直接 `return True` → 缓存坏了也放行。
6. `utils/assets/guard.py:171` — 原生模块没加载被报成 `"MISSING"` 缓存 → 触发无意义重烘并掩盖真实错误。
7. `bridge/assets/cache.py:389-410` — 模型库 memo key 用的是打开缓存**之前**的全局路径/mtime → 缓存换包后仍返回旧模型库配新图集。
8. `utils/materials/builder/world_binding.py:196-206` — 单 chunk 快路径把全部多边形塞进同一材质槽。
9. `utils/save/hierarchy.py:195-201` — 世界 bounds 由 Python 端 min/max 重算（不用 `storage.bounds`）→ 所有实体整体偏移。
10. 三份 `safe_set_custom_prop`、三份实体发现逻辑、两套世界装配流程。

### 3.7 `yefira`（high）

1. `VisualEntityScanner.java:121-123` 旋转语义错（见 §2.2）。
2. `VisualEntityScanner.java:154` 盔甲架走 generic。
3. `VisualEntitySerializer.java:25-38` 无 `components`。
4. `VisualEntityEntry.java:14-27` 线上格式没有 `custom_name`。
5. `versions/1.20.1/.../LevelCompat120.java:166-167`（1.20.4 同）画尺寸 `getWidth()/16` → 全部 1×1。
6. `WebSocketServerManager.java:303-305 vs 456-497` — `trackedVisualEntities` 跨线程 clear+repopulate 无锁 → 重复 UPSERT + 伪 REMOVE。
7. `WebSocketServerManager.java:471-474` — 变更检测漏 `scale`/`roll`。
8. `VisualEntitySerializer.java:62-65` — `escape()` 不处理控制字符/孤立代理 → 非法 JSON → 被 §3.2-9 静默降级成 generic。
9. 唯一的序列化测试 `VisualEntitySerializationTest.java:33` 用的 `{"variant":"minecraft:kebab"}` 根本不是合法的 `EntityKindData`，所以以上全部通过 CI。

---

### 3.8 `mtk-core` / `mtk-cull` / `mtk-material` / `mtk-texture`（覆盖不完整）

**这一组审计因额度耗尽失败，未完成。** 以下仅为本人在别处顺带复核到的点，不能当作完整结论：

1. `mtk-core/src/geometry.rs:318-333` + `mtk-model/src/entity/render.rs:132` — 实体旋转用 `EulerRot::XYZ`（= Rx·Ry·Rz）构造，而 Minecraft 实体朝向是 Ry(yaw)·Rx(pitch)·Rz(roll)。pitch 与 yaw 同时非零时姿态错误（举��：pitch=30°/yaw=45° 时正面法线 (−0.707,0.354,−0.612) vs 正确 (−0.612,0.5,−0.612)）。当前 6 个展示框 facing 实测正确（属巧合），painting 只有 yaw、盔甲架尚未烘焙，所以暂未外显 —— 属**潜伏**缺陷。`render.rs:826` 的 round-trip 测试用 `rotation = ZERO`，无法发现。
2. `mtk-cull/src/geometry/coplanar.rs:97` — `norm_a.cross(u_axis).normalize()` 无零向量保护 → NaN 参与比较恒 false，共面检测静默失效。
3. `mtk-voxel/src/biome/color.rs` 的 5 个调色板取值点全部委托 `mtk_material`，规则 5 的 SSOT 未被破坏（这一条是**通过**）。
4. 未审计：`mtk-texture` 图集装箱的重叠/边缘留白、`mtk-core` 的 subdivide/clip 退化几何、`mtk-material` PBR 解码表。这部分需要重新跑一遍审查。

---

## 4. 静默失败清单（模式 B 的量化）

`libmozitoolkit`（非测试代码）：

- `unwrap_or_default()` / `unwrap_or(...)`：`mtk-py` 21 处、`mtk-jni` 9 处、`libmtk` 17 处、`mtk-save` 12 处
- `.ok()` / `if let Ok(..)` 丢弃错误：`libmtk/src/prebake/models.rs` 6 处、`mtk-resource` 5 处
- `except Exception` 家族（MoziToolKit）：`point_cloud/extract.py` 约 20 处、`dev/loader.py`、`bridge/sync/session.py`

这些不是"防御性编程"，而是把可诊断故障变成不可诊断的视觉错误。

---

## 5. 修复工作流（10 条，文件域互不重叠）

每条都必须：只改自己列出的文件；`cargo fmt`；`cargo clippy -p <crate> --all-targets -- -D warnings`；补测试；不引入兼容层/旧格式双读；不新增 `#[allow(dead_code)]`。

| ID | 范围 | 关键交付 |
|---|---|---|
| **WS-1** | `mtk-save/src/nbt/*`, `region.rs` | 所有外部长度先做上界校验再分配；解压输出上限；越界/溢出改 `Err`；补 3 个恶意输入测试 |
| **WS-2** | `mtk-save/src/chunk.rs`, `loader.rs`, `entity.rs`, `block_entity.rs` | 缺 `xPos/zPos` 报错；缺 `data` 不再伪造 4096 方块；删旧布局双读；`custom_name` 解 Text Component；读 `minecraft:base_color`；删 `contains("pot")` 启发式；`SkullKind` 按 id 判定；`Sign` 接上生产者或删除变体 |
| **WS-3** | `mtk-sync/src/protocol/**`, `hasher.rs` | UUID 解析/格式化唯一实现（非法即 `Err`，禁止 `(0,0)`）；`write_str` 长度上界；payload JSON 失败改为协议错误；删零消费者导出 |
| **WS-4** | `mtk-sync/src/client/session/**`, `server/service/{server,baseline}.rs` | palette/entity 包推进 sequence；`decoded_target_bytes`/`total_chunks` 上界；`snapshot_uuid` 真实唯一；`rel_pos` 校验；baseline 期间全类型 staging + 重放；baseline 删除事件；去掉 `.lock().unwrap()` |
| **WS-5** | `mtk-voxel/src/mesher/**`, `storage/{section,sparse}.rs`, `world/sync.rs`, `container.rs` | AO corner12/13、焊接法线、dirty 合并/biome 脏标记/对角邻居；palette 越界返回 `Err`；`(cy>>2)&3` 回绕；`test_verify_blender.rs` 移出 `src` |
| **WS-6** | `bindings/mtk-py/src/{buffer.rs,mesh/**}` | `PyBufferOwner` 改为钉住缓冲区本身（防重分配后悬空）；buffer format/itemsize/ndim 校验；null buffer 守卫；删 `#[allow(dead_code)]`。~~bool 位压缩越界~~（假阳性，已推翻） |
| **WS-7** | `bindings/mtk-py/src/{model.rs,unstable/testing.rs}` + `MoziToolKit/dev/ops/op_debug_item.py` | 模型库生产面：`get_mesh` / `build_items_gallery` 上移；删 `model_database_get_transforms`；同步 dev 调用点。~~材质槽偏移~~（假阳性，已推翻） |
| **WS-8** | `mtk-model/src/entity/{baker.rs,instance.rs,render.rs,item_frame.rs}` | 删除 `dyed_color`/`custom_name`/`has_glint` 死字段；实体旋转改用 `YXZ` 语义并分解为宿主 XYZ；补 3 轴旋转测试 |
| **WS-9** | `yefira/common-core/**`, `yefira/versions/**` | `components` 序列化（陶罐/旗帜/头颅）；`map_id`；盔甲架；`custom_name`；旋转规范化；JSON 转义；1.20.x 画尺寸；并发保护 |
| **WS-10** | `MoziToolKit/utils/**`, `bridge/**`, `tests/**` | 更新实体补材质绑定；`is_invisible` 进 diff；属性清除；缓存 memo key；guard 状态机；测试去掉 glob[0] 与 skip |

依赖顺序：WS-3 → WS-9（线上契约）；WS-7 → WS-10（API 迁移）；其余可并行。

### 审查可信度说明

- 我本人直接复核并复现过的：ERP 全链路（存档→烘焙→渲染计划→Blender 测试）、JNI UUID 无连字符、`parse_uuid_bits` 宽松解析、gallery 材质槽未偏移（**后经测试推翻：预烘模型的 `face_materials` 是 atlas chunk id，不是槽位**）、`ItemComponentsData` 三个死字段、`custom_name` Text Component、`contains("pot")` 误判、6 个 facing 法线实测正确、三个 `.ci-cache` 端到端 38 quads、`Vec<bool>` 非位压缩（推翻一条 critical）。
- 其余条目来自 7 份并行审计报告，行号是审查时快照，**动手修之前必须逐条复核**；上一轮已经出现过一条假阳性，说明这一步不能省。
- `mtk-core/cull/material/texture` 一组未完成（额度耗尽）。

验收：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
`MTK_TEST_ASSETS=../Minecraft_Source cargo test --workspace`、`cargo build-all-py`、
`blender --background --python tests/run_tests.py -- -rs -q`（要求 0 skipped）。

---

## 6. 本轮已落地的修复

| 提交范围 | 内容 | 验证 |
|---|---|---|
| `bindings/mtk-jni/src/server.rs` | upsert/remove 两处 UUID 改用 `mtk_sync::protocol::format_uuid`（8-4-4-4-12 带连字符）。此前 JNI 提交的 key 无连字符、回环解码后带连字符，同一实体会以两个 key 存在 / 删不掉 | `cargo clippy -p mtk-jni -D warnings` |
| `crates/mtk-sync/src/protocol/{hasher,error}.rs` | 新增严格 `parse_canonical_uuid`（8-4-4-4-12 或 32 hex，其余 `Err`；按字节校验，不再 `String` 切片，杜绝 UTF-8 切断 panic）；新增 `InvalidUuid`/`StringTooLong`/`InvalidPayload` 错误变体 | `cargo test -p mtk-sync` |
| `crates/mtk-model/tests/test_visual_entity_contract.rs` | **新增线上契约回归测试**：把 yefira 生成的展示框 JSON（陶罐碎片 / 旗帜 + map_id / 无组件）喂给 Rust 反序列化并烘焙，断言碎片纹理路由与 undecorated 面回退正确 | 3 passed |
| `yefira` `VisualEntitySerializer` | `serializeItemFrame` 新增 `map_id` + `components.block_entity`；JSON 转义覆盖控制字符与孤立代理；`serializeArmorStand` 接入 | `javac` 独立编译通过 |
| `yefira` `VisualEntityScanner` | 提取并传递 `components` / `map_id`；旋转改为 `pitch=-xRot`、`yaw=wrap(π-yRot)`；盔甲架不再退化为 generic | 需 gradle 编译（本机 JDK 不匹配，未跑） |
| `yefira` `ILevelCompat` + `LevelCompat26` | 新增 `serializeItemBlockEntityJson` / `getItemMapId` 两个版本兼容钩子，26.2 用 `DataComponents` 实现（陶罐/旗帜/头颅/地图）；其他版本默认返回 null（不破坏旧版编译） | 需 gradle 编译 |
| `yefira` `LevelCompat120/1204` | 画尺寸 `getWidth()/16` 无条件除法 → 改为与新版本一致的防御式归一化，修掉"所有画都是 1×1" | 需 gradle 编译 |
| `docs/review/` | 本报告 | — |

**未完成且必须回补的**：WS-1/2/4/5/6/8/10 全部；WS-3 的 `write_str` 上界与 JSON 失败改为协议错误；WS-7 的模型库生产面迁移；`ItemComponentsData` 三个死字段的删除。

**测试结论修正**：初次审查中"gallery 材质槽偏移"与"`Vec<bool>` 位压缩越界"两条为假阳性，已在 §3.4 与 §5 标注。这说明并行审计的结论必须先复现再动手——本轮 8 份报告里已出现 2 条假阳性。
