use super::*;

#[test]
fn test_build_texture_slots() {
    let mut meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 1024,
        height: 1024,
        has_normal: true,
        has_specular: false,
        has_overlay: true,
        has_orm: true,
        has_aux: false,
        has_tint: true,
        pbr: None,
        textures: Vec::new(),
    };

    meta.refresh_textures();
    assert_eq!(meta.textures.len(), 4); // albedo, normal, orm, overlay

    let albedo = meta.textures.iter().find(|s| s.role == "albedo").unwrap();
    assert_eq!(albedo.file_name, "blocks_chunk_001.png");
    assert_eq!(albedo.colorspace, "sRGB");

    let normal = meta.textures.iter().find(|s| s.role == "normal").unwrap();
    assert_eq!(normal.file_name, "blocks_chunk_001_n.png");
    assert_eq!(normal.colorspace, "Non-Color");

    let orm = meta.textures.iter().find(|s| s.role == "orm").unwrap();
    assert_eq!(orm.file_name, "blocks_chunk_001_orm.png");
    assert_eq!(orm.colorspace, "Non-Color");

    let overlay = meta.textures.iter().find(|s| s.role == "overlay").unwrap();
    assert_eq!(overlay.file_name, "blocks_chunk_001_overlay.png");
    assert_eq!(overlay.colorspace, "sRGB");
}
