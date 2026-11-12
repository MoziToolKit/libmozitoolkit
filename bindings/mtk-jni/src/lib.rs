//! # `mtk-jni` - Java Native Interface Bindings for libmtk
//!
//! Exposes authoritative Live Sync RFC v3.1 / v3.2 WebSocket server,
//! baseline streaming, delta submission, and event polling to JVM callers.

use glam::IVec3;
use jni::objects::{JByteArray, JClass, JIntArray, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jobjectArray};
use jni::JNIEnv;

use mtk_sync::server::{LiveSyncServer, ServerConfig, ServerEvent};

// Helper: converts jobjectArray of Strings into Vec<String>
fn extract_string_array(env: &mut JNIEnv, arr: &JObjectArray) -> Result<Vec<String>, String> {
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

// Helper: converts jobjectArray of Strings into Vec<Option<String>> (preserves nulls)
fn extract_optional_string_array(
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

// Helper: extracts JIntArray into Vec<u32>
fn extract_int_array(env: &mut JNIEnv, arr: &JIntArray) -> Result<Vec<u32>, String> {
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

/// Starts a native LiveSync WebSocket server. Returns a pointer handle (jlong) or 0 on error.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerStart(
    mut env: JNIEnv,
    _class: JClass,
    host: JString,
    port: jint,
    token: JString,
    allow_loopback: jboolean,
) -> jlong {
    let host_str: String = match env.get_string(&host) {
        Ok(s) => s.into(),
        Err(_) => "127.0.0.1".to_string(),
    };
    let token_opt: Option<String> = if !token.is_null() {
        match env.get_string(&token) {
            Ok(s) => {
                let trimmed: String = s.into();
                if trimmed.trim().is_empty() {
                    None
                } else {
                    Some(trimmed.trim().to_string())
                }
            }
            Err(_) => None,
        }
    } else {
        None
    };

    let bind_address = format!("{}:{}", host_str, port);
    let config = ServerConfig {
        bind_address,
        auth_token: token_opt,
        allow_loopback_without_token: allow_loopback != 0,
        server_uuid: [0u8; 16],
        max_connections: 8,
        handshake_timeout: std::time::Duration::from_millis(5000),
        ping_interval: std::time::Duration::from_millis(5000),
    };

    match LiveSyncServer::start(config) {
        Ok(server) => Box::into_raw(Box::new(server)) as jlong,
        Err(_) => 0,
    }
}

/// Stops the native server and destroys its handle.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerStop(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        let mut server = unsafe { Box::from_raw(handle as *mut LiveSyncServer) };
        server.stop();
        drop(server);
    }
}

/// Returns whether the native server is running.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerIsRunning(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    if server.is_running() {
        1
    } else {
        0
    }
}

/// Returns the assigned port of the native server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerGetPort(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    server.local_addr().map(|a| a.port() as jint).unwrap_or(0)
}

/// Returns the number of connected clients on the native server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerGetClientCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    server.client_count() as jint
}

/// Updates the authoritative selection on the native server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerSetSelection(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    min_x: jint,
    min_y: jint,
    min_z: jint,
    size_x: jint,
    size_y: jint,
    size_z: jint,
    dimension: JString,
) {
    if handle == 0 {
        return;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    let dim_str: String = match env.get_string(&dimension) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    server.set_selection(
        IVec3::new(min_x, min_y, min_z),
        IVec3::new(size_x, size_y, size_z),
        dim_str,
    );
}

/// Clears the authoritative selection and cancels any active baseline streams.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerClearSelection(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    server.clear_selection();
}

/// Submits a raw baseline scan to the server. The server computes state_hash, updates selection,
/// initializes the dynamic palette, encodes chunked baseline frames, and streams to client_id (or broadcasts if 0).
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerSubmitBaseline(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    client_id: jlong,
    min_x: jint,
    min_y: jint,
    min_z: jint,
    size_x: jint,
    size_y: jint,
    size_z: jint,
    dimension: JString,
    raw_states: JIntArray,
    state_palette: JObjectArray,
    raw_biomes: JIntArray,
    biome_palette: JObjectArray,
    entity_coords: JIntArray,
    entity_payloads: JObjectArray,
) -> jboolean {
    if handle == 0 {
        return 0;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };

    let dim_str: String = match env.get_string(&dimension) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let states = match extract_int_array(&mut env, &raw_states) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let state_names = match extract_string_array(&mut env, &state_palette) {
        Ok(n) => n,
        Err(_) => return 0,
    };
    let biomes = match extract_int_array(&mut env, &raw_biomes) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    let biome_names = match extract_string_array(&mut env, &biome_palette) {
        Ok(n) => n,
        Err(_) => return 0,
    };

    let e_coords = extract_int_array(&mut env, &entity_coords).unwrap_or_default();
    let e_payloads = extract_string_array(&mut env, &entity_payloads).unwrap_or_default();

    let mut block_entities = Vec::new();
    let num_entities = e_payloads.len().min(e_coords.len() / 3);
    for i in 0..num_entities {
        let rx = e_coords[i * 3] as i32;
        let ry = e_coords[i * 3 + 1] as i32;
        let rz = e_coords[i * 3 + 2] as i32;
        block_entities.push((IVec3::new(rx, ry, rz), e_payloads[i].clone()));
    }

    match server.submit_baseline(
        client_id as u64,
        IVec3::new(min_x, min_y, min_z),
        IVec3::new(size_x, size_y, size_z),
        dim_str,
        &states,
        &state_names,
        &biomes,
        &biome_names,
        &block_entities,
    ) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

/// Submits incremental block and entity delta changes to the server.
/// The server auto-registers palette additions, creates DeltaOp and DeltaEntity frames, and broadcasts.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerSubmitDeltas(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    rel_coords: JIntArray,
    state_strings: JObjectArray,
    entity_coords: JIntArray,
    entity_payloads: JObjectArray,
) {
    if handle == 0 {
        return;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };

    let coords = match extract_int_array(&mut env, &rel_coords) {
        Ok(c) => c,
        Err(_) => return,
    };
    let states = match extract_string_array(&mut env, &state_strings) {
        Ok(s) => s,
        Err(_) => return,
    };

    let num_changes = states.len().min(coords.len() / 3);
    let mut rel_positions = Vec::with_capacity(num_changes);
    let mut state_list = Vec::with_capacity(num_changes);
    for i in 0..num_changes {
        let rx = coords[i * 3] as i32;
        let ry = coords[i * 3 + 1] as i32;
        let rz = coords[i * 3 + 2] as i32;
        rel_positions.push(IVec3::new(rx, ry, rz));
        state_list.push(states[i].clone());
    }

    let e_coords = extract_int_array(&mut env, &entity_coords).unwrap_or_default();
    let e_payloads = extract_optional_string_array(&mut env, &entity_payloads).unwrap_or_default();

    let num_entities = e_payloads.len().min(e_coords.len() / 3);
    let mut entities = Vec::with_capacity(num_entities);
    for i in 0..num_entities {
        let rx = e_coords[i * 3] as i32;
        let ry = e_coords[i * 3 + 1] as i32;
        let rz = e_coords[i * 3 + 2] as i32;
        entities.push((IVec3::new(rx, ry, rz), e_payloads[i].clone()));
    }

    let _ = server.submit_deltas(&rel_positions, &state_list, &entities);
}

/// Polls pending server events and returns an array of NativeSync.ServerEvent.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerPollEvents(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    let events = server.poll_events();

    let event_class = match env.find_class("com/mozi1924/yefira/encoder/NativeSync$ServerEvent") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let result_arr = match env.new_object_array(
        events.len() as i32,
        &event_class,
        jni::objects::JObject::null(),
    ) {
        Ok(arr) => arr,
        Err(_) => return std::ptr::null_mut(),
    };

    for (i, ev) in events.iter().enumerate() {
        let (type_code, client_id, epoch, seq, msg) = match ev {
            ServerEvent::ClientConnected { client_id, addr } => {
                (1i32, *client_id as jlong, 0i64, 0i64, addr.to_string())
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                (2i32, *client_id as jlong, 0i64, 0i64, reason.clone())
            }
            ServerEvent::BaselineRequested {
                client_id,
                epoch,
                snapshot_seq,
            } => (
                3i32,
                *client_id as jlong,
                *epoch as jlong,
                *snapshot_seq as jlong,
                String::new(),
            ),
            ServerEvent::ClientResync { client_id, reason } => (
                4i32,
                *client_id as jlong,
                0i64,
                0i64,
                format!("{:?}", reason),
            ),
        };

        let j_msg = match env.new_string(msg) {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let obj = match env.new_object(
            &event_class,
            "(IJJJLjava/lang/String;)V",
            &[
                JValue::Int(type_code),
                JValue::Long(client_id),
                JValue::Long(epoch),
                JValue::Long(seq),
                JValue::Object(&j_msg),
            ],
        ) {
            Ok(o) => o,
            Err(_) => return std::ptr::null_mut(),
        };

        if env
            .set_object_array_element(&result_arr, i as i32, &obj)
            .is_err()
        {
            return std::ptr::null_mut();
        }
    }

    result_arr.into_raw()
}
