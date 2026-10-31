//! Golden tests for the LabPBR 1.3 -> MtkPackedV1 decoder.

use mtk_material::pbr::{decode_ior, decode_labpbr13, PbrLayout, PbrSourceFormat};
use mtk_texture::RgbaBuffer;

fn buffer(width: u32, height: u32, pixels: &[[u8; 4]]) -> RgbaBuffer {
    assert_eq!(pixels.len() as u32, width * height);
    let mut buffer = RgbaBuffer::new(width, height);
    for (index, pixel) in pixels.iter().enumerate() {
        buffer.set_pixel(index as u32 % width, index as u32 / width, *pixel);
    }
    buffer
}

#[test]
fn missing_both_companions_is_empty() {
    let decoded = decode_labpbr13(None, None).expect("decode");
    assert!(decoded.is_empty());
    assert_eq!(decoded.meta.layout, PbrLayout::MtkPackedV1);
    assert_eq!(decoded.meta.source_format, PbrSourceFormat::LabPbr13);
    assert_eq!(decoded.meta.height_scale, 0.25);
}

#[test]
fn normal_only_reconstructs_gl_normal_and_default_orm() {
    let normal = buffer(2, 1, &[[128, 128, 255, 128], [255, 0, 0, 255]]);
    let decoded = decode_labpbr13(Some(&normal), None).expect("decode");
    let normal_gl = decoded.normal_gl.expect("normal_gl");
    let orm = decoded.orm.expect("orm");
    assert!(decoded.aux.is_none());

    assert_eq!(normal_gl.get_pixel(0, 0), [128, 127, 255, 128]);
    assert_eq!(normal_gl.get_pixel(1, 0), [255, 255, 128, 255]);

    // AO comes from _n.B; height is copied to normal_gl.A; default roughness 0.707.
    assert_eq!(orm.get_pixel(0, 0), [255, 180, 0, 0]);
    assert_eq!(orm.get_pixel(1, 0), [0, 180, 0, 0]);
}

#[test]
fn specular_only_unpacks_orm_and_aux() {
    let specular = buffer(2, 1, &[[0, 0, 0, 0], [128, 10, 64, 254]]);
    let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
    assert!(decoded.normal_gl.is_none());
    let orm = decoded.orm.expect("orm");
    let aux = decoded.aux.expect("aux");

    // No normal -> AO defaults to 255.
    assert_eq!(orm.get_pixel(0, 0), [255, 255, 0, 0]);
    assert_eq!(aux.get_pixel(0, 0), [0, 0, 0, 0]);

    assert_eq!(orm.get_pixel(1, 0), [255, 127, 0, 255]);
    // IOR from F0=10/255 ~= 1.494 -> encoded 18; porosity 64 -> 255; SSS 0.
    assert_eq!(aux.get_pixel(1, 0), [18, 255, 0, 0]);
    assert!((decode_ior(18) - 1.494).abs() < 0.01);
}

#[test]
fn both_companions_keep_ao_metal_and_sss() {
    let normal = buffer(1, 1, &[[128, 128, 255, 255]]);
    let specular = buffer(1, 1, &[[255, 230, 255, 128]]);
    let decoded = decode_labpbr13(Some(&normal), Some(&specular)).expect("decode");
    let orm = decoded.orm.expect("orm");
    let aux = decoded.aux.expect("aux");

    assert_eq!(orm.get_pixel(0, 0), [255, 0, 255, 129]);
    // Metal -> IOR 1.5; B=255 -> porosity 0, SSS 1.0.
    assert_eq!(aux.get_pixel(0, 0), [18, 0, 255, 0]);
}

#[test]
fn emission_255_means_no_data() {
    let specular = buffer(3, 1, &[[0, 0, 0, 255], [0, 0, 0, 254], [0, 0, 0, 128]]);
    let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
    let orm = decoded.orm.expect("orm");
    assert_eq!(orm.get_pixel(0, 0)[3], 0);
    assert_eq!(orm.get_pixel(1, 0)[3], 255);
    assert_eq!(orm.get_pixel(2, 0)[3], 129);
}

#[test]
fn porosity_and_sss_split_at_64_65() {
    let specular = buffer(3, 1, &[[0, 0, 64, 0], [0, 0, 65, 0], [0, 0, 255, 0]]);
    let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
    let aux = decoded.aux.expect("aux");

    assert_eq!(aux.get_pixel(0, 0)[1], 255);
    assert_eq!(aux.get_pixel(0, 0)[2], 0);

    assert_eq!(aux.get_pixel(1, 0)[1], 0);
    assert_eq!(aux.get_pixel(1, 0)[2], 0);

    assert_eq!(aux.get_pixel(2, 0)[1], 0);
    assert_eq!(aux.get_pixel(2, 0)[2], 255);
}

#[test]
fn ior_is_clamped_above_eight() {
    // F0 = 229/255 is the largest dielectric value and maps far above IOR 8.
    let specular = buffer(2, 1, &[[0, 229, 0, 0], [0, 230, 0, 0]]);
    let decoded = decode_labpbr13(None, Some(&specular)).expect("decode");
    let aux = decoded.aux.expect("aux");

    assert_eq!(aux.get_pixel(0, 0)[0], 255);
    assert!((decode_ior(aux.get_pixel(0, 0)[0]) - 8.0).abs() < 1e-5);
    // Metal preset keeps the neutral 1.5 IOR (8-bit encoded).
    assert!((decode_ior(aux.get_pixel(1, 0)[0]) - 1.5).abs() < 0.01);
}

#[test]
fn mismatched_dimensions_are_rejected() {
    let normal = RgbaBuffer::new(2, 1);
    let specular = RgbaBuffer::new(1, 1);
    let error = decode_labpbr13(Some(&normal), Some(&specular)).expect_err("must fail");
    let message = error.to_string();
    assert!(message.contains("2x1"), "{message}");
    assert!(message.contains("1x1"), "{message}");
}

#[test]
fn decode_baked_atlas_updates_mapping_metadata() {
    use mtk_material::pbr::decode_baked_atlas;
    use mtk_texture::{
        AtlasAddressMap, AtlasChunkMeta, BakedAtlas, BakedAtlasChunk, PbrLayout, PbrSourceFormat,
    };

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
    use mtk_material::pbr::decode_baked_atlas;
    use mtk_texture::{AtlasAddressMap, AtlasChunkMeta, BakedAtlas, BakedAtlasChunk};

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
