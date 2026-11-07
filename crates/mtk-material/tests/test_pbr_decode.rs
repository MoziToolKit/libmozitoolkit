//! Golden tests for the LabPBR 1.3 -> MtkPackedV1 decoder.

use mtk_material::decode_baked_atlas;
use mtk_texture::{
    AtlasAddressMap, AtlasChunkMeta, BakedAtlas, BakedAtlasChunk, PbrLayout, PbrSourceFormat,
    RgbaBuffer,
};

fn buffer(width: u32, height: u32, pixels: &[[u8; 4]]) -> RgbaBuffer {
    assert_eq!(pixels.len() as u32, width * height);
    let mut buffer = RgbaBuffer::new(width, height);
    for (index, pixel) in pixels.iter().enumerate() {
        buffer.set_pixel(index as u32 % width, index as u32 / width, *pixel);
    }
    buffer
}

#[test]
fn decode_baked_atlas_updates_mapping_metadata() {
    let normal = buffer(2, 1, &[[128, 128, 255, 255], [255, 0, 0, 0]]);
    let specular = buffer(2, 1, &[[0, 0, 0, 0], [255, 230, 65, 254]]);
    let mut address_map = AtlasAddressMap::new();
    address_map.chunks.push(AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 2,
        height: 1,
        has_normal: true,
        has_specular: true,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    let mut atlas = BakedAtlas {
        chunks: vec![BakedAtlasChunk {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 2,
            height: 1,
            albedo: RgbaBuffer::new(2, 1),
            normal: Some(normal),
            specular: Some(specular),
            overlay: None,
            normal_gl: None,
            orm: None,
            aux: None,
        }],
        address_map,
    };

    let summary = decode_baked_atlas(&mut atlas).expect("decode atlas");
    assert_eq!(summary.chunks, 1);
    assert_eq!(summary.with_normal, 1);
    assert_eq!(summary.with_orm, 1);
    assert_eq!(summary.with_aux, 1);

    let chunk = &atlas.chunks[0];
    // Legacy clean-albedo behavior: transparent pixels get white RGB.
    assert_eq!(chunk.albedo.get_pixel(0, 0), [255, 255, 255, 0]);
    assert!(chunk.normal.is_none());
    assert!(chunk.specular.is_none());
    assert!(chunk.normal_gl.is_some());
    assert!(chunk.orm.is_some());
    assert!(chunk.aux.is_some());

    let meta = &atlas.address_map.chunks[0];
    assert!(meta.has_orm);
    assert!(meta.has_aux);
    let pbr = meta.pbr.as_ref().expect("pbr meta");
    assert_eq!(pbr.layout, PbrLayout::MtkPackedV1);
    assert_eq!(pbr.source_format, PbrSourceFormat::LabPbr13);
    assert_eq!(pbr.normal_convention, mtk_texture::NormalConvention::Opengl);
    assert_eq!(pbr.roughness_space, mtk_texture::RoughnessSpace::Perceptual);
    assert_eq!(pbr.height_scale, 0.25);
}

#[test]
fn decode_baked_atlas_without_companions_leaves_metadata_empty() {
    let mut address_map = AtlasAddressMap::new();
    address_map.chunks.push(AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 1,
        height: 1,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });
    let mut atlas = BakedAtlas {
        chunks: vec![BakedAtlasChunk {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 1,
            height: 1,
            albedo: RgbaBuffer::new(1, 1),
            normal: None,
            specular: None,
            overlay: None,
            normal_gl: None,
            orm: None,
            aux: None,
        }],
        address_map,
    };

    let summary = decode_baked_atlas(&mut atlas).expect("decode atlas");
    assert_eq!(summary.chunks, 1);
    assert_eq!(summary.with_orm, 0);
    assert_eq!(summary.with_aux, 0);
    assert_eq!(atlas.chunks[0].albedo.get_pixel(0, 0), [255, 255, 255, 0]);
    assert!(!atlas.address_map.chunks[0].has_orm);
    assert!(!atlas.address_map.chunks[0].has_aux);
    assert!(atlas.address_map.chunks[0].pbr.is_none());
}

#[test]
fn decode_baked_atlas_injects_preset_transmission_and_ior_for_water() {
    use mtk_resource::ResourceLocation;
    use mtk_texture::AtlasSpriteLocation;

    let mut address_map = AtlasAddressMap::new();
    address_map.chunks.push(AtlasChunkMeta {
        chunk_id: 1,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 16,
        height: 16,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    let water_loc = ResourceLocation::parse("minecraft:block/water_still").unwrap();
    address_map.sprites.insert(
        water_loc,
        AtlasSpriteLocation {
            chunk_id: 1,
            category: "blocks".to_string(),
            pixel_rect: [0, 0, 16, 16],
            ..Default::default()
        },
    );

    let mut atlas = BakedAtlas {
        chunks: vec![BakedAtlasChunk {
            chunk_id: 1,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 16,
            height: 16,
            albedo: RgbaBuffer::new(16, 16),
            normal: None,
            specular: None,
            overlay: None,
            normal_gl: None,
            orm: None,
            aux: None,
        }],
        address_map,
    };

    let summary = decode_baked_atlas(&mut atlas).expect("decode atlas");
    assert_eq!(summary.chunks, 1);
    assert_eq!(summary.with_orm, 1);
    assert_eq!(summary.with_aux, 1);

    let chunk = &atlas.chunks[0];
    let aux = chunk.aux.as_ref().expect("aux buffer must be created");
    let orm = chunk.orm.as_ref().expect("orm buffer must be created");

    // Check water pixel: IOR encoded (12 for 1.333), transmission encoded (255 for 1.0)
    let aux_px = aux.get_pixel(0, 0);
    assert_eq!(aux_px[0], 12); // IOR 1.333
    assert_eq!(aux_px[3], 255); // Transmission 1.0

    // Check water roughness in ORM (5 for 0.02)
    let orm_px = orm.get_pixel(0, 0);
    assert_eq!(orm_px[1], 5); // Roughness 0.02

    let meta = &atlas.address_map.chunks[0];
    assert!(meta.has_orm);
    assert!(meta.has_aux);
    assert!(meta.pbr.is_some());
}

#[test]
fn decode_baked_atlas_injects_preset_for_glass_and_preserves_specular_when_present() {
    use mtk_resource::ResourceLocation;
    use mtk_texture::AtlasSpriteLocation;

    let mut address_map = AtlasAddressMap::new();
    address_map.chunks.push(AtlasChunkMeta {
        chunk_id: 2,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 16,
        height: 16,
        has_normal: false,
        has_specular: true,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    let glass_loc = ResourceLocation::parse("minecraft:block/glass").unwrap();
    address_map.sprites.insert(
        glass_loc,
        AtlasSpriteLocation {
            chunk_id: 2,
            category: "blocks".to_string(),
            pixel_rect: [0, 0, 16, 16],
            ..Default::default()
        },
    );

    // Specular map with custom smoothness: R=200 (smoothness), G=50 (F0), B=0, A=255
    let mut specular = RgbaBuffer::new(16, 16);
    specular.set_pixel(0, 0, [200, 50, 0, 255]);

    let mut atlas = BakedAtlas {
        chunks: vec![BakedAtlasChunk {
            chunk_id: 2,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 16,
            height: 16,
            albedo: RgbaBuffer::new(16, 16),
            normal: None,
            specular: Some(specular),
            overlay: None,
            normal_gl: None,
            orm: None,
            aux: None,
        }],
        address_map,
    };

    let summary = decode_baked_atlas(&mut atlas).expect("decode atlas");
    assert_eq!(summary.chunks, 1);
    assert_eq!(summary.with_orm, 1);
    assert_eq!(summary.with_aux, 1);

    let chunk = &atlas.chunks[0];
    let aux = chunk.aux.as_ref().expect("aux");
    let orm = chunk.orm.as_ref().expect("orm");

    // Transmission is injected from preset (255)
    let aux_px = aux.get_pixel(0, 0);
    assert_eq!(aux_px[3], 255);

    // Roughness is preserved from user specular map (255 - 200 = 55)
    let orm_px = orm.get_pixel(0, 0);
    assert_eq!(orm_px[1], 55);
}

#[test]
fn decode_baked_atlas_matches_ctm_glass_tiles_and_rules() {
    use mtk_resource::{BlockMatch, CtmMethod, CtmRule, ResourceLocation};
    use mtk_texture::AtlasSpriteLocation;

    let mut address_map = AtlasAddressMap::new();
    address_map.chunks.push(AtlasChunkMeta {
        chunk_id: 3,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 32,
        height: 16,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: false,
        pbr: None,
        textures: Vec::new(),
    });

    // Tile 1: path contains "glass", e.g. "optifine/ctm/glass/0"
    let ctm_glass_tile = ResourceLocation::parse("minecraft:optifine/ctm/glass/0").unwrap();
    address_map.sprites.insert(
        ctm_glass_tile,
        AtlasSpriteLocation {
            chunk_id: 3,
            category: "blocks".to_string(),
            pixel_rect: [0, 0, 16, 16],
            ..Default::default()
        },
    );

    // Tile 2: arbitrary numeric path "optifine/ctm/default/42", matched via CtmRule
    let numeric_tile = ResourceLocation::parse("minecraft:optifine/ctm/default/42").unwrap();
    address_map.sprites.insert(
        numeric_tile.clone(),
        AtlasSpriteLocation {
            chunk_id: 3,
            category: "blocks".to_string(),
            pixel_rect: [16, 0, 16, 16],
            ..Default::default()
        },
    );

    // Add CTM rule matching "glass" block and referencing numeric_tile
    address_map.ctm_rules.push(CtmRule {
        name: "glass_ctm".to_string(),
        source_path: "optifine/ctm/default/glass.properties".to_string(),
        priority: 0,
        method: CtmMethod::Full { inner_seams: false },
        tiles: vec![Some(numeric_tile)],
        match_blocks: vec![BlockMatch::parse_with_namespace("glass", "minecraft")],
        match_tiles: Vec::new(),
        connect_logic: mtk_resource::ConnectLogic::SameBlock,
        faces: mtk_core::DirMask::ALL,
        biomes: None,
        height_ranges: None,
        tint_index: None,
        tint_block: None,
    });

    let mut atlas = BakedAtlas {
        chunks: vec![BakedAtlasChunk {
            chunk_id: 3,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 32,
            height: 16,
            albedo: RgbaBuffer::new(32, 16),
            normal: None,
            specular: None,
            overlay: None,
            normal_gl: None,
            orm: None,
            aux: None,
        }],
        address_map,
    };

    let summary = decode_baked_atlas(&mut atlas).expect("decode atlas");
    assert_eq!(summary.chunks, 1);
    assert_eq!(summary.with_aux, 1);
    assert_eq!(summary.with_orm, 1);

    let chunk = &atlas.chunks[0];
    let aux = chunk.aux.as_ref().expect("aux");
    let orm = chunk.orm.as_ref().expect("orm");

    // Both tile 1 (path-matched) and tile 2 (rule-matched) must have transmission 255 and IOR 18
    assert_eq!(aux.get_pixel(0, 0)[3], 255); // Tile 1 transmission
    assert_eq!(aux.get_pixel(0, 0)[0], 18); // Tile 1 IOR
    assert_eq!(orm.get_pixel(0, 0)[1], 13); // Tile 1 roughness 0.05

    assert_eq!(aux.get_pixel(16, 0)[3], 255); // Tile 2 transmission
    assert_eq!(aux.get_pixel(16, 0)[0], 18); // Tile 2 IOR
    assert_eq!(orm.get_pixel(16, 0)[1], 13); // Tile 2 roughness 0.05
}
