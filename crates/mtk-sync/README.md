# mtk-sync

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/gpl-3.0.html)

**`mtk-sync`** 是 `libmozitoolkit` (`libmtk`) 套件中的实时网络协同与二进制增量流客户端 Crate。遵循 **无宿主依赖 (Host-Agnostic)** 原则，专为建立 Minecraft（伴随模组如 [Yefira](https://github.com/mozi1924/yefira)）与 3D DCC 宿主（如 Blender [MoziToolKit](https://github.com/mozi1924/MoziToolKit) 插件、WebGPU 视口或独立渲染器）之间的高性能双向二进制实时通信通道而设计。

核心遵循 **RFC v3.1: MTK 2.0 Live Sync 协议与架构设计规范**，将选区状态机、两阶段流式传输、全量基线解算、xxHash64 空间状态指纹自愈、增量网格合并与单一世界大网格完全收敛在后台。

---

## 1. 架构拓扑与核心职责

```
┌────────────────────────────────┐
│   Minecraft (Yefira 模组)       │
│  Selection / Chunks / Deltas    │
└────────────────┬───────────────┘
                 │ WebSocket (Little-Endian Binary, RFC v3.1)
                 ▼
┌────────────────────────────────────────────────────────────────────────┐
│                               mtk-sync                                 │
│  ┌────────────────────────┐  ┌──────────────────────────────────────┐  │
│  │   SyncClient (守护线程)  │  │        decode_packet (协议编解码)    │  │
│  │ (tungstenite / 重连机制)│  │ (BaselineBegin/Chunk/DeltaOp/...)    │  │
│  └───────────┬────────────┘  └──────────────────┬───────────────────┘  │
│              │ ClientMessage                    │ Packet               │
│              └─────────────────► ┌──────────────▼─────────────┐        │
│                                  │   LiveSyncSession / 调度器 │        │
│                                  │ (VoxelWorld / Rayon Mesher) │        │
│                                  └──────────────┬─────────────┘        │
└─────────────────────────────────────────────────┼──────────────────────┘
                                                  │ SyncEvent (通道分发)
                                                  ▼
                                ┌───────────────────────────────────┐
                                │     DCC 宿主前端 (如 Blender)      │
                                │   poll_events() / 网格极速灌入     │
                                └───────────────────────────────────┘
```

- **二进制协议极速解析**：全量采用 Little-Endian（小端序）POD 紧凑内存布局，避免低效的 JSON/文本协议开销。
- **两阶段无缝基线流**：支持两阶段渐进式传输协议（`BaselineBegin` $\to$ `BaselineChunk` 增量摄取 $\to$ `BaselineEnd` 全局协同并行网格化），彻底消除跨区块面剔除错误与共面裂隙。
- **非阻塞事件轮询**：宿主仅需在每帧调用 `session.poll_events()` 或 `session.poll_events_coalesced()` 提取构建完毕的 `MeshData` 或状态变动，不阻塞 UI 渲染主循环。
- **单一规范入口**：所有内部子模块均已私有化收敛，所有对外类型与函数统一经由根命名空间 `mtk_sync::*` 暴露。

---

## 2. 根导出规范 API (Canonical Root Surface)

| 类别 | 规范符号 | 职责与功能 |
| :--- | :--- | :--- |
| **会话控制** | `LiveSyncSession` | 高阶实时会话控制器，管理 WebSocket 客户端、`VoxelWorld` 镜像与多线程网格化。 |
| **会话状态** | `SyncLifecycle` | 会话生命周期状态枚举（`Connecting`, `ReceivingBaseline`, `ValidatingHash`, `Live`, 等）。 |
| **事件流** | `SyncEvent` | 宿主订阅的高阶事件枚举（`WorldMeshReady`, `StreamProgress`, `DeltaApplied`, `SelectionUpdated`, 等）。 |
| **进度归一** | `normalize_stream_progress` | 权威两阶段进度归一化算子（将 download 与 meshing 阶段平滑映射至单调 0..100%）。 |
| **协议模型** | `Packet`, `DeltaChangeEntry` | 强类型 RFC v3.1 二进制通信数据包枚举与增量修改条目。 |
| **协议编解码** | `decode_packet`, `encode_packet` | 零拷贝二进制数据包解码与小端序封包。 |
| **哈希校验 SSOT**| `compute_sparse_state_hash`, `compute_sparse_state_hash_for_storage`, `compute_sparse_state_hash_for_point_cloud`, `compute_delta_batch_hash`, `compute_canonical_state_hash` | 基于 xxHash64 的权威 3D 稀疏世界指纹计算，保障 0ms 重连与一致性自愈。 |
| **协议常量与枚举**| `Opcode`, `SyncDecision`, `BaselineAckResult`, `CancelReason`, `ResyncReason`, `ErrorCode`, `ProtocolError`, `PROTOCOL_MAGIC`, `PROTOCOL_VERSION` | RFC v3.1 协议规范常量、指令码与错误枚举。 |

---

## 3. 二进制网络协议规范 (RFC v3.1)

### 3.1 数据包头部与版本协商

所有通信数据包均以 4 字节固定头部起始：

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       Magic: 'M' (0x4D)       |       Magic: 'T' (0x54)       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|     Version: 0x03 (Protocol)  |     Opcode (0x01..0x12)       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

- **`PROTOCOL_MAGIC`**: `[0x4D, 0x54]` (ASCII `"MT"`)。
- **`PROTOCOL_VERSION`**: `0x03`（对齐 RFC v3.1 规范）。

### 3.2 强类型数据包与操作码 (`Opcode`)

| Opcode | 标识符 | 方向 | 职责描述 |
| :--- | :--- | :--- | :--- |
| `0x01` | `Hello` | C $\to$ S | 客户端握手发起，携带版本范围、Capabilities、Auth Token 与缓存状态指纹。 |
| `0x02` | `Welcome` | S $\to$ C | 服务端握手响应，包含协商决策 (`SyncDecision`)、世界 UUID、Epoch 与选区信息。 |
| `0x03` | `BaselineBegin` | S $\to$ C | 全量基线开始，包含包围盒、调色板、切片总量与压缩格式。 |
| `0x04` | `BaselineChunk` | S $\to$ C | 基线体素切片数据流（含 xxHash64 校验码）。 |
| `0x05` | `BaselineEnd` | S $\to$ C | 基线流结束通知，携带最终 `final_state_hash` 用于自愈校验。 |
| `0x06` | `BaselineAck` | C $\to$ S | 客户端提交确认，回传校验状态 (`BaselineAckResult`)。 |
| `0x07` | `DeltaOp` | S $\to$ C | 增量方块改动批量推送 (`DeltaChangeEntry`)。 |
| `0x08` | `PaletteUpdate` | S $\to$ C | 动态追加方块状态调色板。 |
| `0x0B` | `Cancel` | 双向 | 会话或流式任务取消 (`CancelReason`)。 |
| `0x0D` | `ResyncRequest` | C $\to$ S | 客户端发现哈希不匹配或序列空洞时发起重同步 (`ResyncReason`)。 |
| `0x0E` | `Ping` | 双向 | 链路保活探测。 |
| `0x0F` | `Pong` | 双向 | 链路保活响应。 |
| `0x10` | `Error` | 双向 | 错误诊断响应 (`ErrorCode`)。 |
| `0x11` | `BaselineEntities`| S $\to$ C | RFC v3.2 稀疏方块实体基线流。 |
| `0x12` | `DeltaEntity` | S $\to$ C | RFC v3.2 稀疏方块实体增量修改。 |

---

## 4. 核心协同机制与数据流

### 4.1 实时会话控制器 (`LiveSyncSession`)

`LiveSyncSession` 是面向宿主的高层 API 门面：

```rust
pub struct LiveSyncSession {
    pub world: Arc<RwLock<VoxelWorld>>, // 会话唯一的体素世界与网格缓存
    // ... 内部客户端与线程句柄已私有化收敛
}
```

#### 关键方法
- `new(config, culler, model_db) -> Self`：创建会话实例；世界网格始终为单一合并网格。
- `set_auth_token(token)`：设置随每个 HELLO 发送的共享密钥（RFC v3.1 §4.1，自动 `trim`）。
- `start(url, auto_reconnect, max_reconnect_attempts) -> Result<(), String>`：连接 Minecraft 服务端并启动后台 I/O 与网格化守护线程。
- `start_with_cache(...)`：携带缓存参数启动，支持基于哈希指纹匹配的 0ms 零网格重建秒级恢复。
- `stop()`：优雅且强制终止网络连接（触发底层 TCP `shutdown(Shutdown::Both)` 瞬间释放端口）与后台 Worker 线程。
- `is_active() -> bool`：查询后台会话线程是否处于活跃状态。
- `is_connected() -> bool`：查询当前是否已建立有效网络连接。
- `status() -> String`：获取当前连接状态文本（`"CONNECTED"`, `"CONNECTING..."`, `"DISCONNECTED"` 等）。
- `poll_events() -> Vec<SyncEvent>`：非阻塞提取当前已就绪的所有高阶事件。
- `poll_events_coalesced() -> Vec<SyncEvent>`：只保留最新 `WorldMeshReady` generation，避免宿主主线程被中间过渡网格淹没。
- `get_world_mesh() -> MeshData`：主动并行网格化当前 `VoxelWorld` 内的所有区块并合并为全局网格。
- `request_resync() -> Result<(), String>`：客户端主动向服务端发送 `ResyncRequest` 请求恢复。

---

## 5. 快速上手示例 (Quick Start)

### 示例 1：启动 Live Sync 会话并轮询事件流

```rust
use std::thread;
use std::time::Duration;
use mtk_sync::{LiveSyncSession, SyncEvent};
use mtk_voxel::{CoordinateSystem, MesherConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置网格化参数
    let config = MesherConfig {
        coordinate_system: CoordinateSystem::ZUpRightHanded,
        origin_centered: true,
        weld_vertices: true,
        ..Default::default()
    };

    // 2. 初始化 LiveSyncSession
    let mut session = LiveSyncSession::new(Some(config), None, None);
    session.set_auth_token("my-secret-token");

    // 3. 连接 Minecraft Yefira 模组 (本地默认端口 25566)
    println!("正在连接 Live Sync 协同服务器...");
    session.start("ws://127.0.0.1:25566", true, 5)?;

    // 4. 事件轮询主循环
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(50));

        let events = session.poll_events_coalesced();
        for evt in events {
            match evt {
                SyncEvent::StatusChange(status) => {
                    println!("[状态变更]: {}", status);
                }
                SyncEvent::SelectionUpdated { min_pos, size } => {
                    println!("[选区更新]: 最小坐标 {:?}, 尺寸 {:?}", min_pos, size);
                }
                SyncEvent::WorldMeshReady { mesh, generation, .. } => {
                    println!("[网格就绪 gen={}]: 顶点数 = {}", generation, mesh.vertex_count());
                }
                SyncEvent::DeltaApplied { change_count, affected_sections } => {
                    println!("[增量变动]: {} 个方块修改, 影响 {} 个区块", change_count, affected_sections.len());
                }
                SyncEvent::StreamProgress { stage, current, total, message } => {
                    println!("[进度 - {} ({}/{})]: {}", stage, current, total, message);
                }
                SyncEvent::Verified { is_verified, message } => {
                    println!("[状态校验]: 验证结果 = {}, 信息 = {}", is_verified, message);
                }
                _ => {}
            }
        }
    }

    // 5. 退出会话
    session.stop();
    Ok(())
}
```

---

### 示例 2：二进制数据包编解码

```rust
use mtk_sync::{decode_packet, encode_packet, Packet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 构造一个 Ping 数据包
    let ping_packet = Packet::Ping { timestamp: 12345678 };
    let encoded_bytes = encode_packet(&ping_packet);

    // 解码数据包
    let decoded_packet = decode_packet(&encoded_bytes)?;
    assert_eq!(decoded_packet, ping_packet);
    Ok(())
}
```

---

## 6. 开源协议 (License)

本项目遵循 [GNU General Public License v3.0 or later (GPL-3.0-or-later)](../../LICENSE) 开源协议。
