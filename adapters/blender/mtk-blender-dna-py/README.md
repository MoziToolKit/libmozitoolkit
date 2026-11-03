# libmtk_blender_dna

Standalone ABI3 wheel exposing generic raw-memory copy, direct-read, and zero-copy memoryview adapters for Blender DNA buffers.

```python
import libmtk_blender_dna as dna

dna.api_version()  # 2

# Direct copy to Blender memory
dna.copy_f32(src_view, dst_ptr, count, max_bytes=...)
dna.copy_i32(...)
dna.copy_u32(...)
dna.copy_u8(...)

# Direct read from Blender memory to buffer
dna.read_f32(src_ptr, dst_view, count, max_bytes=...)
dna.read_i32(...)
dna.read_u32(...)
dna.read_u8(...)

# Zero-copy read-only memoryview direct mapping
mv = dna.as_memoryview_f32(src_ptr, count)
mv = dna.as_memoryview_i32(src_ptr, count)
```

It has no dependency on `libmtk_py`, `mtk-core`, or `mtk-voxel`.
