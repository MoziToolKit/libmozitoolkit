//! Host-agnostic raw slice to destination-pointer copy primitives.
//!
//! This crate intentionally has no dependency on `mtk-core`, `mtk-voxel`, or any
//! host SDK. Host adapters obtain a typed source slice through Python's Buffer
//! Protocol and pass a destination address owned by the host application.

use std::mem::align_of;

/// Errors raised by raw pointer copies.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error("destination pointer is null")]
    NullPointer,
    #[error("destination pointer 0x{ptr:x} is not aligned to {align} bytes")]
    MisalignedPointer { ptr: usize, align: usize },
    #[error("write of {required} bytes exceeds destination capacity of {limit} bytes")]
    BufferOverflow { required: usize, limit: usize },
}

pub(crate) type MemoryResult<T> = Result<T, MemoryError>;

/// Validates a host memory pointer and capacity.
#[inline]
pub fn validate_pointer(
    ptr: usize,
    required_bytes: usize,
    align: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<()> {
    if ptr == 0 {
        return Err(MemoryError::NullPointer);
    }
    if align > 1 && ptr % align != 0 {
        return Err(MemoryError::MisalignedPointer { ptr, align });
    }
    if let Some(limit) = max_bytes {
        if required_bytes > limit {
            return Err(MemoryError::BufferOverflow {
                required: required_bytes,
                limit,
            });
        }
    }
    Ok(())
}

/// Validates a destination pointer and capacity.
#[inline]
pub fn validate_destination(
    dst_ptr: usize,
    required_bytes: usize,
    align: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<()> {
    validate_pointer(dst_ptr, required_bytes, align, max_bytes)
}

/// Validates a source pointer and capacity.
#[inline]
pub fn validate_source(
    src_ptr: usize,
    required_bytes: usize,
    align: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<()> {
    validate_pointer(src_ptr, required_bytes, align, max_bytes)
}

/// Copies a typed slice into a host destination pointer.
///
/// # Safety
///
/// The caller must guarantee that `dst_ptr` points to at least
/// `src.len() * size_of::<T>()` writable bytes. `max_bytes` is an additional
/// defensive capacity bound and is not a substitute for that guarantee.
#[inline]
pub unsafe fn copy_slice_to_ptr<T: Copy>(
    src: &[T],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<usize> {
    if src.is_empty() {
        return Ok(0);
    }
    let byte_size = std::mem::size_of_val(src);
    validate_destination(dst_ptr, byte_size, align_of::<T>(), max_bytes)?;
    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr() as *const u8, dst_ptr as *mut u8, byte_size);
    }
    Ok(byte_size)
}

/// Copies an `f32` slice into a host destination pointer.
///
/// # Safety
///
/// `dst_ptr` must point to at least `src.len() * size_of::<f32>()` writable bytes.
#[inline]
pub unsafe fn copy_f32_to_ptr(
    src: &[f32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<usize> {
    copy_slice_to_ptr(src, dst_ptr, max_bytes)
}

/// Copies an `i32` slice into a host destination pointer.
///
/// # Safety
///
/// `dst_ptr` must point to at least `src.len() * size_of::<i32>()` writable bytes.
#[inline]
pub unsafe fn copy_i32_to_ptr(
    src: &[i32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<usize> {
    copy_slice_to_ptr(src, dst_ptr, max_bytes)
}

/// Copies a `u32` slice into a host destination pointer.
///
/// # Safety
///
/// `dst_ptr` must point to at least `src.len() * size_of::<u32>()` writable bytes.
#[inline]
pub unsafe fn copy_u32_to_ptr(
    src: &[u32],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<usize> {
    copy_slice_to_ptr(src, dst_ptr, max_bytes)
}

/// Copies a `u8` slice into a host destination pointer.
///
/// # Safety
///
/// `dst_ptr` must point to at least `src.len() * size_of::<u8>()` writable bytes.
#[inline]
pub unsafe fn copy_u8_to_ptr(
    src: &[u8],
    dst_ptr: usize,
    max_bytes: Option<usize>,
) -> MemoryResult<usize> {
    copy_slice_to_ptr(src, dst_ptr, max_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_f32_slice() {
        let src = [1.0f32, 2.0, 3.0, 4.0];
        let mut dst = [0.0f32; 4];
        let written =
            unsafe { copy_f32_to_ptr(&src, dst.as_mut_ptr() as usize, Some(16)) }.unwrap();
        assert_eq!(written, 16);
        assert_eq!(dst, src);
    }

    #[test]
    fn copies_u8_slice() {
        let src = [1u8, 2, 3];
        let mut dst = [0u8; 3];
        let written = unsafe { copy_u8_to_ptr(&src, dst.as_mut_ptr() as usize, None) }.unwrap();
        assert_eq!(written, 3);
        assert_eq!(dst, src);
    }

    #[test]
    fn rejects_null_and_overflow_and_misalignment() {
        let src = [1.0f32, 2.0];
        assert_eq!(
            unsafe { copy_f32_to_ptr(&src, 0, None) },
            Err(MemoryError::NullPointer)
        );
        let mut dst = [0.0f32; 2];
        assert!(matches!(
            unsafe { copy_f32_to_ptr(&src, dst.as_mut_ptr() as usize, Some(4)) },
            Err(MemoryError::BufferOverflow { .. })
        ));
        assert!(matches!(
            unsafe { copy_f32_to_ptr(&src, dst.as_mut_ptr() as usize + 1, None) },
            Err(MemoryError::MisalignedPointer { .. })
        ));
    }

    #[test]
    fn empty_slice_is_noop() {
        let src: [f32; 0] = [];
        assert_eq!(unsafe { copy_f32_to_ptr(&src, 0, None) }.unwrap(), 0);
    }
}
