//! # JNI Conversion Helpers
//!
//! Type extraction utilities between JVM arrays/strings and Rust vectors.

use jni::objects::{JIntArray, JObjectArray, JString};
use jni::JNIEnv;

/// Converts jobjectArray of Strings into `Vec<String>`.
pub(crate) fn extract_string_array(
    env: &mut JNIEnv,
    arr: &JObjectArray,
) -> Result<Vec<String>, String> {
    if arr.is_null() {
        return Ok(Vec::new());
    }
    let len = env
        .get_array_length(arr)
        .map_err(|e| format!("Failed to get array length: {}", e))? as usize;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let obj = env
            .get_object_array_element(arr, i as i32)
            .map_err(|e| format!("Failed to get element {}: {}", i, e))?;
        if obj.is_null() {
            out.push(String::new());
        } else {
            let jstr: JString = obj.into();
            let rust_str = env
                .get_string(&jstr)
                .map_err(|e| format!("Failed to extract string {}: {}", i, e))?;
            out.push(rust_str.into());
        }
    }
    Ok(out)
}

/// Converts jobjectArray of Strings into `Vec<Option<String>>` (preserves nulls).
pub(crate) fn extract_optional_string_array(
    env: &mut JNIEnv,
    arr: &JObjectArray,
) -> Result<Vec<Option<String>>, String> {
    if arr.is_null() {
        return Ok(Vec::new());
    }
    let len = env
        .get_array_length(arr)
        .map_err(|e| format!("Failed to get array length: {}", e))? as usize;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let obj = env
            .get_object_array_element(arr, i as i32)
            .map_err(|e| format!("Failed to get element {}: {}", i, e))?;
        if obj.is_null() {
            out.push(None);
        } else {
            let jstr: JString = obj.into();
            let rust_str = env
                .get_string(&jstr)
                .map_err(|e| format!("Failed to extract string {}: {}", i, e))?;
            out.push(Some(rust_str.into()));
        }
    }
    Ok(out)
}

/// Extracts JIntArray into `Vec<u32>`.
pub(crate) fn extract_int_array(env: &mut JNIEnv, arr: &JIntArray) -> Result<Vec<u32>, String> {
    if arr.is_null() {
        return Ok(Vec::new());
    }
    let len = env
        .get_array_length(arr)
        .map_err(|e| format!("Failed to get int array length: {}", e))? as usize;
    let mut buf = vec![0i32; len];
    env.get_int_array_region(arr, 0, &mut buf)
        .map_err(|e| format!("Failed to copy int array: {}", e))?;
    Ok(buf.into_iter().map(|v| v as u32).collect())
}
