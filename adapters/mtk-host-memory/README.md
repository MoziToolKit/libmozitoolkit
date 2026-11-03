# mtk-host-memory

Host-agnostic raw slice ↔ destination/source pointer memory copy and direct read primitives.

- No `mtk-core` / `mtk-voxel` / host SDK dependency.
- Typed `f32` / `i32` / `u32` / `u8` copies and direct reads with null/alignment/capacity guards.
- Direct read primitives: `read_f32_from_ptr`, `read_i32_from_ptr`, `read_u32_from_ptr`, `read_u8_from_ptr`.
- Host adapters obtain source slices via Python's Buffer Protocol and pass host-owned addresses for zero-copy transfers.
