//! Blender-oriented host memory adapter over `mtk-host-memory`.
//!
//! Exposes typed contiguous buffer write primitives and safety validators.

pub use mtk_host_memory::{
    copy_f32_to_ptr, copy_i32_to_ptr, copy_slice_to_ptr, copy_u32_to_ptr, copy_u8_to_ptr,
    validate_destination, validate_pointer, validate_source, MemoryError as DnaError,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_positions_and_indices() {
        let src = [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        let mut dst = [0.0f32; 6];
        let bytes = unsafe { copy_f32_to_ptr(&src, dst.as_mut_ptr() as usize, Some(24)) }.unwrap();
        assert_eq!(bytes, 24);
        assert_eq!(dst, src);

        let idx = [0u32, 1, 0];
        let mut idx_dst = [0u32; 3];
        let bytes = unsafe { copy_u32_to_ptr(&idx, idx_dst.as_mut_ptr() as usize, None) }.unwrap();
        assert_eq!(bytes, 12);
        assert_eq!(idx_dst, idx);
    }
}
