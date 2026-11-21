//! Runtime texture integration: the provider (not `mtk-voxel`) owns the atlas, so the
//! skull/banner assertions live next to the implementation.

use std::collections::HashSet;

use glam::IVec3;
use mtk_block_entity_textures::BlockEntityTextures;
use mtk_voxel::{block_index, MesherConfig, TextureResolveMode, VoxelWorld};

/// Unique `(chunk_id, slot)` pairs currently allocated (pinned banner slots included).
fn allocated_slot_count(world: &VoxelWorld) -> usize {
    world
        .dynamic_entity_slots()
        .into_values()
        .collect::<HashSet<_>>()
        .len()
}

#[test]
fn test_player_skull_dynamic_atlas_meshing_and_uv_remapping() {
    let textures = BlockEntityTextures::default();
    let mut world = VoxelWorld::new(None, None, None);

    let hash = "5813943ca53225ee0ee814321cba1e420ea21f7c55f2998f9306cfad6655f69e";
    let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([128, 64, 32, 255]));
    let mut png_buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png_buf, image::ImageFormat::Png)
        .expect("PNG encoding should succeed");
    let png_bytes = png_buf.into_inner();

    textures.register_memory_skin(hash, png_bytes);
    world.set_block_entity_textures(Some(Box::new(textures)));

    let skull_entity = mtk_model::BlockEntityData::new_skull(
        mtk_model::skull::SkullKind::Player,
        Some(0),
        Some(mtk_model::skull::PlayerProfileData::new(
            Some("Steve".to_string()),
            Some("00000000-0000-0000-0000-000000000000".to_string()),
            Some(format!("http://textures.minecraft.net/texture/{}", hash)),
            None,
        )),
    );

    world.set_block(2, 2, 2, "minecraft:player_head[rotation=0]", None);
    let sec_coord = IVec3::new(0, 0, 0);
    let sec = world.storage.sections.get_mut(&sec_coord).unwrap();
    let local_idx = block_index(2, 2, 2) as u16;
    sec.set_block_entity(local_idx, Some(skull_entity));

    let mesh = world.rebuild_all().expect("Rebuild should succeed").clone();
    assert!(mesh.face_count() > 0);

    // Verify dynamic skin atlas allocated slot and blitted pixels
    // 16 pinned slots for vanilla banners + 1 player skull
    assert_eq!(allocated_slot_count(&world), 17);
    let atlas_rgba = world
        .dynamic_entity_atlas_rgba()
        .expect("Atlas RGBA must exist");
    assert_eq!(atlas_rgba.len(), 1024 * 1024 * 4);

    // Verify skull face material slot is 0xFFF0
    assert!(
        mesh.face_materials
            .contains(&mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID),
        "Mesh must contain dynamic entity material slot 0xFFF0"
    );

    // Verify skull UVs are remapped into allocated slot bounds (slot 16)
    let dynamic_slot = mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
    let (_, slot_idx) = *world.dynamic_entity_slots().get(hash).expect("Skull slot");
    assert_eq!(slot_idx, 16);
    let expected = mtk_texture::DynamicAtlasChunk::slot_info(slot_idx).uv_bounds;

    let mut dynamic_face_found = false;
    for (f_idx, &mat) in mesh.face_materials.iter().enumerate() {
        if mat == dynamic_slot {
            dynamic_face_found = true;
            for tri in 0..2 {
                let v_start = (f_idx * 2 + tri) * 3;
                for i in 0..3 {
                    let idx = mesh.indices[v_start + i] as usize;
                    let uv = mesh.uvs[idx];
                    assert!(
                        uv[0] >= expected[0] - 1e-4 && uv[0] <= expected[2] + 1e-4,
                        "U must be within slot {} bounds (got {})",
                        slot_idx,
                        uv[0]
                    );
                    assert!(
                        uv[1] >= expected[1] - 1e-4 && uv[1] <= expected[3] + 1e-4,
                        "V must be within slot {} bounds (got {})",
                        slot_idx,
                        uv[1]
                    );
                }
            }
        }
    }
    assert!(dynamic_face_found, "Must have checked dynamic face UVs");
}

#[test]
fn test_banner_dynamic_atlas_meshing_and_baking() {
    let textures = BlockEntityTextures::default();
    let mut world = VoxelWorld::new(None, None, None);
    world.set_block_entity_textures(Some(Box::new(textures)));

    // Place standing red banner (plain, no patterns)
    world.set_block(2, 2, 2, "minecraft:red_banner[rotation=0]", None);

    // Place wall banner with patterns
    let banner_entity = mtk_model::BlockEntityData::new_banner(
        "white",
        vec![("stripe_top", "blue"), ("flower", "red")],
    );
    world.set_block(2, 2, 3, "minecraft:white_wall_banner[facing=north]", None);
    let sec_coord = IVec3::new(0, 0, 0);
    let sec = world.storage.sections.get_mut(&sec_coord).unwrap();
    let local_idx = block_index(2, 2, 3) as u16;
    sec.set_block_entity(local_idx, Some(banner_entity.clone()));

    let mesh = world.rebuild_all().expect("Rebuild should succeed").clone();
    assert!(mesh.face_count() > 0);

    // 16 pinned slots for vanilla banners + 1 custom patterned banner = 17
    assert_eq!(allocated_slot_count(&world), 17);

    // Red banner slot is 14
    let red_slot = 14;
    let expected_red = mtk_texture::DynamicAtlasChunk::slot_info(red_slot).uv_bounds;

    // Patterned banner slot is 16
    let pat_key = "minecraft:banner/white+stripe_top#blue+flower#red";
    let (cid, pat_slot) = *world
        .dynamic_entity_slots()
        .get(pat_key)
        .expect("Patterned banner slot");
    assert_eq!(cid, 0xFFF0);
    assert_eq!(pat_slot, 16);
    let expected_pat = mtk_texture::DynamicAtlasChunk::slot_info(pat_slot).uv_bounds;

    let dynamic_chunk_id = mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
    let mut red_cloth_found = false;
    let mut pat_cloth_found = false;

    for f_idx in 0..mesh.face_count() {
        let mat = mesh.face_materials[f_idx];
        if mat == dynamic_chunk_id {
            let mut is_red = true;
            let mut is_pat = true;
            for i in 0..4 {
                let uv = mesh.uvs[f_idx * 4 + i];
                if uv[0] < expected_red[0] - 1e-4
                    || uv[0] > expected_red[2] + 1e-4
                    || uv[1] < expected_red[1] - 1e-4
                    || uv[1] > expected_red[3] + 1e-4
                {
                    is_red = false;
                }
                if uv[0] < expected_pat[0] - 1e-4
                    || uv[0] > expected_pat[2] + 1e-4
                    || uv[1] < expected_pat[1] - 1e-4
                    || uv[1] > expected_pat[3] + 1e-4
                {
                    is_pat = false;
                }
            }
            if is_red {
                red_cloth_found = true;
            }
            if is_pat {
                pat_cloth_found = true;
            }
        }
    }

    assert!(
        red_cloth_found,
        "Must have found red banner cloth in dynamic slot 14"
    );
    assert!(
        pat_cloth_found,
        "Must have found patterned banner cloth in dynamic slot 16"
    );
}

#[test]
fn test_plain_banner_without_block_entity_meshing() {
    let mut addr_map = mtk_texture::AtlasAddressMap::new();
    let banner_base_id =
        mtk_resource::ResourceLocation::parse("minecraft:entity/banner/banner_base").unwrap();
    let banner_base_loc = mtk_texture::AtlasSpriteLocation {
        chunk_id: 0,
        category: "banner_patterns".to_string(),
        is_animated: false,
        sprite_kind: mtk_texture::SpriteKind::StaticAtlas,
        texture_id: 0,
        uv_bounds: [0.0, 0.0, 0.5, 0.5],
        frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
        local_uv_bounds: [0.0, 0.0, 1.0, 1.0],
        frame_uv_step: [0.0, 0.0],
        pixel_rect: [0, 0, 64, 64],
        strip_pixel_rect: [0, 0, 64, 64],
        frame_size: [64, 64],
        frame_count: 1,
        animation: None,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        average_color: None,
        slot_index: None,
        ..Default::default()
    };
    addr_map.sprites.insert(banner_base_id, banner_base_loc);

    let config = MesherConfig {
        atlas_address_map: Some(std::sync::Arc::new(addr_map)),
        ..Default::default()
    };
    let mut world = VoxelWorld::new(Some(config), None, None);
    world.set_block_entity_textures(Some(Box::new(BlockEntityTextures::default())));

    // The provider owns the 16 pinned vanilla banner slots; the first prepare publishes them
    // into the world's atlas map even though no *new* sprite was added.
    world.prepare_block_entity_textures(None, TextureResolveMode::Cached, None);
    let map = world.config.atlas_address_map.as_ref().unwrap();
    assert!(map.lookup_str("minecraft:banner/red").is_some());
    assert!(map.lookup_str("minecraft:banner/blue").is_some());

    // Place a plain red banner and blue wall banner WITHOUT any block entities
    world.set_block(1, 1, 1, "minecraft:red_banner[rotation=4]", None);
    world.set_block(3, 1, 3, "minecraft:blue_wall_banner[facing=north]", None);

    let mesh = world.rebuild_all().expect("Rebuild should succeed").clone();
    assert!(mesh.face_count() > 0);

    let dynamic_chunk_id = mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID;
    let mut red_cloth_found = false;
    let mut blue_cloth_found = false;
    let expected_red = mtk_texture::DynamicAtlasChunk::slot_info(14).uv_bounds;
    let expected_blue = mtk_texture::DynamicAtlasChunk::slot_info(11).uv_bounds;

    for f_idx in 0..mesh.face_count() {
        let mat = mesh.face_materials[f_idx];
        if mat == dynamic_chunk_id {
            let mut is_red = true;
            let mut is_blue = true;
            for i in 0..4 {
                let uv = mesh.uvs[f_idx * 4 + i];
                if uv[0] < expected_red[0] - 1e-4
                    || uv[0] > expected_red[2] + 1e-4
                    || uv[1] < expected_red[1] - 1e-4
                    || uv[1] > expected_red[3] + 1e-4
                {
                    is_red = false;
                }
                if uv[0] < expected_blue[0] - 1e-4
                    || uv[0] > expected_blue[2] + 1e-4
                    || uv[1] < expected_blue[1] - 1e-4
                    || uv[1] > expected_blue[3] + 1e-4
                {
                    is_blue = false;
                }
            }
            if is_red {
                red_cloth_found = true;
            }
            if is_blue {
                blue_cloth_found = true;
            }
        }
    }

    assert!(
        red_cloth_found,
        "Plain standing red banner cloth must map to slot 14 on dynamic atlas"
    );
    assert!(
        blue_cloth_found,
        "Plain wall blue banner cloth must map to slot 11 on dynamic atlas"
    );
}

#[test]
fn test_allow_network_preserved_across_set_skin_cache_dir() {
    let temp_dir =
        std::env::temp_dir().join(format!("mtk_test_runtime_offline_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut textures = BlockEntityTextures::default();
    textures.set_allow_network(false);
    textures.set_skin_cache_dir(Some(temp_dir.clone()));

    let mut world = VoxelWorld::new(None, None, None);
    world.set_block_entity_textures(Some(Box::new(textures)));

    let uncached_entity = mtk_model::BlockEntityData::new_skull(
        mtk_model::skull::SkullKind::Player,
        Some(0),
        Some(mtk_model::skull::PlayerProfileData::new(
            Some("OfflineTest".to_string()),
            Some("00000000-0000-0000-0000-000000000000".to_string()),
            Some("http://127.0.0.1:59996/offline_test.png".to_string()),
            None,
        )),
    );
    world.set_block(1, 1, 1, "minecraft:player_head[rotation=0]", None);
    let sec_coord = IVec3::new(0, 0, 0);
    let sec = world.storage.sections.get_mut(&sec_coord).unwrap();
    sec.set_block_entity(block_index(1, 1, 1) as u16, Some(uncached_entity));

    // Blocking resolution must not allocate or fetch the uncached skull when allow_network=false.
    let added = world.ensure_dynamic_entities_resolved_sync();
    let _ = added;
    assert_eq!(allocated_slot_count(&world), 16);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
