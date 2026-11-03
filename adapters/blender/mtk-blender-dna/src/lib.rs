//! Blender-oriented semantic wrappers over `mtk-host-memory`.
//!
//! These functions accept raw slices only. They never reference `MeshData`,
//! `VoxelPointCloud`, or any other core type; the Python adapter is responsible
//! for producing the correct contiguous buffer view.

pub use mtk_host_memory::{
    copy_f32_to_ptr, copy_i32_to_ptr, copy_slice_to_ptr, copy_u32_to_ptr, copy_u8_to_ptr,
    read_f32_from_ptr, read_i32_from_ptr, read_ptr_to_slice, read_u32_from_ptr, read_u8_from_ptr,
    validate_destination, validate_pointer, validate_source, MemoryError as DnaError,
    MemoryResult as DnaResult,
};

/// Copies contiguous `[f32; 3]` positions into Blender's position layer.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_positions_to_ptr(
    src: &[f32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_f32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies contiguous `[f32; 3]` normals into Blender's normal layer.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_normals_to_ptr(
    src: &[f32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_f32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies `u32` triangle or quad indices into Blender's corner-vertex layer.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_indices_to_ptr(
    src: &[u32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_u32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies expanded per-loop `[f32; 2]` UVs into Blender's UV layer.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_loop_uvs_to_ptr(
    src: &[f32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_f32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies per-vertex or per-loop `[f32; 4]` colors into Blender's color layer.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_colors_to_ptr(
    src: &[f32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_f32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud `[f32; 3]` positions.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_positions_to_ptr(
    src: &[f32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_f32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud block X coordinates.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_block_x_to_ptr(
    src: &[i32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_i32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud block Y coordinates.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_block_y_to_ptr(
    src: &[i32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_i32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud block Z coordinates.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_block_z_to_ptr(
    src: &[i32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_i32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud state palette indices.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_state_indices_to_ptr(
    src: &[i32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_i32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud biome palette indices.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_biome_indices_to_ptr(
    src: &[i32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_i32_to_ptr(src, dst_ptr, max_bytes) }
}

/// Copies point-cloud light levels.
///
/// # Safety
/// `dst_ptr` must point to enough writable bytes for the copied slice.
pub unsafe fn copy_point_cloud_light_levels_to_ptr(
    src: &[u8],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> DnaResult<usize> {
    unsafe { copy_u8_to_ptr(src, dst_ptr, max_bytes) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_positions_and_indices() {
        let src = [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        let mut dst = [0.0f32; 6];
        let bytes =
            unsafe { copy_positions_to_ptr(&src, dst.as_mut_ptr() as usize, Some(24)) }.unwrap();
        assert_eq!(bytes, 24);
        assert_eq!(dst, src);

        let idx = [0u32, 1, 0];
        let mut idx_dst = [0u32; 3];
        let bytes =
            unsafe { copy_indices_to_ptr(&idx, idx_dst.as_mut_ptr() as usize, None) }.unwrap();
        assert_eq!(bytes, 12);
        assert_eq!(idx_dst, idx);
    }
}
