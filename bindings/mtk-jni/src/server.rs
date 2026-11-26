//! # Native LiveSync Server JNI Interface
//!
//! Exposes RFC v3.1 / v3.2 server lifecycle, selection, baseline streaming,
//! delta updates, and event polling to the JVM.

use glam::IVec3;
use jni::objects::{JByteArray, JClass, JIntArray, JObjectArray, JString};
use jni::sys::{jboolean, jint, jlong, jstring};
use jni::JNIEnv;

use mtk_sync::server::{LiveSyncServer, ServerConfig};
use mtk_sync::InstanceMetadata;

use super::convert::{extract_int_array, extract_optional_string_array, extract_string_array};
use crate::handle::HandleTable;

/// Live native server instances, keyed by the opaque handle handed to the JVM.
pub(crate) static SERVERS: HandleTable<LiveSyncServer> = HandleTable::new();

fn read_optional_jstring(env: &mut JNIEnv, s: &JString) -> String {
    if s.is_null() {
        return String::new();
    }
    env.get_string(s)
        .map(|js| {
            let v: String = js.into();
            v.trim().to_string()
        })
        .unwrap_or_default()
}

/// Starts a native LiveSync server (IPC and/or WebSocket). Returns an opaque handle (jlong) or 0 on error.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerStart(
    mut env: JNIEnv,
    _class: JClass,
    enable_ipc: jboolean,
    world_name: JString,
    mc_version: JString,
    loader: JString,
    dimension: JString,
    require_token_on_ipc: jboolean,
    enable_ws: jboolean,
    host: JString,
    port: jint,
    token: JString,
    allow_loopback: jboolean,
) -> jlong {
    let ws_enabled = enable_ws != 0;
    let ipc_enabled = enable_ipc != 0;
    if !ws_enabled && !ipc_enabled {
        return 0;
    }

    let bind_address = if ws_enabled {
        let host_str = read_optional_jstring(&mut env, &host);
        let resolved_host = if host_str.is_empty() {
            "127.0.0.1".to_string()
        } else {
            host_str
        };
        Some(format!("{}:{}", resolved_host, port))
    } else {
        None
    };

    let token_str = read_optional_jstring(&mut env, &token);
    let token_opt = if token_str.is_empty() {
        None
    } else {
        Some(token_str)
    };

    let instance_info = if ipc_enabled {
        Some(InstanceMetadata {
            world_name: read_optional_jstring(&mut env, &world_name),
            mc_version: read_optional_jstring(&mut env, &mc_version),
            loader: read_optional_jstring(&mut env, &loader),
            dimension: {
                let d = read_optional_jstring(&mut env, &dimension);
                if d.is_empty() {
                    "minecraft:overworld".to_string()
                } else {
                    d
                }
            },
        })
    } else {
        None
    };

    let config = ServerConfig {
        bind_address,
        enable_ipc: ipc_enabled,
        ipc_name: None,
        instance_info,
        auth_token: token_opt,
        allow_loopback_without_token: allow_loopback != 0,
        require_token_on_ipc: require_token_on_ipc != 0,
        server_uuid: [0u8; 16],
        max_connections: 8,
        handshake_timeout: std::time::Duration::from_millis(5000),
        ping_interval: std::time::Duration::from_millis(5000),
    };

    match LiveSyncServer::start(config) {
        Ok(server) => SERVERS.insert(server) as jlong,
        Err(_) => 0,
    }
}

/// Returns the local IPC URL (`ipc://...`) of the native server, or null if IPC is disabled.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerGetIpcUrl(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let Some(server) = SERVERS.get(handle as u64) else {
        return std::ptr::null_mut();
    };
    let Some(url) = server.ipc_url() else {
        return std::ptr::null_mut();
    };
    match env.new_string(url) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Updates descriptive discovery metadata on a running IPC server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerUpdateMetadata(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    world_name: JString,
    mc_version: JString,
    loader: JString,
    dimension: JString,
) {
    let Some(server) = SERVERS.get(handle as u64) else {
        return;
    };
    server.update_instance_metadata(InstanceMetadata {
        world_name: read_optional_jstring(&mut env, &world_name),
        mc_version: read_optional_jstring(&mut env, &mc_version),
        loader: read_optional_jstring(&mut env, &loader),
        dimension: read_optional_jstring(&mut env, &dimension),
    });
}

/// Stops the native server and retires its handle.
///
/// Calls already in flight hold an `Arc` and finish safely; the handle stops resolving, so a
/// worker thread polling after the stop gets the "not running" answer instead of freed memory.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerStop(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if let Some(server) = SERVERS.remove(handle as u64) {
        server.stop();
    }
}

/// Returns whether the native server is running.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerIsRunning(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    let Some(server) = SERVERS.get(handle as u64) else {
        return 0;
    };
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
    let Some(server) = SERVERS.get(handle as u64) else {
        return 0;
    };
    server.local_addr().map(|a| a.port() as jint).unwrap_or(0)
}

/// Returns the number of connected clients on the native server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerGetClientCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    let Some(server) = SERVERS.get(handle as u64) else {
        return 0;
    };
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
    let Some(server) = SERVERS.get(handle as u64) else {
        return;
    };
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
    let Some(server) = SERVERS.get(handle as u64) else {
        return;
    };
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
    let Some(server) = SERVERS.get(handle as u64) else {
        return 0;
    };

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

/// Broadcasts a pre-encoded raw RFC frame to all connected clients.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerBroadcastPacket(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    data: JByteArray,
) {
    if data.is_null() {
        return;
    }
    let Some(server) = SERVERS.get(handle as u64) else {
        return;
    };
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return,
    };
    server.broadcast_packet_bytes(bytes);
}

/// Submits incremental block and entity delta changes to the server.
/// The server auto-registers palette additions, creates DeltaOp and DeltaBlockEntity frames, and broadcasts.
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
    let Some(server) = SERVERS.get(handle as u64) else {
        return;
    };

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
