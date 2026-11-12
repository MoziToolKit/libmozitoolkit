//! # `mtk-jni` - Java Native Interface Bindings for libmtk
//!
//! Exposes authoritative Live Sync RFC v3.1 / v3.2 WebSocket server,
//! baseline streaming, delta submission, and event polling to JVM callers.

use jni::objects::{JByteArray, JClass};
use jni::sys::jlong;
use jni::JNIEnv;

pub(crate) mod convert;
pub mod event;
pub mod server;

/// Computes 64-bit xxHash (xxHash64) over arbitrary byte array.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeXxHash64(
    env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    seed: jlong,
) -> jlong {
    if data.is_null() {
        return 0;
    }
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    use xxhash_rust::xxh64::Xxh64;
    let mut hasher = Xxh64::new(seed as u64);
    hasher.update(&bytes);
    hasher.digest() as jlong
}
