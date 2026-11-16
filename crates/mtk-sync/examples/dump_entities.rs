use mtk_sync::{LiveSyncSession, SyncEvent};
use std::time::{Duration, Instant};

fn main() {
    println!("Connecting to Live Sync on ws://127.0.0.1:8765 ...");
    let mut session = LiveSyncSession::new(None, None, None);
    if let Err(e) = session.start("ws://127.0.0.1:8765", false, 0) {
        eprintln!("Failed to start session: {}", e);
        return;
    }

    let start = Instant::now();
    let mut received_any_entities = false;
    while start.elapsed() < Duration::from_secs(5) {
        let events = session.poll_events();
        for ev in events {
            match ev {
                SyncEvent::StatusChange(s) => println!("[StatusChange] {}", s),
                SyncEvent::SelectionUpdated { min_pos, size } => {
                    println!("[SelectionUpdated] min: {:?}, size: {:?}", min_pos, size);
                }
                SyncEvent::Handshake {
                    decision,
                    dimension,
                    ..
                } => {
                    println!(
                        "[Handshake] decision: {}, dimension: {}",
                        decision, dimension
                    );
                }
                SyncEvent::VisualEntitiesUpdated {
                    upserted,
                    deleted_uuids,
                } => {
                    println!(
                        "[VisualEntitiesUpdated] upserted: {}, deleted: {}",
                        upserted.len(),
                        deleted_uuids.len()
                    );
                    for ent in &upserted {
                        println!("--------------------------------------------------");
                        println!("Entity ID: {}", ent.id);
                        println!("UUID: {:?}", ent.uuid);
                        println!("Position: {:?}", ent.position);
                        println!("Facing: {:?}", ent.facing);
                        println!("Rotation (pitch, yaw, roll) [rad]: {:?}", ent.rotation);
                        println!(
                            "Rotation [deg]: ({:.1}, {:.1}, {:.1})",
                            ent.rotation.x.to_degrees(),
                            ent.rotation.y.to_degrees(),
                            ent.rotation.z.to_degrees()
                        );
                        match &ent.kind {
                            mtk_model::EntityKindData::ItemFrame(frame) => {
                                println!("ItemFrameData:");
                                println!("  item_id: {}", frame.item_id);
                                println!("  count: {}", frame.count);
                                println!("  rotation (0..7): {}", frame.rotation);
                                println!("  is_glowing: {}", frame.is_glowing);
                                println!("  is_invisible: {}", frame.is_invisible);
                                println!("  map_id: {:?}", frame.map_id);
                            }
                            other => println!("Kind: {:?}", other),
                        }
                    }
                    received_any_entities = true;
                }
                SyncEvent::WorldMeshReady { .. } => {
                    println!("[WorldMeshReady]");
                }
                SyncEvent::Error(err) => {
                    println!("[Error] {:?}", err);
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    if !received_any_entities {
        println!("No visual entities received in events. Checking storage:");
        let storage = session.get_storage();
        println!("Storage entities count: {}", storage.entities.len());
        for ent in &storage.entities {
            println!(
                "Storage Entity: {} at {:?}, kind: {:?}",
                ent.id, ent.position, ent.kind
            );
        }
    }
}
