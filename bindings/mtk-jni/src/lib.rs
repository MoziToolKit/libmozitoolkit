//! # `mtk-jni` - Java Native Interface Bindings for libmtk
//!
//! Exposes authoritative Live Sync RFC v3.1 / v3.2 baseline chunking,
//! xxHash64 sparse state hash computation, and delta batch packaging to JVM callers.

use glam::IVec3;
use jni::objects::{JByteArray, JClass, JIntArray, JObjectArray, JString};
use jni::sys::{jbyteArray, jint, jlong, jobjectArray};
use jni::JNIEnv;

use mtk_sync::server::{
    decide_handshake, encode_baseline_from_raw, encode_delta_op, BaselineEncoderConfig,
    HandshakeContext,
};
use mtk_sync::{
    compute_sparse_state_hash, decode_packet, encode_packet, DeltaChangeEntry, SparseBlockEntry,
};

// Helper: converts jobjectArray of Strings into Vec<String>
fn extract_string_array(env: &mut JNIEnv, arr: &JObjectArray) -> Result<Vec<String>, String> {
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

// Helper: extracts JIntArray into Vec<u32>
fn extract_int_array(env: &mut JNIEnv, arr: &JIntArray) -> Result<Vec<u32>, String> {
    let len = env
        .get_array_length(arr)
        .map_err(|e| format!("Failed to get int array length: {}", e))? as usize;
    let mut buf = vec![0i32; len];
    env.get_int_array_region(arr, 0, &mut buf)
        .map_err(|e| format!("Failed to copy int array: {}", e))?;
    Ok(buf.into_iter().map(|v| v as u32).collect())
}

// Helper: converts a list of byte slices into a jobjectArray of byte[] (byte[][])
fn to_java_byte_array_array(env: &mut JNIEnv, packets: &[Vec<u8>]) -> Result<jobjectArray, String> {
    let byte_arr_class = env
        .find_class("[B")
        .map_err(|e| format!("Failed to find byte[] class: {}", e))?;
    let dummy = env
        .new_byte_array(0)
        .map_err(|e| format!("Failed to create dummy byte array: {}", e))?;
    let result_arr = env
        .new_object_array(packets.len() as i32, &byte_arr_class, &dummy)
        .map_err(|e| format!("Failed to create result object array: {}", e))?;

    for (i, pkt) in packets.iter().enumerate() {
        let j_bytes = env
            .byte_array_from_slice(pkt)
            .map_err(|e| format!("Failed to convert packet {} to byte array: {}", i, e))?;
        env.set_object_array_element(&result_arr, i as i32, &j_bytes)
            .map_err(|e| format!("Failed to set packet {} in array: {}", i, e))?;
    }

    Ok(result_arr.into_raw())
}

/// Native state hash calculation (RFC v3.1 Section 3 SSOT).
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeComputeStateHash(
    mut env: JNIEnv,
    _class: JClass,
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
    entity_coords: JIntArray,      // Flattened [rx, ry, rz, ...]
    entity_payloads: JObjectArray, // JSON payloads corresponding to coords
) -> jlong {
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

    let mut entity_map = std::collections::HashMap::new();
    let num_entities = e_payloads.len().min(e_coords.len() / 3);
    for i in 0..num_entities {
        let rx = e_coords[i * 3] as i32;
        let ry = e_coords[i * 3 + 1] as i32;
        let rz = e_coords[i * 3 + 2] as i32;
        entity_map.insert(IVec3::new(rx, ry, rz), &e_payloads[i]);
    }

    let min_pos = IVec3::new(min_x, min_y, min_z);
    let size = IVec3::new(size_x, size_y, size_z);
    let sx = size.x as usize;
    let sz = size.z as usize;
    let slice_area = sx * sz;
    let quart_grid = mtk_sync::QuartGrid::from_bounds(min_pos, size);

    let non_air_iter = states.iter().enumerate().filter_map(|(idx, &raw_id)| {
        let state_str = if (raw_id as usize) < state_names.len() {
            &state_names[raw_id as usize]
        } else {
            "minecraft:air"
        };

        if state_str.is_empty() || state_str == "minecraft:air" {
            return None;
        }

        let ly = (idx / slice_area) as u32;
        let rem = idx % slice_area;
        let lz = (rem / sx) as u32;
        let lx = (rem % sx) as u32;

        let q_idx = quart_grid.relative_voxel_to_quart_index(lx, ly, lz);
        let raw_b_id = biomes.get(q_idx).copied().unwrap_or(0);
        let biome = if (raw_b_id as usize) < biome_names.len() {
            &biome_names[raw_b_id as usize]
        } else {
            "minecraft:plains"
        };

        let entity_json = entity_map
            .get(&IVec3::new(lx as i32, ly as i32, lz as i32))
            .map(|s| s.as_str());

        Some(SparseBlockEntry {
            rel_x: lx,
            rel_y: ly,
            rel_z: lz,
            state: state_str,
            biome,
            entity_json,
        })
    });

    compute_sparse_state_hash(min_pos, size, &dim_str, non_air_iter) as jlong
}

/// Native baseline stream encoder. Produces chunked RFC v3.1 byte[][] array.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeEncodeBaseline(
    mut env: JNIEnv,
    _class: JClass,
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
    epoch: jlong,
    snapshot_uuid: JByteArray,
    snapshot_seq: jlong,
    palette_revision: jint,
    max_chunk_size: jint,
    compression_mode: jint,
    compression_level: jint,
) -> jobjectArray {
    let dim_str: String = match env.get_string(&dimension) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let states = match extract_int_array(&mut env, &raw_states) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let state_names = match extract_string_array(&mut env, &state_palette) {
        Ok(n) => n,
        Err(_) => return std::ptr::null_mut(),
    };
    let biomes = match extract_int_array(&mut env, &raw_biomes) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };
    let biome_names = match extract_string_array(&mut env, &biome_palette) {
        Ok(n) => n,
        Err(_) => return std::ptr::null_mut(),
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

    let mut uuid_arr = [0u8; 16];
    if !snapshot_uuid.is_null() {
        if let Ok(vec) = env.convert_byte_array(&snapshot_uuid) {
            if vec.len() >= 16 {
                uuid_arr.copy_from_slice(&vec[..16]);
            }
        }
    }

    let config = BaselineEncoderConfig {
        max_chunk_size: (max_chunk_size as usize).max(1024),
        compression_mode: compression_mode as u8,
        compression_level,
        epoch: epoch as u64,
        snapshot_uuid: uuid_arr,
        snapshot_seq: snapshot_seq as u64,
        palette_revision: palette_revision as u32,
    };

    let stream = match encode_baseline_from_raw(
        IVec3::new(min_x, min_y, min_z),
        IVec3::new(size_x, size_y, size_z),
        &dim_str,
        &states,
        &state_names,
        &biomes,
        &biome_names,
        &block_entities,
        &[],
        &config,
    ) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let mut encoded_packets = Vec::with_capacity(stream.packets.len());
    for pkt in &stream.packets {
        encoded_packets.push(encode_packet(pkt));
    }

    match to_java_byte_array_array(&mut env, &encoded_packets) {
        Ok(arr) => arr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Evaluates client HELLO and produces WELCOME byte[] packet.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeDecideHandshake(
    mut env: JNIEnv,
    _class: JClass,
    hello_bytes: JByteArray,
    server_uuid: JByteArray,
    epoch: jlong,
    sequence: jlong,
    min_x: jint,
    min_y: jint,
    min_z: jint,
    size_x: jint,
    size_y: jint,
    size_z: jint,
    state_hash: jlong,
    dimension: JString,
    expected_auth_token: JString,
) -> jbyteArray {
    let hello_raw = match env.convert_byte_array(&hello_bytes) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let hello_packet = match decode_packet(&hello_raw) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };

    let mut uuid_arr = [0u8; 16];
    if !server_uuid.is_null() {
        if let Ok(vec) = env.convert_byte_array(&server_uuid) {
            if vec.len() >= 16 {
                uuid_arr.copy_from_slice(&vec[..16]);
            }
        }
    }

    let dim_str: String = match env.get_string(&dimension) {
        Ok(s) => s.into(),
        Err(_) => "minecraft:overworld".to_string(),
    };

    let auth_str: Option<String> = if !expected_auth_token.is_null() {
        env.get_string(&expected_auth_token).ok().map(|s| s.into())
    } else {
        None
    };

    let ctx = HandshakeContext {
        server_uuid: uuid_arr,
        epoch: epoch as u64,
        sequence: sequence as u64,
        min_pos: IVec3::new(min_x, min_y, min_z),
        size: IVec3::new(size_x, size_y, size_z),
        state_hash: state_hash as u64,
        dimension: dim_str,
        expected_auth_token: auth_str,
    };

    let result = match decide_handshake(&hello_packet, &ctx) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    let welcome_bytes = encode_packet(&result.welcome_packet);
    match env.byte_array_from_slice(&welcome_bytes) {
        Ok(arr) => arr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Encodes delta changes into chunked RFC DELTA_OP byte[][] array.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeEncodeDelta(
    mut env: JNIEnv,
    _class: JClass,
    epoch: jlong,
    seq_id: jlong,
    palette_revision: jint,
    rel_coords: JIntArray, // Flattened [rx, ry, rz, ...]
    state_ids: JIntArray,  // Array of state_id
) -> jobjectArray {
    let coords = match extract_int_array(&mut env, &rel_coords) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let ids = match extract_int_array(&mut env, &state_ids) {
        Ok(i) => i,
        Err(_) => return std::ptr::null_mut(),
    };

    let num_changes = ids.len().min(coords.len() / 3);
    let mut changes = Vec::with_capacity(num_changes);
    for i in 0..num_changes {
        let rx = coords[i * 3] as i32;
        let ry = coords[i * 3 + 1] as i32;
        let rz = coords[i * 3 + 2] as i32;
        let state_id = ids[i] as u16;
        changes.push(DeltaChangeEntry {
            rel_pos: IVec3::new(rx, ry, rz),
            state_id,
        });
    }

    let packets = encode_delta_op(
        epoch as u64,
        seq_id as u64,
        palette_revision as u32,
        changes,
    );

    let mut encoded_packets = Vec::with_capacity(packets.len());
    for pkt in &packets {
        encoded_packets.push(encode_packet(pkt));
    }

    match to_java_byte_array_array(&mut env, &encoded_packets) {
        Ok(arr) => arr,
        Err(_) => std::ptr::null_mut(),
    }
}
