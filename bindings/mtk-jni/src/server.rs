//! # Native LiveSync Server JNI Interface
//!
//! Exposes RFC v3.1 / v3.2 server lifecycle, selection, baseline streaming,
//! delta updates, and event polling to the JVM.

use glam::IVec3;
use jni::objects::{JByteArray, JClass, JIntArray, JObjectArray, JString};
use jni::sys::{jboolean, jbyte, jdouble, jfloat, jint, jlong, jobjectArray};
use jni::JNIEnv;

use mtk_sync::server::{LiveSyncServer, ServerConfig};
use mtk_sync::VisualEntityDelta;

use super::convert::{extract_int_array, extract_optional_string_array, extract_string_array};

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
    visual_entities_bytes: JByteArray,
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

    let mut visual_entities = Vec::new();
    if !visual_entities_bytes.is_null() {
        if let Ok(bytes) = env.convert_byte_array(&visual_entities_bytes) {
            let mut offset = 0;
            while offset < bytes.len() {
                match mtk_sync::protocol::read_visual_entity(&bytes, &mut offset) {
                    Ok(e) => visual_entities.push(e),
                    Err(_) => break,
                }
            }
        }
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
        &visual_entities,
    ) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

/// Returns the combined array of subscribed visual entity types from all connected clients.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerGetSubscribedVisualEntities(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    let subs = server.subscribed_visual_entities();

    let string_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let result_arr = match env.new_object_array(
        subs.len() as i32,
        &string_class,
        jni::objects::JObject::null(),
    ) {
        Ok(arr) => arr,
        Err(_) => return std::ptr::null_mut(),
    };

    for (i, s) in subs.iter().enumerate() {
        let j_str = match env.new_string(s) {
            Ok(js) => js,
            Err(_) => return std::ptr::null_mut(),
        };
        if env
            .set_object_array_element(&result_arr, i as i32, &j_str)
            .is_err()
        {
            return std::ptr::null_mut();
        }
    }

    result_arr.into_raw()
}

/// Submits an incremental visual entity UPSERT to the server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerSubmitVisualEntityUpsert(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    most_sig: jlong,
    least_sig: jlong,
    id: JString,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    pitch: jfloat,
    yaw: jfloat,
    roll: jfloat,
    scale_x: jfloat,
    scale_y: jfloat,
    scale_z: jfloat,
    facing: jbyte,
    custom_payload: JByteArray,
) {
    if handle == 0 {
        return;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    let id_str: String = match env.get_string(&id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let payload = if !custom_payload.is_null() {
        env.convert_byte_array(&custom_payload).unwrap_or_default()
    } else {
        Vec::new()
    };

    let uuid_str = format!("{:016x}{:016x}", most_sig as u64, least_sig as u64);
    let facing_dir = mtk_sync::protocol::facing_byte_to_direction(facing as u8);
    let kind: mtk_model::EntityKindData = if !payload.is_empty() {
        serde_json::from_slice(&payload).unwrap_or_else(|_| mtk_model::EntityKindData::Generic {
            custom_id: id_str.clone(),
            payload: payload.clone(),
        })
    } else {
        mtk_model::EntityKindData::Generic {
            custom_id: id_str.clone(),
            payload: Vec::new(),
        }
    };

    let entity = mtk_model::EntityInstance {
        uuid: Some(uuid_str),
        id: id_str,
        custom_name: None,
        position: glam::DVec3::new(x, y, z),
        facing: facing_dir,
        rotation: glam::Vec3::new(pitch, yaw, roll),
        scale: glam::Vec3::new(scale_x, scale_y, scale_z),
        kind,
    };

    server.submit_visual_entity_delta(VisualEntityDelta::Upsert(entity));
}

/// Submits an incremental visual entity REMOVE to the server.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerSubmitVisualEntityRemove(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    most_sig: jlong,
    least_sig: jlong,
) {
    if handle == 0 {
        return;
    }
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    let uuid_str = format!("{:016x}{:016x}", most_sig as u64, least_sig as u64);
    server.submit_visual_entity_delta(VisualEntityDelta::Remove { uuid: uuid_str });
}

/// Broadcasts a pre-encoded raw RFC frame to all connected clients.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerBroadcastPacket(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    data: JByteArray,
) {
    if handle == 0 || data.is_null() {
        return;
    }
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return,
    };
    let server = unsafe { &*(handle as *const LiveSyncServer) };
    server.broadcast_packet_bytes(bytes);
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
