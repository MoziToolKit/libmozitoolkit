//! # JNI Server Event Polling
//!
//! Maps Rust `ServerEvent` variants into Java `NativeSync.ServerEvent` objects.

use jni::objects::{JClass, JValue};
use jni::sys::{jlong, jobjectArray};
use jni::JNIEnv;

use mtk_sync::server::ServerEvent;

use crate::server::SERVERS;

fn poll_server_events(handle: jlong) -> Option<Vec<ServerEvent>> {
    if handle <= 0 {
        return None;
    }
    SERVERS
        .get(handle as u64)
        .map(|server| server.poll_events())
}

/// Polls pending server events and returns an array of NativeSync.ServerEvent.
#[no_mangle]
pub extern "system" fn Java_com_mozi1924_yefira_encoder_NativeSync_nativeServerPollEvents(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jobjectArray {
    let Some(events) = poll_server_events(handle) else {
        return std::ptr::null_mut();
    };

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
            ServerEvent::Warning(message) => (5i32, 0i64, 0i64, 0i64, message.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_sync::server::{LiveSyncServer, ServerConfig};
    use std::time::Duration;

    #[test]
    fn poll_server_events_rejects_invalid_and_retired_handles() {
        assert!(poll_server_events(0).is_none());
        assert!(poll_server_events(-1).is_none());

        let config = ServerConfig {
            bind_address: "127.0.0.1:0".to_string(),
            auth_token: None,
            allow_loopback_without_token: true,
            server_uuid: [0u8; 16],
            max_connections: 8,
            handshake_timeout: Duration::from_millis(5000),
            ping_interval: Duration::from_millis(5000),
        };
        let server = LiveSyncServer::start(config).expect("server must bind 127.0.0.1:0");
        let handle = SERVERS.insert(server) as jlong;

        let events = poll_server_events(handle);
        assert!(events.is_some(), "active handle must poll events safely");

        if let Some(removed) = SERVERS.remove(handle as u64) {
            removed.stop();
        }
        assert!(
            poll_server_events(handle).is_none(),
            "retired handle must return None instead of dereferencing freed memory"
        );
    }
}
