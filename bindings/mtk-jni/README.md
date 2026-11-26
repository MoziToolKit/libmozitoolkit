# `mtk-jni` (Java Native Interface Bindings for libmtk)

[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/gpl-3.0.html)

**`mtk-jni`** 是 `libmozitoolkit` 原生核心的高性能 Java Native Interface (JNI) 绑定 Crate。
作为专为 Minecraft 服务端伴生模组（[`yefira`](https://github.com/MoziToolKit/yefira)）与 JVM 数据管道量身打造的跨平台底层扩展，它将 Protocol v4（`0x04`）二进制流式封包、xxHash64 权威指纹计算与多线程异步 WebSocket 服务端（`LiveSyncServer`）全量下沉至 Rust 原生层，消除 JVM 垃圾回收（GC）暂停与低效对象分配。

---

## 1. 核心架构与设计原则

```
┌────────────────────────────────────────────────────────┐
│             Minecraft (Yefira Mod / JVM)               │
│     CanonicalVoxelScanner  │  BlockEntitySerializer    │
│             ▲                        ▲                 │
│             │ JNI Primitive Arrays   │ String Arrays   │
│             ▼                        ▼                 │
│         NativeSync.java (JNI Bridge Interface)         │
└───────────────────────────┬────────────────────────────┘
                            │ JNI Call (C-ABI / Opaque Handle IDs)
                            ▼
┌────────────────────────────────────────────────────────┐
│                   libmtk_jni (Rust)                    │
│  ┌───────────────────┐    ┌─────────────────────────┐  │
│  │ nativeXxHash64    │    │ nativeServer*           │  │
│  └───────────────────┘    └────────────┬────────────┘  │
│                                        ▼               │
│                          HandleTable<LiveSyncServer>   │
│                          LiveSyncServer (mtk-sync)     │
│                            - Multi-client WebSocket    │
│                            - Zstd Chunk Streaming      │
│                            - Delta Batch & BlockEntity │
└────────────────────────────────────────────────────────┘
```

- **零拷贝原生数据摄取 (Zero-Copy Ingestion)**：直接消费 Java 原始基本类型数组（`int[]`, `byte[]`）与字符串数组，无需在 JVM 堆上反复构建昂贵的数据包 DTO。
- **句柄表生命周期安全 (HandleTable Safety)**：通过单调递增不透明句柄 ID 索引 `Arc<LiveSyncServer>`，避免跨线程轮询与停服时的悬垂指针解引用；停服后旧句柄自动失效并安全返回空/未运行状态。
- **单一事实源指纹 (SSOT State Hash)**：由 Rust 原生执行 100% 字节级一致的 xxHash64 稀疏指纹计算，确保三端自愈校验严格对齐。
- **纯原生多线程网络服务**：内置独立的多线程 WebSocket 服务端（基于 `tungstenite` 与线程安全代际控制），绝不阻塞 Minecraft 游戏主刻时钟（Server Tick Loop）。
- **极简紧凑体积**：通过 Cargo 特性隔离裁剪客户端与网格化模块，产出体积小于 2MB 的纯净轻量动态链接库。

---

## 2. Java 映射契约 (`NativeSync.java`)

Java 端对应类路径为 `com.mozi1924.yefira.encoder.NativeSync`。

### 2.1 基础与哈希运算
```java
// 权威 xxHash64 计算
public static native long nativeXxHash64(byte[] data, long seed);
```

### 2.2 服务端生命周期控制
```java
// 启动 LiveSyncServer 实例（支持本地 IPC 与可选 WebSocket 监听），返回不透明句柄 ID（失败返回 0）
public static native long nativeServerStart(
    boolean enableIpc, String worldName, String mcVersion, String loader, String dimension,
    boolean requireTokenOnIpc,
    boolean enableWs, String host, int port, String token, boolean allowLoopback
);

// 查询本地 IPC URL 与更新实例发现元数据
public static native String nativeServerGetIpcUrl(long handle);
public static native void nativeServerUpdateMetadata(
    long handle, String worldName, String mcVersion, String loader, String dimension
);

// 停止服务端实例并注销句柄（安全释放底层 TCP 端口与工作线程）
public static native void nativeServerStop(long handle);

// 运行状态与端口查询
public static native boolean nativeServerIsRunning(long handle);
public static native int nativeServerGetPort(long handle);
public static native int nativeServerGetClientCount(long handle);
```

### 2.3 选区与数据流式提交
```java
// 设置选区包围盒与维度
public static native void nativeServerSetSelection(
    long handle, int minX, int minY, int minZ, int sizeX, int sizeY, int sizeZ, String dimension
);

// 清空当前选区
public static native void nativeServerClearSelection(long handle);

// 全量基线流式推送（自动计算 state_hash、Zstd 分块编码与下发；包含方块实体载荷）
public static native boolean nativeServerSubmitBaseline(
    long handle, long clientId,
    int minX, int minY, int minZ, int sizeX, int sizeY, int sizeZ,
    String dimension,
    int[] rawStates, String[] statePalette,
    int[] rawBiomes, String[] biomePalette,
    int[] entityCoords, String[] entityPayloads
);

// 增量变动批量广播（自动注册调色板并下发 DeltaOp / DeltaBlockEntity）
public static native void nativeServerSubmitDeltas(
    long handle,
    int[] relCoords, String[] stateStrings,
    int[] entityCoords, String[] entityPayloads
);

// 广播预编码的原始协议帧
public static native void nativeServerBroadcastPacket(long handle, byte[] data);
```

### 2.4 Protocol v4 事件轮询
```java
// 原生事件轮询 (client_connected=1, client_disconnected=2, baseline_requested=3, client_resync=4, warning=5)
public static native ServerEvent[] nativeServerPollEvents(long handle);
```

---

## 3. 跨平台打包与发布矩阵

动态链接库由 GitHub Actions CI 自动交叉编译并预先打包进 `yefira` 模组 Jar 内的 `/natives/<platform>/` 资源目录：

| 平台目录 | 目标架构 | 编译产物名称 |
| :--- | :--- | :--- |
| `natives/macos-arm64/` | macOS Apple Silicon (ARM64) | `libmtk_jni.dylib` |
| `natives/macos-x64/` | macOS Intel (x86_64) | `libmtk_jni.dylib` |
| `natives/linux-x64/` | Linux glibc (x86_64) | `libmtk_jni.so` |
| `natives/linux-arm64/` | Linux glibc (AArch64) | `libmtk_jni.so` |
| `natives/windows-x64/` | Windows (x86_64) | `mtk_jni.dll` |
| `natives/windows-arm64/` | Windows (ARM64) | `mtk_jni.dll` |

`NativeSync` 在模组启动时，按以下优先级智能定位并加载动态库：
1. 系统显式属性指定：`-Dmtk.jni.path=/path/to/libmtk_jni.dylib`
2. 本地开发工作区相对路径自动嗅探（`target/release/`）
3. 模组 Jar 内部嵌套资源解压至临时目录加载

---

## 4. 本地构建与测试

```bash
# 编译 Release 动态库
cargo build --release -p mtk-jni

# 运行 mtk-jni 单元测试
cargo test -p mtk-jni
```
