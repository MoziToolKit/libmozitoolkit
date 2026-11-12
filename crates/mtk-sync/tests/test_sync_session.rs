use glam::IVec3;
use mtk_sync::protocol::*;
use mtk_sync::*;
use mtk_voxel::MesherConfig;
use std::io::Write;

#[test]
fn test_sync_session_initialization() {
    let mut session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);
    let events = session.poll_events();
    assert!(events.is_empty());
    assert_eq!(session.client_generation(), 1);
    assert_eq!(session.status(), "DISCONNECTED");

    // Inherits canonical supported entity schema from mtk-model by default
    let default_subs = session.subscribed_entities();
    assert!(default_subs.contains(&"minecraft:painting".to_string()));
    assert!(default_subs.contains(&"minecraft:item_frame".to_string()));
    assert!(default_subs.contains(&"minecraft:glow_item_frame".to_string()));
    assert!(default_subs.contains(&"minecraft:armor_stand".to_string()));

    // Custom subscriptions
    session.set_subscribed_entities(Some(vec!["minecraft:painting".to_string()]));
    assert_eq!(
        session.subscribed_entities(),
        vec!["minecraft:painting".to_string()]
    );

    // Resetting to None restores mtk-model defaults
    session.set_subscribed_entities(None);
    assert_eq!(session.subscribed_entities(), default_subs);
}

#[test]
fn test_handshake_welcome_verified_skip_rebuild() {
    let session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);

    let welcome = Packet::Welcome {
        negotiated_version: 0x0003,
        server_uuid: [1u8; 16],
        epoch: 10,
        server_seq: 150,
        current_min: IVec3::new(0, 64, 0),
        current_size: IVec3::new(16, 16, 16),
        current_state_hash: 0x12345678,
        decision: SyncDecision::VerifiedSkipRebuild,
        message_or_dimension: "minecraft:overworld".to_string(),
    };

    session.process_packet_direct(welcome);

    assert_eq!(session.current_epoch(), 10);
    assert_eq!(session.current_sequence(), 150);
    assert_eq!(session.lifecycle(), SyncLifecycle::Live);

    let events = session.poll_events();
    assert!(events.iter().any(|e| matches!(
        e,
        SyncEvent::Verified {
            is_verified: true,
            ..
        }
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, SyncEvent::Handshake { sequence: 150, .. })));
}

#[test]
fn test_baseline_assembly_and_meshing() {
    let session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);

    let min_pos = IVec3::new(0, 0, 0);
    let size = IVec3::new(2, 2, 2);
    let total_voxels = 8;
    let block_palette = vec!["minecraft:air".to_string(), "minecraft:stone".to_string()];
    let biome_palette = vec!["minecraft:plains".to_string()];

    // Single voxel of stone at (0, 0, 0), rest air
    let mut raw_voxels = vec![0u8; total_voxels];
    raw_voxels[0] = 1; // stone

    // Compress using Zstd
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    encoder.write_all(&raw_voxels).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut chunk_hasher = xxhash_rust::xxh64::Xxh64::new(0);
    chunk_hasher.update(&compressed);
    let chunk_hash = chunk_hasher.digest();

    let expected_state_hash = compute_sparse_state_hash(
        min_pos,
        size,
        "minecraft:overworld",
        [SparseBlockEntry {
            rel_x: 0,
            rel_y: 0,
            rel_z: 0,
            state: "minecraft:stone",
            biome: "minecraft:plains",
            entity_json: None,
        }],
    );

    // 1. Send Welcome -> FullBaselineRequired
    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [2u8; 16],
        epoch: 1,
        server_seq: 10,
        current_min: min_pos,
        current_size: size,
        current_state_hash: expected_state_hash,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: "minecraft:overworld".to_string(),
    });
    assert_eq!(session.lifecycle(), SyncLifecycle::ReceivingBaseline);

    // 2. BaselineBegin
    session.process_packet_direct(Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [3u8; 16],
        snapshot_seq: 10,
        min_pos,
        size,
        state_hash: expected_state_hash,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: compressed.len() as u64,
        total_decoded_bytes: 8,
        compression_mode: 1,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette,
        biome_palette,
    });

    // 3. BaselineChunk
    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [3u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: compressed.len() as u32,
        decoded_target_bytes: 8,
        chunk_xxhash64: chunk_hash,
        payload: compressed,
    });

    // 4. BaselineEnd
    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [3u8; 16],
        snapshot_seq: 10,
        total_chunks_sent: 1,
        final_state_hash: expected_state_hash,
        status: 0,
    });

    // State machine should transition to Live and emit WorldMeshReady
    assert_eq!(session.lifecycle(), SyncLifecycle::Live);
    let events = session.poll_events();
    let has_mesh = events
        .iter()
        .any(|e| matches!(e, SyncEvent::WorldMeshReady { .. }));
    assert!(
        has_mesh,
        "WorldMeshReady must be emitted upon successful baseline assembly"
    );
}

#[test]
fn test_delta_staging_and_replay_after_baseline() {
    let session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);

    let min_pos = IVec3::new(0, 0, 0);
    let size = IVec3::new(2, 2, 2);
    let raw_voxels = vec![0u8; 8];

    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    encoder.write_all(&raw_voxels).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut chunk_hasher = xxhash_rust::xxh64::Xxh64::new(0);
    chunk_hasher.update(&compressed);
    let chunk_hash = chunk_hasher.digest();

    let expected_state_hash = compute_sparse_state_hash(
        min_pos,
        size,
        "minecraft:overworld",
        std::iter::empty::<SparseBlockEntry>(),
    );

    // Welcome
    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [2u8; 16],
        epoch: 1,
        server_seq: 20,
        current_min: min_pos,
        current_size: size,
        current_state_hash: expected_state_hash,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: "minecraft:overworld".to_string(),
    });

    // BaselineBegin
    session.process_packet_direct(Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [5u8; 16],
        snapshot_seq: 20,
        min_pos,
        size,
        state_hash: expected_state_hash,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: compressed.len() as u64,
        total_decoded_bytes: 8,
        compression_mode: 1,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string(), "minecraft:dirt".to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    });

    // In-flight Delta arriving during baseline download (seq = 21)
    session.process_packet_direct(Packet::DeltaOp {
        epoch: 1,
        seq_id: 21,
        palette_revision: 1,
        count: 1,
        batch_xxhash64: compute_delta_batch_hash(&[DeltaChangeEntry {
            rel_pos: IVec3::new(1, 1, 1),
            state_id: 1, // dirt
        }]),
        changes: vec![DeltaChangeEntry {
            rel_pos: IVec3::new(1, 1, 1),
            state_id: 1,
        }],
    });

    // BaselineChunk
    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [5u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: compressed.len() as u32,
        decoded_target_bytes: 8,
        chunk_xxhash64: chunk_hash,
        payload: compressed,
    });

    // BaselineEnd
    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [5u8; 16],
        snapshot_seq: 20,
        total_chunks_sent: 1,
        final_state_hash: expected_state_hash,
        status: 0,
    });

    assert_eq!(session.lifecycle(), SyncLifecycle::Live);
    assert_eq!(session.current_sequence(), 21);

    // Verify dirt was applied from replayed delta!
    let blk = session.world.read().unwrap().get_block(1, 1, 1).to_string();
    assert_eq!(blk, "minecraft:dirt");
}

#[test]
fn test_baseline_and_delta_entity_integration() {
    let session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);

    let min_pos = IVec3::new(0, 0, 0);
    let size = IVec3::new(2, 2, 2);
    let raw_voxels = vec![1u8; 8]; // all decorated pots (state 1)

    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    encoder.write_all(&raw_voxels).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut chunk_hasher = xxhash_rust::xxh64::Xxh64::new(0);
    chunk_hasher.update(&compressed);
    let chunk_hash = chunk_hasher.digest();

    let pot_state = "minecraft:decorated_pot[facing=north]";
    let pot_data = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [
            Some("minecraft:archer_pottery_sherd".to_string()),
            Some("minecraft:prize_pottery_sherd".to_string()),
            None,
            None,
        ],
    };
    let mut expected_storage = mtk_voxel::VoxelStorage::new();
    for y in 0..2 {
        for z in 0..2 {
            for x in 0..2 {
                expected_storage.set_block(x, y, z, pot_state, Some("minecraft:plains"));
            }
        }
    }
    let sec = expected_storage
        .sections
        .get_mut(&IVec3::new(0, 0, 0))
        .unwrap();
    sec.set_block_entity(0, Some(pot_data.clone()));
    let expected_state_hash = compute_sparse_state_hash_for_storage(
        &expected_storage,
        min_pos,
        size,
        "minecraft:overworld",
    );

    // 1. Welcome
    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [6u8; 16],
        epoch: 1,
        server_seq: 10,
        current_min: min_pos,
        current_size: size,
        current_state_hash: expected_state_hash,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: "minecraft:overworld".to_string(),
    });

    // 2. BaselineBegin
    session.process_packet_direct(Packet::BaselineBegin {
        epoch: 1,
        snapshot_uuid: [7u8; 16],
        snapshot_seq: 10,
        min_pos,
        size,
        state_hash: expected_state_hash,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: compressed.len() as u64,
        total_decoded_bytes: 8,
        compression_mode: 1,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string(), pot_state.to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    });

    // 3. BaselineEntities
    session.process_packet_direct(Packet::BaselineEntities {
        epoch: 1,
        snapshot_uuid: [7u8; 16],
        entities: vec![(IVec3::new(0, 0, 0), pot_data.clone())],
    });

    // 4. BaselineChunk
    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 1,
        snapshot_uuid: [7u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: compressed.len() as u32,
        decoded_target_bytes: 8,
        chunk_xxhash64: chunk_hash,
        payload: compressed,
    });

    // 5. BaselineEnd
    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 1,
        snapshot_uuid: [7u8; 16],
        snapshot_seq: 10,
        total_chunks_sent: 1,
        final_state_hash: expected_state_hash,
        status: 0,
    });

    assert_eq!(session.lifecycle(), SyncLifecycle::Live);

    // Verify baseline entity is loaded in section (0, 0, 0)
    {
        let world = session.world.read().unwrap();
        let sec = world
            .storage
            .sections
            .get(&IVec3::new(0, 0, 0))
            .expect("Section (0,0,0) must exist");
        let entity = sec
            .get_block_entity(0)
            .expect("Entity at local 0 must exist");
        assert_eq!(*entity, pot_data);
    }

    // 6. Send DeltaEntity for (1, 1, 1)
    let new_pot_data = mtk_model::BlockEntityData::DecoratedPot {
        sherds: [
            Some("minecraft:miner_pottery_sherd".to_string()),
            None,
            None,
            None,
        ],
    };
    session.process_packet_direct(Packet::DeltaEntity {
        epoch: 1,
        seq_id: 11,
        rel_pos: IVec3::new(1, 1, 1),
        entity: Some(new_pot_data.clone()),
    });

    // Verify DeltaEntity updated section
    {
        let world = session.world.read().unwrap();
        let sec = world
            .storage
            .sections
            .get(&IVec3::new(0, 0, 0))
            .expect("Section (0,0,0) must exist");
        let local_1_1_1 = 1 | (1 << 8) | (1 << 4); // x + y*256 + z*16
        let entity = sec
            .get_block_entity(local_1_1_1)
            .expect("Entity at (1,1,1) must exist");
        assert_eq!(*entity, new_pot_data);
    }

    // 7. Send DeltaEntity with None to delete entity at (0, 0, 0)
    session.process_packet_direct(Packet::DeltaEntity {
        epoch: 1,
        seq_id: 12,
        rel_pos: IVec3::new(0, 0, 0),
        entity: None,
    });

    {
        let world = session.world.read().unwrap();
        let sec = world
            .storage
            .sections
            .get(&IVec3::new(0, 0, 0))
            .expect("Section (0,0,0) must exist");
        assert_eq!(sec.get_block_entity(0), None);
    }
}

#[test]
fn test_baseline_and_delta_visual_entity_integration() {
    let session = LiveSyncSession::new(Some(MesherConfig::default()), None, None);

    let min_pos = IVec3::new(0, 0, 0);
    let size = IVec3::new(2, 2, 2);
    let raw_voxels = vec![1u8; 8];

    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    encoder.write_all(&raw_voxels).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut chunk_hasher = xxhash_rust::xxh64::Xxh64::new(0);
    chunk_hasher.update(&compressed);
    let chunk_hash = chunk_hasher.digest();

    let stone_state = "minecraft:stone";
    let mut expected_storage = mtk_voxel::VoxelStorage::new();
    for y in 0..2 {
        for z in 0..2 {
            for x in 0..2 {
                expected_storage.set_block(x, y, z, stone_state, Some("minecraft:plains"));
            }
        }
    }
    let expected_state_hash = compute_sparse_state_hash_for_storage(
        &expected_storage,
        min_pos,
        size,
        "minecraft:overworld",
    );

    let painting = mtk_model::EntityInstance::new_painting(
        Some("11111111-2222-3333-4444-555555555555".to_string()),
        glam::DVec3::new(1.0, 64.0, 2.0),
        mtk_core::Direction::North,
        glam::Vec3::ZERO,
        "kebab",
        1,
        1,
        Some([1, 64, 2]),
    );

    let item_frame = mtk_model::EntityInstance::new_item_frame(
        Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string()),
        glam::DVec3::new(2.0, 65.0, 3.0),
        mtk_core::Direction::South,
        glam::Vec3::ZERO,
        "minecraft:diamond_sword",
        1,
        0,
        false,
        false,
        None,
    );

    let initial_entities = vec![painting.clone(), item_frame.clone()];
    let entities_hash = mtk_sync::protocol::compute_visual_entities_hash(&initial_entities);

    // 1. Welcome
    session.process_packet_direct(Packet::Welcome {
        negotiated_version: 3,
        server_uuid: [8u8; 16],
        epoch: 2,
        server_seq: 20,
        current_min: min_pos,
        current_size: size,
        current_state_hash: expected_state_hash,
        decision: SyncDecision::FullBaselineRequired,
        message_or_dimension: "minecraft:overworld".to_string(),
    });

    // 2. BaselineBegin
    session.process_packet_direct(Packet::BaselineBegin {
        epoch: 2,
        snapshot_uuid: [9u8; 16],
        snapshot_seq: 20,
        min_pos,
        size,
        state_hash: expected_state_hash,
        palette_revision: 1,
        total_chunks: 1,
        total_encoded_bytes: compressed.len() as u64,
        total_decoded_bytes: 8,
        compression_mode: 1,
        palette_bits_per_block: 8,
        biome_bits_per_quart: 8,
        dimension: "minecraft:overworld".to_string(),
        block_palette: vec!["minecraft:air".to_string(), stone_state.to_string()],
        biome_palette: vec!["minecraft:plains".to_string()],
    });

    // 3. BaselineVisualEntities
    session.process_packet_direct(Packet::BaselineVisualEntities {
        epoch: 2,
        snapshot_seq: 20,
        entities_hash,
        entities: initial_entities.clone(),
    });

    // 4. BaselineChunk
    session.process_packet_direct(Packet::BaselineChunk {
        epoch: 2,
        snapshot_uuid: [9u8; 16],
        chunk_index: 0,
        total_chunks: 1,
        first_voxel_index: 0,
        voxel_count: 8,
        payload_len: compressed.len() as u32,
        decoded_target_bytes: 8,
        chunk_xxhash64: chunk_hash,
        payload: compressed,
    });

    // 5. BaselineEnd
    session.process_packet_direct(Packet::BaselineEnd {
        epoch: 2,
        snapshot_uuid: [9u8; 16],
        snapshot_seq: 20,
        total_chunks_sent: 1,
        final_state_hash: expected_state_hash,
        status: 0,
    });

    assert_eq!(session.lifecycle(), SyncLifecycle::Live);

    // Verify baseline visual entities are present in storage
    {
        let world = session.world.read().unwrap();
        assert_eq!(world.storage.entities.len(), 2);
        assert_eq!(world.storage.entities[0].uuid, painting.uuid);
        assert_eq!(world.storage.entities[1].uuid, item_frame.uuid);
    }

    // 6. Test DeltaVisualEntity UPSERT (add new entity)
    let armor_stand = mtk_model::EntityInstance {
        uuid: Some("ffffffff-ffff-ffff-ffff-ffffffffffff".to_string()),
        id: "minecraft:armor_stand".to_string(),
        custom_name: Some("Stand1".to_string()),
        position: glam::DVec3::new(5.0, 64.0, 5.0),
        facing: Some(mtk_core::Direction::South),
        rotation: glam::Vec3::ZERO,
        scale: glam::Vec3::ONE,
        kind: mtk_model::EntityKindData::Generic {
            custom_id: "minecraft:armor_stand".to_string(),
            payload: Vec::new(),
        },
    };

    session.process_packet_direct(Packet::DeltaVisualEntity {
        epoch: 2,
        seq_id: 21,
        delta: VisualEntityDelta::Upsert(armor_stand.clone()),
    });

    {
        let world = session.world.read().unwrap();
        assert_eq!(world.storage.entities.len(), 3);
        assert!(world
            .storage
            .entities
            .iter()
            .any(|e| e.uuid.as_deref() == Some("ffffffff-ffff-ffff-ffff-ffffffffffff")));
    }

    // Check event fired
    let events = session.poll_events();
    let visual_updated = events.iter().any(|ev| matches!(ev, SyncEvent::VisualEntitiesUpdated { upserted, .. } if upserted.len() == 1 && upserted[0].uuid == armor_stand.uuid));
    assert!(
        visual_updated,
        "SyncEvent::VisualEntitiesUpdated should be fired on DeltaVisualEntity UPSERT"
    );

    // 7. Test DeltaVisualEntity REMOVE
    session.process_packet_direct(Packet::DeltaVisualEntity {
        epoch: 2,
        seq_id: 22,
        delta: VisualEntityDelta::Remove {
            uuid: painting.uuid.clone().unwrap(),
        },
    });

    {
        let world = session.world.read().unwrap();
        assert_eq!(world.storage.entities.len(), 2);
        assert!(!world
            .storage
            .entities
            .iter()
            .any(|e| e.uuid.as_deref() == Some("11111111-2222-3333-4444-555555555555")));
    }

    let events = session.poll_events();
    let visual_removed = events.iter().any(|ev| matches!(ev, SyncEvent::VisualEntitiesUpdated { deleted_uuids, .. } if deleted_uuids.contains(&"11111111-2222-3333-4444-555555555555".to_string())));
    assert!(
        visual_removed,
        "SyncEvent::VisualEntitiesUpdated should report deleted uuid on DeltaVisualEntity REMOVE"
    );
}

#[test]
fn test_wss_url_tls_support() {
    // Attempting to connect to a non-existent port with wss:// must proceed to network I/O
    // (i.e. Io Error like ConnectionRefused) rather than failing immediately with
    // `Url("wss scheme is not supported without a TLS feature enabled")`.
    let err = tungstenite::connect("wss://127.0.0.1:59997").unwrap_err();
    match err {
        tungstenite::Error::Io(_) => {
            // Expected: TLS stack initialized and attempted TCP connect!
        }
        other => {
            let msg = other.to_string();
            assert!(
                !msg.contains("not supported without a TLS feature enabled"),
                "Tungstenite should support wss with rustls feature, got: {msg}"
            );
        }
    }
}
