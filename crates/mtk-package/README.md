# mtk-package

`mtk-package` 是 MoziToolKit 生态中纯 Rust、宿主无关的统一二进制存储包容器编解码器，用于全量资产预编译缓存 (`.mtkcache`) 与轻量场景交换包 (`.mtkscene`)。

---

## 1. 架构定位

- **物理规范**：严格遵循 64 字节固定头部 + 64 字节内存对齐 Chunk Payloads + 块级 Zstd 压缩 TOC。
- **零拷贝寻址**：基于 `memmap2` 内存映射实现零拷贝直读，无需将整个文件预先载入内存。
- **分块按需压缩**：
  - 数据与模型（`MODL`, `META`, JSON 映射表）：默认采用 Zstd 块级压缩；
  - 贴图与图像（`ATLS`, `TXTR` 中的 PNG 字节流）：采用 `CompressionType::None` 直存原始字节流，避免二次压缩损耗性能。

---

## 2. 核心类型与公共 API

- [`PackageHeader`](src/header.rs)：64 字节头部结构体，包含 Magic `b"MTKP"`、Profile、版本与 16 字节 Fingerprint。
- [`PackageProfile`](src/header.rs)：包用途配置枚举（`AssetCache`, `SceneInterchange`）。
- [`MtkPackageWriter`](src/writer.rs) / [`MtkPackageFileWriter`](src/writer.rs)：流式与文件原子包写入器，自动处理 64 字节边界对齐、分块压缩与 CRC32 计算。
- [`MtkPackageReader`](src/reader.rs)：内存映射与缓冲包解析器，支持 `read_header_only` 微秒级头部校验、按块分类枚举 (`iter_chunk_identifiers`) 与按需解压提取。
- [`ChunkWriteOptions`](src/writer.rs)：分块写入压缩配置（`raw()`, `zstd_fast()`）。

---

## 3. 极简使用示例

### 写入包文件 (`.mtkcache`)

```rust
use mtk_package::{MtkPackageFileWriter, PackageProfile, ChunkWriteOptions};

let mut writer = MtkPackageFileWriter::create("test.mtkcache", PackageProfile::AssetCache, [0u8; 16])?;

// 1. 写入模型数据（Zstd 压缩）
writer.add_chunk(*b"MODL", "models/database", &model_bytes, &ChunkWriteOptions::zstd_fast())?;

// 2. 写入图集图片（Raw 不压缩）
writer.add_chunk(*b"ATLS", "atlas/chunk_001.png", &png_bytes, &ChunkWriteOptions::raw())?;

// 3. 写入元数据 JSON
writer.add_json_chunk(*b"META", "manifest", &manifest_obj, &ChunkWriteOptions::zstd_fast())?;

// 4. 完成写入（自动对齐并更新头部 TOC 偏移）
let header = writer.finish()?;
```

### 读取包文件

```rust
use mtk_package::MtkPackageReader;

let reader = MtkPackageReader::open_file("test.mtkcache")?;

// 1. 获取模型数据
let model_bytes = reader.read_chunk_decompressed("models/database")?;

// 2. 零拷贝获取贴图数据或提取到文件
reader.extract_chunk_to_file("atlas/chunk_001.png", "output/chunk_001.png")?;
```
