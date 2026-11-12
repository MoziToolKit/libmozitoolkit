# `mtk-jni` (Java Native Interface Bindings for libmtk)

Provides lean, high-performance JNI bindings for `libmozitoolkit` to power Minecraft server-side mods (`yefira`) and JVM data pipelines.

## Features
- **Zero-Copy Memory Ingestion**: Consumes primitive arrays and direct byte buffers from Java without redundant heap allocation.
- **Authoritative SSOT State Hash**: 100% bit-exact xxHash64 calculation aligned with RFC v3.1.
- **Fast Baseline Chunking**: Direct chunked baseline streaming with Zstd compression and bit-packing.
- **Lightweight Footprint**: Feature-gated (`sync-server` default) to produce a compact binary (< 1.5MB) stripped of client, mesher, and GUI overhead.
