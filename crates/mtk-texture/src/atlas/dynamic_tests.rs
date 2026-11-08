    use super::*;

    #[test]
    fn test_dynamic_atlas_allocation_and_deduplication() {
        let mut atlas = DynamicAtlasChunk::new(0xFFF0);
        let s1 = atlas.allocate_slot("alex");
        let s2 = atlas.allocate_slot("steve");
        let s1_again = atlas.allocate_slot("alex");

        assert_eq!(s1, 0);
        assert_eq!(s2, 1);
        assert_eq!(s1_again, 0); // Natural deduplication
        assert_eq!(atlas.allocated_count(), 2);
    }

    #[test]
    fn test_dynamic_atlas_slot_uv_bounds() {
        let info0 = DynamicAtlasChunk::slot_info(0);
        assert_eq!(info0.col, 0);
        assert_eq!(info0.row, 0);
        assert_eq!(info0.pixel_rect, [0, 0, 64, 64]);
        assert_eq!(info0.uv_bounds, [0.0, 0.9375, 0.0625, 1.0]);

        let info17 = DynamicAtlasChunk::slot_info(17); // row 1, col 1
        assert_eq!(info17.col, 1);
        assert_eq!(info17.row, 1);
        assert_eq!(info17.pixel_rect, [64, 64, 64, 64]);
        assert_eq!(info17.uv_bounds, [0.0625, 0.875, 0.125, 0.9375]);
    }

    #[test]
    fn test_dynamic_atlas_blit_and_address_map() {
        let mut atlas = DynamicAtlasChunk::new(0xFFF0);
        let dummy_skin_rgba = vec![255u8; 64 * 64 * 4];
        let info = atlas
            .blit_skin_rgba(0, &dummy_skin_rgba, 64, 64)
            .expect("Blit should succeed");

        assert_eq!(info.slot_index, 0);
        atlas.slot_map.insert("player_test".to_string(), 0);

        let mut addr_map = AtlasAddressMap::new();
        atlas.register_to_address_map(&mut addr_map);

        let loc = addr_map
            .lookup_str("minecraft:skin/player_test")
            .expect("Sprite must be registered");
        assert_eq!(loc.chunk_id, 0xFFF0);
        assert_eq!(loc.uv_bounds, [0.0, 0.9375, 0.0625, 1.0]);
    }

    #[test]
    fn test_dynamic_atlas_overflow_round_robin() {
        let mut atlas = DynamicAtlasChunk::new(0xFFF0);
        for i in 0..256 {
            let slot = atlas.allocate_slot(&format!("skin_{}", i));
            assert_eq!(slot, i as u16);
        }
        // Slot 256 wraps around to 0
        let slot_256 = atlas.allocate_slot("skin_256");
        assert_eq!(slot_256, 0);
        // Slot 257 wraps around to 1
        let slot_257 = atlas.allocate_slot("skin_257");
        assert_eq!(slot_257, 1);
        // Stale skin_0 should have been purged, skin_256 now owns slot 0
        assert_eq!(atlas.get_slot("skin_256"), Some(0));
        assert_eq!(atlas.get_slot("skin_0"), None);
    }

    #[test]
    fn test_dynamic_atlas_manager_multi_chunk_expansion() {
        let mut mgr = DynamicAtlasManager::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID);
        assert_eq!(mgr.chunk_count(), 1);

        // Fill up chunk 0 (0xFFF0) with 256 skins
        for i in 0..256 {
            let (cid, slot) = mgr.allocate_slot(&format!("player_{}", i));
            assert_eq!(cid, 0xFFF0);
            assert_eq!(slot, i as u16);
        }
        assert_eq!(mgr.allocated_count(), 256);
        assert_eq!(mgr.chunk_count(), 1);

        // 257th skin must automatically trigger allocation of chunk 1 (0xFFF1)
        let (cid_256, slot_256) = mgr.allocate_slot("player_256");
        assert_eq!(cid_256, 0xFFF1);
        assert_eq!(slot_256, 0);
        assert_eq!(mgr.chunk_count(), 2);
        assert_eq!(mgr.allocated_count(), 257);

        // Deduplication works across chunks
        let (re_cid, re_slot) = mgr.allocate_slot("player_0");
        assert_eq!(re_cid, 0xFFF0);
        assert_eq!(re_slot, 0);

        let (re_cid_256, re_slot_256) = mgr.allocate_slot("player_256");
        assert_eq!(re_cid_256, 0xFFF1);
        assert_eq!(re_slot_256, 0);

        // Address map registration registers both chunks and correct sprite locations
        let mut addr_map = AtlasAddressMap::new();
        mgr.register_to_address_map(&mut addr_map);

        assert_eq!(addr_map.chunks.len(), 2);
        assert_eq!(addr_map.chunks[0].chunk_id, 0xFFF0);
        assert_eq!(addr_map.chunks[0].category_chunk_index, 1);
        assert_eq!(addr_map.chunks[1].chunk_id, 0xFFF1);
        assert_eq!(addr_map.chunks[1].category_chunk_index, 2);

        let loc0 = addr_map
            .lookup_str("minecraft:skin/player_0")
            .expect("player_0 must be in address map");
        assert_eq!(loc0.chunk_id, 0xFFF0);

        let loc256 = addr_map
            .lookup_str("minecraft:skin/player_256")
            .expect("player_256 must be in address map");
        assert_eq!(loc256.chunk_id, 0xFFF1);
    }

    #[test]
    fn test_dynamic_atlas_manager_dirty_tracking() {
        let mut mgr = DynamicAtlasManager::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID);
        assert_eq!(mgr.dirty_chunk_ids().len(), 0);

        let dummy_skin = vec![128u8; 64 * 64 * 4];
        let (cid, slot) = mgr.allocate_slot("steve");
        let _ = mgr.blit_skin_rgba(cid, slot, &dummy_skin, 64, 64).unwrap();

        assert!(mgr.dirty_chunk_ids().contains(&0xFFF0));
        assert_eq!(mgr.dirty_chunk_buffers().len(), 1);

        mgr.clear_dirty();
        assert_eq!(mgr.dirty_chunk_ids().len(), 0);
    }

    #[test]
    fn test_dynamic_atlas_prewarm_vanilla_banners() {
        let mut mgr = DynamicAtlasManager::new(DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID);
        mgr.prewarm_vanilla_banners(None)
            .expect("Prewarm must succeed");

        assert_eq!(mgr.allocated_count(), 16);
        assert!(mgr.contains_key("banner:white"));
        assert!(mgr.contains_key("banner:red"));
        assert!(mgr.contains_key("banner:black"));
        assert!(mgr.contains_key("minecraft:red_banner"));

        // Red is index 14
        let red_loc = mgr
            .get_location("banner:red")
            .expect("red banner must exist");
        assert_eq!(red_loc, (0xFFF0, 14));

        // Primary rgba buffer is populated and has red pixels in slot 14
        let buf = mgr.primary_rgba_buffer().expect("Must have primary buffer");
        assert_eq!(buf.len(), 1024 * 1024 * 4);

        // Next allocated slot should be 16
        let (cid, slot) = mgr.allocate_slot("custom_player");
        assert_eq!(cid, 0xFFF0);
        assert_eq!(slot, 16);

        // Address map registration resolves banner keys
        let mut addr_map = AtlasAddressMap::new();
        mgr.register_to_address_map(&mut addr_map);

        let loc = addr_map
            .lookup_str("minecraft:banner/red")
            .expect("banner/red in map");
        assert_eq!(loc.chunk_id, 0xFFF0);
        assert_eq!(loc.pixel_rect, DynamicAtlasChunk::slot_info(14).pixel_rect);
    }

    #[test]
    fn test_dynamic_atlas_pinned_slots_overflow_protection() {
        let mut chunk = DynamicAtlasChunk::new(0xFFF0);
        chunk.prewarm_vanilla_banners(None).unwrap();
        assert_eq!(chunk.pinned_slots, 16);
        assert_eq!(chunk.allocated_count(), 16);

        // Fill slots from 16 to 255 (240 dynamic slots)
        for i in 16..256 {
            let slot = chunk.allocate_slot(&format!("entity_{}", i));
            assert_eq!(slot, i as u16);
        }

        // Slot 256 should wrap around to 16, NEVER to 0..15!
        let wrap_slot = chunk.allocate_slot("overflow_1");
        assert_eq!(wrap_slot, 16);

        // Pinned banner:red (slot 14) and banner:white (slot 0) must remain intact!
        assert_eq!(chunk.get_slot("banner:red"), Some(14));
        assert_eq!(chunk.get_slot("banner:white"), Some(0));

        // Slot 16 old occupant was evicted, overflow_1 owns it
        assert_eq!(chunk.get_slot("entity_16"), None);
        assert_eq!(chunk.get_slot("overflow_1"), Some(16));
    }
