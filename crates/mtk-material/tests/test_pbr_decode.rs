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
